use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{errors::AppError, models::DeploymentManifest};

#[derive(Debug, Clone)]
pub struct ConflictStore {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct StoredConflictSubject {
    pub profile_id: Uuid,
    pub mod_name: String,
    pub load_order: u32,
    pub manifest: DeploymentManifest,
}

impl ConflictStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn active_subjects(&self) -> Result<(Uuid, Vec<StoredConflictSubject>), AppError> {
        let mut transaction = self.pool.begin().await?;
        let profile_value: String =
            sqlx::query_scalar("SELECT active_profile_id FROM app_state WHERE singleton = 1")
                .fetch_one(&mut *transaction)
                .await?;
        let profile_id = parse_uuid(&profile_value, "active profile")?;
        let rows = sqlx::query(
            "SELECT
                pm.mod_id,
                pm.load_order,
                COALESCE(NULLIF(TRIM(l.display_name_override), ''), a.name, m.logical_id) AS mod_name,
                dr.id AS deployment_id,
                dr.profile_id AS deployment_profile_id,
                dr.mod_id AS deployment_mod_id,
                dr.strategy_id,
                dr.destination_root,
                dr.manifest_json
             FROM profile_mods pm
             JOIN mods m ON m.id = pm.mod_id
             LEFT JOIN mod_author_metadata a ON a.mod_id = pm.mod_id
             LEFT JOIN mod_local_metadata l ON l.mod_id = pm.mod_id
             LEFT JOIN deployment_records dr
                ON dr.profile_id = pm.profile_id AND dr.mod_id = pm.mod_id
             WHERE pm.profile_id = ? AND pm.enabled = 1
             ORDER BY pm.load_order ASC, pm.mod_id ASC",
        )
        .bind(profile_id.to_string())
        .fetch_all(&mut *transaction)
        .await?;
        transaction.commit().await?;

        let mut subjects = Vec::with_capacity(rows.len());
        for row in rows {
            let mod_value: String = row.try_get("mod_id")?;
            let mod_id = parse_uuid(&mod_value, "enabled mod")?;
            let load_order_value: i64 = row.try_get("load_order")?;
            let load_order = u32::try_from(load_order_value).map_err(|_| {
                AppError::DataIntegrity(format!("模组 {mod_id} 的 Profile 加载顺序超出支持范围。"))
            })?;
            let deployment_id: Option<String> = row.try_get("deployment_id")?;
            let deployment_profile: Option<String> = row.try_get("deployment_profile_id")?;
            let deployment_mod: Option<String> = row.try_get("deployment_mod_id")?;
            let strategy_id: Option<String> = row.try_get("strategy_id")?;
            let destination_root: Option<String> = row.try_get("destination_root")?;
            let manifest_json: Option<String> = row.try_get("manifest_json")?;
            let (
                deployment_id,
                deployment_profile,
                deployment_mod,
                strategy_id,
                destination_root,
                manifest_json,
            ) = match (
                deployment_id,
                deployment_profile,
                deployment_mod,
                strategy_id,
                destination_root,
                manifest_json,
            ) {
                (Some(id), Some(profile), Some(mod_id), Some(strategy), Some(root), Some(json)) => {
                    (id, profile, mod_id, strategy, root, json)
                }
                _ => {
                    return Err(AppError::DataIntegrity(format!(
                        "已启用模组 {mod_id} 缺少部署记录。"
                    )));
                }
            };
            let manifest: DeploymentManifest =
                serde_json::from_str(&manifest_json).map_err(AppError::ConfigFormat)?;
            if manifest.id != parse_uuid(&deployment_id, "deployment")?
                || manifest.profile_id != parse_uuid(&deployment_profile, "deployment profile")?
                || manifest.mod_id != parse_uuid(&deployment_mod, "deployment mod")?
                || manifest.profile_id != profile_id
                || manifest.mod_id != mod_id
                || manifest.strategy_id != strategy_id
                || !storage_path(&manifest.destination_root)
                    .eq_ignore_ascii_case(&destination_root.replace('\\', "/"))
            {
                return Err(AppError::DataIntegrity(format!(
                    "模组 {mod_id} 的部署记录列与清单 JSON 不一致。"
                )));
            }
            subjects.push(StoredConflictSubject {
                profile_id,
                mod_name: row.try_get("mod_name")?,
                load_order,
                manifest,
            });
        }
        Ok((profile_id, subjects))
    }
}

fn parse_uuid(value: &str, label: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(value)
        .map_err(|_| AppError::DataIntegrity(format!("{label} UUID 无效：{value}")))
}

fn storage_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use uuid::Uuid;

    use crate::{
        database::{ConflictStore, Database, DeploymentStore},
        models::{DeploymentEntry, DeploymentManifest},
    };

    #[tokio::test]
    async fn loads_enabled_subjects_in_profile_order() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let database = Database::connect(&directory.path().join("mods.db")).await?;
        let deployment_store = DeploymentStore::new(database.pool().clone());
        let profile_id = deployment_store.active_profile_id().await?;
        let mut manifests = Vec::new();
        for (index, name) in ["First", "Second"].into_iter().enumerate() {
            let mod_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO mods
                    (id, logical_id, repository_path, content_fingerprint, size_bytes,
                     installed_at, updated_at, lifecycle_state)
                 VALUES (?, ?, ?, 'fingerprint', 1, 1, 1, 'installed')",
            )
            .bind(mod_id.to_string())
            .bind(format!("fixture.{index}"))
            .bind(format!("fixture-{index}"))
            .execute(database.pool())
            .await?;
            sqlx::query(
                "INSERT INTO mod_author_metadata (mod_id, name, source_kind)
                 VALUES (?, ?, 'inferred')",
            )
            .bind(mod_id.to_string())
            .bind(name)
            .execute(database.pool())
            .await?;
            manifests.push(DeploymentManifest {
                schema_version: 1,
                id: Uuid::new_v4(),
                profile_id,
                mod_id,
                strategy_id: "efmi.copy.v1".to_owned(),
                destination_root: PathBuf::from(r"C:\EFMI\Mods"),
                destination_directory: PathBuf::from(format!("AEMM_{}", mod_id.simple())),
                source_content_fingerprint: "fingerprint".to_owned(),
                entries: vec![DeploymentEntry {
                    source_relative: PathBuf::from("mod.ini"),
                    destination_relative: PathBuf::from("mod.ini"),
                    size_bytes: 1,
                    content_hash: "a".repeat(64),
                }],
                created_at: i64::try_from(index)?,
            });
        }
        deployment_store
            .save_enabled(profile_id, &manifests)
            .await?;

        let (stored_profile, subjects) = ConflictStore::new(database.pool().clone())
            .active_subjects()
            .await?;
        assert_eq!(stored_profile, profile_id);
        assert_eq!(subjects.len(), 2);
        assert_eq!(subjects[0].mod_name, "First");
        assert_eq!(subjects[0].load_order, 0);
        assert_eq!(subjects[1].mod_name, "Second");
        assert_eq!(subjects[1].load_order, 1);
        Ok(())
    }
}
