use std::{path::PathBuf, time::SystemTime};

use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    errors::AppError,
    models::{DeploymentManifest, ModLifecycleState},
};

#[derive(Debug, Clone)]
pub struct DeploymentStore {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct StoredDeploymentSource {
    pub mod_id: Uuid,
    pub repository_path: PathBuf,
    pub content_fingerprint: String,
    pub lifecycle_state: ModLifecycleState,
}

impl DeploymentStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn active_profile_id(&self) -> Result<Uuid, AppError> {
        let value: String =
            sqlx::query_scalar("SELECT active_profile_id FROM app_state WHERE singleton = 1")
                .fetch_one(&self.pool)
                .await?;
        parse_uuid(&value, "active profile")
    }

    pub async fn deployment_source(
        &self,
        mod_id: Uuid,
    ) -> Result<StoredDeploymentSource, AppError> {
        let row = sqlx::query(
            "SELECT id, repository_path, content_fingerprint, lifecycle_state
             FROM mods WHERE id = ?",
        )
        .bind(mod_id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotAvailable(format!("模组 {mod_id} 不存在。")))?;
        let stored_id: String = row.try_get("id")?;
        let lifecycle: String = row.try_get("lifecycle_state")?;
        Ok(StoredDeploymentSource {
            mod_id: parse_uuid(&stored_id, "mod")?,
            repository_path: PathBuf::from(row.try_get::<String, _>("repository_path")?),
            content_fingerprint: row
                .try_get::<Option<String>, _>("content_fingerprint")?
                .ok_or_else(|| {
                    AppError::DataIntegrity(format!("模组 {mod_id} 缺少内容指纹，请重新扫描仓库。"))
                })?,
            lifecycle_state: lifecycle_from_database(&lifecycle)?,
        })
    }

    pub async fn manifest(
        &self,
        profile_id: Uuid,
        mod_id: Uuid,
    ) -> Result<Option<DeploymentManifest>, AppError> {
        let row = sqlx::query(
            "SELECT id, profile_id, mod_id, strategy_id, destination_root, manifest_json
             FROM deployment_records WHERE profile_id = ? AND mod_id = ?",
        )
        .bind(profile_id.to_string())
        .bind(mod_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| row_to_manifest(&row)).transpose()
    }

    pub async fn all_manifests(&self) -> Result<Vec<DeploymentManifest>, AppError> {
        let rows = sqlx::query(
            "SELECT id, profile_id, mod_id, strategy_id, destination_root, manifest_json
             FROM deployment_records ORDER BY created_at ASC, id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_manifest).collect()
    }

    pub async fn save_enabled(
        &self,
        profile_id: Uuid,
        manifests: &[DeploymentManifest],
    ) -> Result<(), AppError> {
        if manifests.is_empty() {
            return Ok(());
        }
        let now = unix_timestamp_seconds()?;
        let mut transaction = self.pool.begin().await?;
        let active_profile: String =
            sqlx::query_scalar("SELECT active_profile_id FROM app_state WHERE singleton = 1")
                .fetch_one(&mut *transaction)
                .await?;
        if parse_uuid(&active_profile, "active profile")? != profile_id {
            return Err(AppError::DataIntegrity(
                "部署期间当前 Profile 已变化，已取消数据库提交。".to_owned(),
            ));
        }
        let mut next_order: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(load_order), -1) + 1 FROM profile_mods WHERE profile_id = ?",
        )
        .bind(profile_id.to_string())
        .fetch_one(&mut *transaction)
        .await?;

        for manifest in manifests {
            validate_manifest_identity(manifest, profile_id)?;
            let lifecycle: Option<String> =
                sqlx::query_scalar("SELECT lifecycle_state FROM mods WHERE id = ?")
                    .bind(manifest.mod_id.to_string())
                    .fetch_optional(&mut *transaction)
                    .await?;
            if lifecycle.as_deref() != Some("installed") {
                return Err(AppError::NotAvailable(format!(
                    "模组 {} 当前不可部署。",
                    manifest.mod_id
                )));
            }
            let existing_order: Option<i64> = sqlx::query_scalar(
                "SELECT load_order FROM profile_mods WHERE profile_id = ? AND mod_id = ?",
            )
            .bind(profile_id.to_string())
            .bind(manifest.mod_id.to_string())
            .fetch_optional(&mut *transaction)
            .await?;
            let load_order = match existing_order {
                Some(value) => value,
                None => {
                    let current = next_order;
                    next_order = next_order.checked_add(1).ok_or_else(|| {
                        AppError::DataIntegrity("Profile 加载顺序超过 SQLite 支持范围。".to_owned())
                    })?;
                    current
                }
            };
            sqlx::query(
                "INSERT INTO profile_mods (profile_id, mod_id, enabled, load_order)
                 VALUES (?, ?, 1, ?)
                 ON CONFLICT(profile_id, mod_id)
                 DO UPDATE SET enabled = 1",
            )
            .bind(profile_id.to_string())
            .bind(manifest.mod_id.to_string())
            .bind(load_order)
            .execute(&mut *transaction)
            .await?;

            let manifest_json = serde_json::to_string(manifest).map_err(AppError::ConfigFormat)?;
            let result = sqlx::query(
                "INSERT INTO deployment_records
                    (id, profile_id, mod_id, strategy_id, destination_root, manifest_json, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(manifest.id.to_string())
            .bind(profile_id.to_string())
            .bind(manifest.mod_id.to_string())
            .bind(&manifest.strategy_id)
            .bind(storage_path(&manifest.destination_root))
            .bind(manifest_json)
            .bind(manifest.created_at)
            .execute(&mut *transaction)
            .await;
            if let Err(error) = result {
                return Err(AppError::Database(error));
            }
        }
        sqlx::query("UPDATE profiles SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(profile_id.to_string())
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn save_disabled(
        &self,
        profile_id: Uuid,
        manifests: &[DeploymentManifest],
    ) -> Result<(), AppError> {
        if manifests.is_empty() {
            return Ok(());
        }
        let now = unix_timestamp_seconds()?;
        let mut transaction = self.pool.begin().await?;
        let active_profile: String =
            sqlx::query_scalar("SELECT active_profile_id FROM app_state WHERE singleton = 1")
                .fetch_one(&mut *transaction)
                .await?;
        if parse_uuid(&active_profile, "active profile")? != profile_id {
            return Err(AppError::DataIntegrity(
                "撤销期间当前 Profile 已变化，已取消数据库提交。".to_owned(),
            ));
        }
        for manifest in manifests {
            validate_manifest_identity(manifest, profile_id)?;
            let deleted = sqlx::query(
                "DELETE FROM deployment_records
                 WHERE id = ? AND profile_id = ? AND mod_id = ?",
            )
            .bind(manifest.id.to_string())
            .bind(profile_id.to_string())
            .bind(manifest.mod_id.to_string())
            .execute(&mut *transaction)
            .await?;
            if deleted.rows_affected() != 1 {
                return Err(AppError::DataIntegrity(format!(
                    "模组 {} 的部署记录在撤销期间发生变化。",
                    manifest.mod_id
                )));
            }
            let updated = sqlx::query(
                "UPDATE profile_mods SET enabled = 0
                 WHERE profile_id = ? AND mod_id = ?",
            )
            .bind(profile_id.to_string())
            .bind(manifest.mod_id.to_string())
            .execute(&mut *transaction)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(AppError::DataIntegrity(format!(
                    "模组 {} 缺少 Profile 状态记录。",
                    manifest.mod_id
                )));
            }
        }
        sqlx::query("UPDATE profiles SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(profile_id.to_string())
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }
}

fn row_to_manifest(row: &sqlx::sqlite::SqliteRow) -> Result<DeploymentManifest, AppError> {
    let id: String = row.try_get("id")?;
    let profile_id: String = row.try_get("profile_id")?;
    let mod_id: String = row.try_get("mod_id")?;
    let strategy_id: String = row.try_get("strategy_id")?;
    let destination_root: String = row.try_get("destination_root")?;
    let manifest_json: String = row.try_get("manifest_json")?;
    let manifest: DeploymentManifest =
        serde_json::from_str(&manifest_json).map_err(AppError::ConfigFormat)?;
    if manifest.id != parse_uuid(&id, "deployment")?
        || manifest.profile_id != parse_uuid(&profile_id, "deployment profile")?
        || manifest.mod_id != parse_uuid(&mod_id, "deployment mod")?
        || manifest.strategy_id != strategy_id
        || !paths_equal_storage(&manifest.destination_root, &destination_root)
    {
        return Err(AppError::DataIntegrity(
            "部署记录列与清单 JSON 不一致。".to_owned(),
        ));
    }
    Ok(manifest)
}

fn validate_manifest_identity(
    manifest: &DeploymentManifest,
    profile_id: Uuid,
) -> Result<(), AppError> {
    if manifest.profile_id != profile_id {
        return Err(AppError::DataIntegrity(
            "部署清单不属于当前 Profile。".to_owned(),
        ));
    }
    Ok(())
}

fn parse_uuid(value: &str, label: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(value)
        .map_err(|_| AppError::DataIntegrity(format!("{label} UUID 无效：{value}")))
}

fn lifecycle_from_database(value: &str) -> Result<ModLifecycleState, AppError> {
    match value {
        "installing" => Ok(ModLifecycleState::Installing),
        "installed" => Ok(ModLifecycleState::Installed),
        "broken" => Ok(ModLifecycleState::Broken),
        "removing" => Ok(ModLifecycleState::Removing),
        _ => Err(AppError::DataIntegrity(format!(
            "未知的模组生命周期状态：{value}"
        ))),
    }
}

fn storage_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn paths_equal_storage(path: &std::path::Path, stored: &str) -> bool {
    storage_path(path).eq_ignore_ascii_case(&stored.replace('\\', "/"))
}

fn unix_timestamp_seconds() -> Result<i64, AppError> {
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| AppError::DataIntegrity("系统时间早于 Unix Epoch。".to_owned()))?;
    i64::try_from(duration.as_secs())
        .map_err(|_| AppError::DataIntegrity("系统时间超出支持范围。".to_owned()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use uuid::Uuid;

    use crate::{
        database::{Database, DeploymentStore},
        models::{DeploymentEntry, DeploymentManifest},
    };

    #[tokio::test]
    async fn persists_enabled_and_disabled_state_atomically()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let database = Database::connect(&directory.path().join("mods.db")).await?;
        let mod_id = Uuid::new_v4();
        let now = 1_i64;
        sqlx::query(
            "INSERT INTO mods
                (id, logical_id, repository_path, content_fingerprint, size_bytes,
                 installed_at, updated_at, lifecycle_state)
             VALUES (?, 'fixture.mod', 'fixture', 'fingerprint', 1, ?, ?, 'installed')",
        )
        .bind(mod_id.to_string())
        .bind(now)
        .bind(now)
        .execute(database.pool())
        .await?;
        let store = DeploymentStore::new(database.pool().clone());
        let profile_id = store.active_profile_id().await?;
        let manifest = DeploymentManifest {
            schema_version: 1,
            id: Uuid::new_v4(),
            profile_id,
            mod_id,
            strategy_id: "efmi.copy.v1".to_owned(),
            destination_root: PathBuf::from(r"C:\EFMI\Mods"),
            destination_directory: PathBuf::from(format!("AEMM_{}", mod_id.simple())),
            source_content_fingerprint: "fingerprint".to_owned(),
            entries: vec![DeploymentEntry {
                source_relative: PathBuf::from("main.ini"),
                destination_relative: PathBuf::from("main.ini"),
                size_bytes: 1,
                content_hash: "a".repeat(64),
            }],
            created_at: now,
        };

        store
            .save_enabled(profile_id, std::slice::from_ref(&manifest))
            .await?;
        assert_eq!(
            store.manifest(profile_id, mod_id).await?,
            Some(manifest.clone())
        );
        store
            .save_disabled(profile_id, std::slice::from_ref(&manifest))
            .await?;
        assert!(store.manifest(profile_id, mod_id).await?.is_none());
        let enabled: i64 = sqlx::query_scalar(
            "SELECT enabled FROM profile_mods WHERE profile_id = ? AND mod_id = ?",
        )
        .bind(profile_id.to_string())
        .bind(mod_id.to_string())
        .fetch_one(database.pool())
        .await?;
        assert_eq!(enabled, 0);
        Ok(())
    }
}
