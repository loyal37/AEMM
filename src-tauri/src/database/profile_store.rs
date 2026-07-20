use std::{
    collections::{HashMap, HashSet},
    time::SystemTime,
};

use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    core::profiles::ProfileSwitchPlan,
    errors::AppError,
    models::{DeploymentManifest, Profile, ProfileMod},
};

#[derive(Debug, Clone)]
pub struct ProfileStore {
    pool: SqlitePool,
}

impl ProfileStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<Profile>, AppError> {
        let mut transaction = self.pool.begin().await?;
        let active: String =
            sqlx::query_scalar("SELECT active_profile_id FROM app_state WHERE singleton = 1")
                .fetch_one(&mut *transaction)
                .await?;
        let active_id = parse_uuid(&active, "active profile")?;
        let rows = sqlx::query(
            "SELECT id, name, created_at, updated_at
             FROM profiles
             ORDER BY CASE WHEN id = ? THEN 0 ELSE 1 END, updated_at DESC, name COLLATE NOCASE",
        )
        .bind(active_id.to_string())
        .fetch_all(&mut *transaction)
        .await?;

        let mut profiles = Vec::with_capacity(rows.len());
        let mut indexes = HashMap::with_capacity(rows.len());
        for row in rows {
            let id = parse_uuid(&row.try_get::<String, _>("id")?, "profile")?;
            indexes.insert(id, profiles.len());
            profiles.push(Profile {
                id,
                name: row.try_get("name")?,
                is_active: id == active_id,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                mods: Vec::new(),
            });
        }

        let mod_rows = sqlx::query(
            "SELECT pm.profile_id, pm.mod_id, pm.enabled, pm.load_order,
                    COALESCE(NULLIF(TRIM(l.display_name_override), ''), a.name, m.logical_id) AS mod_name
             FROM profile_mods pm
             JOIN mods m ON m.id = pm.mod_id
             LEFT JOIN mod_author_metadata a ON a.mod_id = m.id
             LEFT JOIN mod_local_metadata l ON l.mod_id = m.id
             ORDER BY pm.profile_id, pm.load_order, pm.mod_id",
        )
        .fetch_all(&mut *transaction)
        .await?;
        for row in mod_rows {
            let profile_id = parse_uuid(&row.try_get::<String, _>("profile_id")?, "profile")?;
            let Some(index) = indexes.get(&profile_id).copied() else {
                return Err(AppError::DataIntegrity(
                    "Profile 模组记录引用了不存在的 Profile。".to_owned(),
                ));
            };
            profiles[index].mods.push(ProfileMod {
                mod_id: parse_uuid(&row.try_get::<String, _>("mod_id")?, "mod")?,
                mod_name: row.try_get("mod_name")?,
                enabled: row.try_get::<i64, _>("enabled")? == 1,
                load_order: to_u32(row.try_get("load_order")?, "Profile load order")?,
            });
        }
        transaction.commit().await?;
        Ok(profiles)
    }

    pub async fn get(&self, profile_id: Uuid) -> Result<Profile, AppError> {
        self.list()
            .await?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| AppError::Profile(format!("Profile {profile_id} 不存在。")))
    }

    pub async fn create(&self, profile_id: Uuid, name: &str) -> Result<Profile, AppError> {
        let now = unix_timestamp_seconds()?;
        let mut transaction = self.pool.begin().await?;
        ensure_name_available(&mut transaction, name, None).await?;
        sqlx::query("INSERT INTO profiles (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(profile_id.to_string())
            .bind(name)
            .bind(now)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        self.get(profile_id).await
    }

    pub async fn rename(&self, profile_id: Uuid, name: &str) -> Result<Profile, AppError> {
        let now = unix_timestamp_seconds()?;
        let mut transaction = self.pool.begin().await?;
        ensure_profile_exists(&mut transaction, profile_id).await?;
        ensure_name_available(&mut transaction, name, Some(profile_id)).await?;
        let result = sqlx::query("UPDATE profiles SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(now)
            .bind(profile_id.to_string())
            .execute(&mut *transaction)
            .await?;
        if result.rows_affected() != 1 {
            return Err(AppError::DataIntegrity(
                "重命名 Profile 时记录发生变化。".to_owned(),
            ));
        }
        transaction.commit().await?;
        self.get(profile_id).await
    }

    pub async fn copy(
        &self,
        source_profile_id: Uuid,
        target_profile_id: Uuid,
        name: &str,
    ) -> Result<Profile, AppError> {
        let now = unix_timestamp_seconds()?;
        let mut transaction = self.pool.begin().await?;
        ensure_profile_exists(&mut transaction, source_profile_id).await?;
        ensure_name_available(&mut transaction, name, None).await?;
        sqlx::query("INSERT INTO profiles (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(target_profile_id.to_string())
            .bind(name)
            .bind(now)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, load_order)
             SELECT ?, mod_id, enabled, load_order
             FROM profile_mods WHERE profile_id = ? ORDER BY load_order",
        )
        .bind(target_profile_id.to_string())
        .bind(source_profile_id.to_string())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.get(target_profile_id).await
    }

    pub async fn delete(&self, profile_id: Uuid) -> Result<(), AppError> {
        let mut transaction = self.pool.begin().await?;
        ensure_profile_exists(&mut transaction, profile_id).await?;
        let active: String =
            sqlx::query_scalar("SELECT active_profile_id FROM app_state WHERE singleton = 1")
                .fetch_one(&mut *transaction)
                .await?;
        if parse_uuid(&active, "active profile")? == profile_id {
            return Err(AppError::Profile(
                "不能删除当前正在使用的 Profile，请先切换到其他配置。".to_owned(),
            ));
        }
        let deployments: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM deployment_records WHERE profile_id = ?")
                .bind(profile_id.to_string())
                .fetch_one(&mut *transaction)
                .await?;
        if deployments != 0 {
            return Err(AppError::DataIntegrity(
                "非活动 Profile 意外包含部署记录，拒绝删除。".to_owned(),
            ));
        }
        let result = sqlx::query("DELETE FROM profiles WHERE id = ?")
            .bind(profile_id.to_string())
            .execute(&mut *transaction)
            .await?;
        if result.rows_affected() != 1 {
            return Err(AppError::DataIntegrity(
                "删除 Profile 时记录发生变化。".to_owned(),
            ));
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn reorder_enabled(
        &self,
        profile_id: Uuid,
        ordered_mod_ids: &[Uuid],
    ) -> Result<Profile, AppError> {
        const MAX_REORDERED_MODS: usize = 2_000;
        if ordered_mod_ids.len() > MAX_REORDERED_MODS {
            return Err(AppError::Profile(format!(
                "单个 Profile 最多调整 {MAX_REORDERED_MODS} 个启用模组。"
            )));
        }
        let requested = ordered_mod_ids.iter().copied().collect::<HashSet<_>>();
        if requested.len() != ordered_mod_ids.len() {
            return Err(AppError::Profile("加载顺序包含重复模组。".to_owned()));
        }

        let now = unix_timestamp_seconds()?;
        let mut transaction = self.pool.begin().await?;
        ensure_profile_exists(&mut transaction, profile_id).await?;
        let rows = sqlx::query(
            "SELECT mod_id, enabled, load_order FROM profile_mods
             WHERE profile_id = ? ORDER BY load_order, mod_id",
        )
        .bind(profile_id.to_string())
        .fetch_all(&mut *transaction)
        .await?;
        let mut all = Vec::with_capacity(rows.len());
        let mut enabled_ids = Vec::new();
        let mut enabled_slots = Vec::new();
        let mut maximum_order = 0_i64;
        for row in rows {
            let mod_id = parse_uuid(&row.try_get::<String, _>("mod_id")?, "mod")?;
            let enabled = row.try_get::<i64, _>("enabled")? == 1;
            let load_order: i64 = row.try_get("load_order")?;
            maximum_order = maximum_order.max(load_order);
            if enabled {
                enabled_ids.push(mod_id);
                enabled_slots.push(load_order);
            }
            all.push((mod_id, enabled, load_order));
        }
        if enabled_ids.len() != ordered_mod_ids.len()
            || enabled_ids.iter().copied().collect::<HashSet<_>>() != requested
        {
            return Err(AppError::Profile(
                "提交的加载顺序必须恰好包含该 Profile 当前全部启用模组。".to_owned(),
            ));
        }
        if enabled_ids == ordered_mod_ids {
            transaction.commit().await?;
            return self.get(profile_id).await;
        }

        let temporary_base = maximum_order.checked_add(1).ok_or_else(|| {
            AppError::DataIntegrity("Profile 临时加载顺序超出支持范围。".to_owned())
        })?;
        let temporary_count = i64::try_from(all.len()).map_err(|_| {
            AppError::DataIntegrity("Profile 模组数量超出 SQLite 支持范围。".to_owned())
        })?;
        temporary_base
            .checked_add(temporary_count.saturating_sub(1))
            .ok_or_else(|| {
                AppError::DataIntegrity("Profile 临时加载顺序超出支持范围。".to_owned())
            })?;
        for (index, (mod_id, _, _)) in all.iter().enumerate() {
            let temporary_order = temporary_base
                .checked_add(i64::try_from(index).map_err(|_| {
                    AppError::DataIntegrity("Profile 模组数量超出 SQLite 支持范围。".to_owned())
                })?)
                .ok_or_else(|| {
                    AppError::DataIntegrity("Profile 临时加载顺序超出支持范围。".to_owned())
                })?;
            sqlx::query(
                "UPDATE profile_mods SET load_order = ? WHERE profile_id = ? AND mod_id = ?",
            )
            .bind(temporary_order)
            .bind(profile_id.to_string())
            .bind(mod_id.to_string())
            .execute(&mut *transaction)
            .await?;
        }

        let enabled_positions = ordered_mod_ids
            .iter()
            .copied()
            .zip(enabled_slots)
            .collect::<HashMap<_, _>>();
        for (mod_id, enabled, original_order) in all {
            let load_order = if enabled {
                enabled_positions.get(&mod_id).copied().ok_or_else(|| {
                    AppError::DataIntegrity("加载顺序映射缺少启用模组。".to_owned())
                })?
            } else {
                original_order
            };
            let updated = sqlx::query(
                "UPDATE profile_mods SET load_order = ? WHERE profile_id = ? AND mod_id = ?",
            )
            .bind(load_order)
            .bind(profile_id.to_string())
            .bind(mod_id.to_string())
            .execute(&mut *transaction)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(AppError::DataIntegrity(
                    "更新 Profile 加载顺序时记录发生变化。".to_owned(),
                ));
            }
        }
        sqlx::query("UPDATE profiles SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(profile_id.to_string())
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        self.get(profile_id).await
    }

    pub async fn prepare_switch(
        &self,
        target_profile_id: Uuid,
    ) -> Result<ProfileSwitchPlan, AppError> {
        let mut transaction = self.pool.begin().await?;
        let active: String =
            sqlx::query_scalar("SELECT active_profile_id FROM app_state WHERE singleton = 1")
                .fetch_one(&mut *transaction)
                .await?;
        let source_profile_id = parse_uuid(&active, "active profile")?;
        ensure_profile_exists(&mut transaction, target_profile_id).await?;

        if source_profile_id == target_profile_id {
            transaction.commit().await?;
            return Ok(ProfileSwitchPlan {
                source_profile_id,
                target_profile_id,
                source_manifests: Vec::new(),
                target_mod_ids: Vec::new(),
                warnings: vec!["所选 Profile 已经处于活动状态。".to_owned()],
            });
        }

        let source_rows = sqlx::query(
            "SELECT pm.mod_id, dr.manifest_json
             FROM profile_mods pm
             LEFT JOIN deployment_records dr
               ON dr.profile_id = pm.profile_id AND dr.mod_id = pm.mod_id
             WHERE pm.profile_id = ? AND pm.enabled = 1
             ORDER BY pm.load_order, pm.mod_id",
        )
        .bind(source_profile_id.to_string())
        .fetch_all(&mut *transaction)
        .await?;
        let mut source_manifests = Vec::with_capacity(source_rows.len());
        for row in source_rows {
            let mod_id = parse_uuid(&row.try_get::<String, _>("mod_id")?, "mod")?;
            let json = row
                .try_get::<Option<String>, _>("manifest_json")?
                .ok_or_else(|| {
                    AppError::DataIntegrity(format!(
                        "活动 Profile 中已启用模组 {mod_id} 缺少部署记录。"
                    ))
                })?;
            source_manifests.push(parse_manifest(&json, source_profile_id, mod_id)?);
        }
        let source_deployment_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM deployment_records WHERE profile_id = ?")
                .bind(source_profile_id.to_string())
                .fetch_one(&mut *transaction)
                .await?;
        if source_deployment_count
            != i64::try_from(source_manifests.len()).map_err(|_| {
                AppError::DataIntegrity("活动部署数量超出 SQLite 支持范围。".to_owned())
            })?
        {
            return Err(AppError::DataIntegrity(
                "活动 Profile 的启用状态与部署记录不一致。".to_owned(),
            ));
        }

        let target_deployment_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM deployment_records WHERE profile_id = ?")
                .bind(target_profile_id.to_string())
                .fetch_one(&mut *transaction)
                .await?;
        if target_deployment_count != 0 {
            return Err(AppError::DataIntegrity(
                "非活动目标 Profile 意外包含部署记录。".to_owned(),
            ));
        }

        let target_rows = sqlx::query(
            "SELECT pm.mod_id, m.lifecycle_state, m.content_fingerprint
             FROM profile_mods pm
             JOIN mods m ON m.id = pm.mod_id
             WHERE pm.profile_id = ? AND pm.enabled = 1
             ORDER BY pm.load_order, pm.mod_id",
        )
        .bind(target_profile_id.to_string())
        .fetch_all(&mut *transaction)
        .await?;
        let mut target_mod_ids = Vec::with_capacity(target_rows.len());
        for row in target_rows {
            let mod_id = parse_uuid(&row.try_get::<String, _>("mod_id")?, "mod")?;
            let lifecycle: String = row.try_get("lifecycle_state")?;
            let fingerprint: Option<String> = row.try_get("content_fingerprint")?;
            if lifecycle != "installed" || fingerprint.as_deref().is_none_or(str::is_empty) {
                return Err(AppError::Profile(format!(
                    "目标 Profile 引用的模组 {mod_id} 当前不可部署，请先重新扫描或修复仓库。"
                )));
            }
            target_mod_ids.push(mod_id);
        }
        transaction.commit().await?;
        Ok(ProfileSwitchPlan {
            source_profile_id,
            target_profile_id,
            source_manifests,
            target_mod_ids,
            warnings: Vec::new(),
        })
    }

    pub async fn commit_switch(
        &self,
        plan: &ProfileSwitchPlan,
        target_manifests: &[DeploymentManifest],
    ) -> Result<(), AppError> {
        if plan.source_profile_id == plan.target_profile_id {
            return Ok(());
        }
        if target_manifests.len() != plan.target_mod_ids.len() {
            return Err(AppError::DataIntegrity(
                "目标部署结果与 Profile 计划数量不一致。".to_owned(),
            ));
        }
        for (manifest, mod_id) in target_manifests.iter().zip(&plan.target_mod_ids) {
            if manifest.profile_id != plan.target_profile_id || manifest.mod_id != *mod_id {
                return Err(AppError::DataIntegrity(
                    "目标部署清单与 Profile 切换计划不一致。".to_owned(),
                ));
            }
        }

        let now = unix_timestamp_seconds()?;
        let mut transaction = self.pool.begin().await?;
        let active: String =
            sqlx::query_scalar("SELECT active_profile_id FROM app_state WHERE singleton = 1")
                .fetch_one(&mut *transaction)
                .await?;
        if parse_uuid(&active, "active profile")? != plan.source_profile_id {
            return Err(AppError::DataIntegrity(
                "提交切换前活动 Profile 已发生变化。".to_owned(),
            ));
        }

        let rows = sqlx::query(
            "SELECT mod_id, manifest_json FROM deployment_records
             WHERE profile_id = ? ORDER BY created_at, id",
        )
        .bind(plan.source_profile_id.to_string())
        .fetch_all(&mut *transaction)
        .await?;
        let mut current = HashMap::with_capacity(rows.len());
        for row in rows {
            let mod_id = parse_uuid(&row.try_get::<String, _>("mod_id")?, "mod")?;
            let json: String = row.try_get("manifest_json")?;
            current.insert(
                mod_id,
                parse_manifest(&json, plan.source_profile_id, mod_id)?,
            );
        }
        if current.len() != plan.source_manifests.len()
            || plan
                .source_manifests
                .iter()
                .any(|manifest| current.get(&manifest.mod_id) != Some(manifest))
        {
            return Err(AppError::DataIntegrity(
                "提交切换前源 Profile 部署记录已发生变化。".to_owned(),
            ));
        }

        let target_records: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM deployment_records WHERE profile_id = ?")
                .bind(plan.target_profile_id.to_string())
                .fetch_one(&mut *transaction)
                .await?;
        if target_records != 0 {
            return Err(AppError::DataIntegrity(
                "提交切换前目标 Profile 已出现部署记录。".to_owned(),
            ));
        }
        let target_ids = sqlx::query_scalar::<_, String>(
            "SELECT mod_id FROM profile_mods
             WHERE profile_id = ? AND enabled = 1 ORDER BY load_order, mod_id",
        )
        .bind(plan.target_profile_id.to_string())
        .fetch_all(&mut *transaction)
        .await?
        .into_iter()
        .map(|value| parse_uuid(&value, "target profile mod"))
        .collect::<Result<Vec<_>, _>>()?;
        if target_ids != plan.target_mod_ids {
            return Err(AppError::DataIntegrity(
                "提交切换前目标 Profile 内容已发生变化。".to_owned(),
            ));
        }

        for manifest in &plan.source_manifests {
            if manifest.strategy_id == "efmi.direct-folder.v1" {
                let name = manifest_directory_name(manifest)?;
                sqlx::query("UPDATE mods SET repository_path = ?, updated_at = ? WHERE id = ?")
                    .bind(format!("DISABLED_{name}"))
                    .bind(now)
                    .bind(manifest.mod_id.to_string())
                    .execute(&mut *transaction)
                    .await?;
            }
        }
        for manifest in target_manifests {
            if manifest.strategy_id == "efmi.direct-folder.v1" {
                let name = manifest_directory_name(manifest)?;
                sqlx::query("UPDATE mods SET repository_path = ?, updated_at = ? WHERE id = ?")
                    .bind(name)
                    .bind(now)
                    .bind(manifest.mod_id.to_string())
                    .execute(&mut *transaction)
                    .await?;
            }
        }

        let deleted = sqlx::query("DELETE FROM deployment_records WHERE profile_id = ?")
            .bind(plan.source_profile_id.to_string())
            .execute(&mut *transaction)
            .await?;
        if deleted.rows_affected()
            != u64::try_from(plan.source_manifests.len())
                .map_err(|_| AppError::DataIntegrity("源部署数量超出支持范围。".to_owned()))?
        {
            return Err(AppError::DataIntegrity(
                "删除源 Profile 部署记录时数量不一致。".to_owned(),
            ));
        }

        for manifest in target_manifests {
            let json = serde_json::to_string(manifest).map_err(AppError::ConfigFormat)?;
            sqlx::query(
                "INSERT INTO deployment_records
                    (id, profile_id, mod_id, strategy_id, destination_root, manifest_json, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(manifest.id.to_string())
            .bind(manifest.profile_id.to_string())
            .bind(manifest.mod_id.to_string())
            .bind(&manifest.strategy_id)
            .bind(storage_path(&manifest.destination_root))
            .bind(json)
            .bind(manifest.created_at)
            .execute(&mut *transaction)
            .await?;
        }

        let updated = sqlx::query(
            "UPDATE app_state SET active_profile_id = ?, updated_at = ?
             WHERE singleton = 1 AND active_profile_id = ?",
        )
        .bind(plan.target_profile_id.to_string())
        .bind(now)
        .bind(plan.source_profile_id.to_string())
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::DataIntegrity(
                "更新活动 Profile 时状态发生变化。".to_owned(),
            ));
        }
        transaction.commit().await?;
        Ok(())
    }
}

fn manifest_directory_name(manifest: &DeploymentManifest) -> Result<&str, AppError> {
    manifest
        .destination_directory
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::DataIntegrity("EFMI 原地状态清单缺少有效目录名称。".to_owned()))
}

async fn ensure_profile_exists(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    profile_id: Uuid,
) -> Result<(), AppError> {
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM profiles WHERE id = ?")
        .bind(profile_id.to_string())
        .fetch_one(&mut **transaction)
        .await?;
    if exists != 1 {
        return Err(AppError::Profile(format!("Profile {profile_id} 不存在。")));
    }
    Ok(())
}

async fn ensure_name_available(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    name: &str,
    excluding: Option<Uuid>,
) -> Result<(), AppError> {
    let count: i64 = match excluding {
        Some(profile_id) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM profiles WHERE name = ? COLLATE NOCASE AND id != ?",
            )
            .bind(name)
            .bind(profile_id.to_string())
            .fetch_one(&mut **transaction)
            .await?
        }
        None => {
            sqlx::query_scalar("SELECT COUNT(*) FROM profiles WHERE name = ? COLLATE NOCASE")
                .bind(name)
                .fetch_one(&mut **transaction)
                .await?
        }
    };
    if count != 0 {
        return Err(AppError::Profile(format!("Profile 名称“{name}”已存在。")));
    }
    Ok(())
}

fn parse_manifest(
    json: &str,
    profile_id: Uuid,
    mod_id: Uuid,
) -> Result<DeploymentManifest, AppError> {
    let manifest: DeploymentManifest =
        serde_json::from_str(json).map_err(AppError::ConfigFormat)?;
    if manifest.profile_id != profile_id || manifest.mod_id != mod_id {
        return Err(AppError::DataIntegrity(
            "部署清单 JSON 与 Profile/模组列不一致。".to_owned(),
        ));
    }
    Ok(manifest)
}

fn parse_uuid(value: &str, label: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(value)
        .map_err(|_| AppError::DataIntegrity(format!("{label} UUID 无效：{value}")))
}

fn to_u32(value: i64, label: &str) -> Result<u32, AppError> {
    u32::try_from(value)
        .map_err(|_| AppError::DataIntegrity(format!("{label} 超出支持范围：{value}")))
}

fn storage_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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
    use uuid::Uuid;

    use crate::database::{Database, ProfileStore};

    #[tokio::test]
    async fn creates_copies_renames_and_protects_active_profile()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let database = Database::connect(&directory.path().join("mods.db")).await?;
        let store = ProfileStore::new(database.pool().clone());
        let active = store
            .list()
            .await?
            .into_iter()
            .find(|item| item.is_active)
            .ok_or_else(|| std::io::Error::other("active profile missing"))?;
        let created_id = Uuid::new_v4();
        let created = store.create(created_id, "截图配置").await?;
        assert!(!created.is_active);
        let copied_id = Uuid::new_v4();
        let copied = store.copy(active.id, copied_id, "默认配置副本").await?;
        assert_eq!(copied.mods.len(), active.mods.len());
        assert_eq!(store.rename(created_id, "摄影配置").await?.name, "摄影配置");
        assert!(store.delete(active.id).await.is_err());
        store.delete(copied_id).await?;
        assert!(store.get(copied_id).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn reorders_exact_enabled_membership_without_changing_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let database = Database::connect(&directory.path().join("mods.db")).await?;
        let store = ProfileStore::new(database.pool().clone());
        let profile_id = Uuid::new_v4();
        store.create(profile_id, "Order fixture").await?;
        let mod_ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        for (index, mod_id) in mod_ids.iter().enumerate() {
            sqlx::query(
                "INSERT INTO mods
                    (id, logical_id, repository_path, content_fingerprint, size_bytes,
                     installed_at, updated_at, lifecycle_state)
                 VALUES (?, ?, ?, 'fingerprint', 1, 1, 1, 'installed')",
            )
            .bind(mod_id.to_string())
            .bind(format!("fixture.mod.{index}"))
            .bind(format!("fixture-{index}"))
            .execute(database.pool())
            .await?;
            sqlx::query(
                "INSERT INTO profile_mods (profile_id, mod_id, enabled, load_order)
                 VALUES (?, ?, 1, ?)",
            )
            .bind(profile_id.to_string())
            .bind(mod_id.to_string())
            .bind(i64::try_from(index)?)
            .execute(database.pool())
            .await?;
        }

        let requested = [mod_ids[2], mod_ids[0], mod_ids[1]];
        let profile = store.reorder_enabled(profile_id, &requested).await?;

        assert_eq!(
            profile
                .mods
                .iter()
                .filter(|item| item.enabled)
                .map(|item| item.mod_id)
                .collect::<Vec<_>>(),
            requested
        );
        assert!(profile.mods.iter().all(|item| item.enabled));
        assert!(
            store
                .reorder_enabled(profile_id, &requested[..2])
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn rejects_reorder_when_no_safe_temporary_range_exists()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let database = Database::connect(&directory.path().join("mods.db")).await?;
        let store = ProfileStore::new(database.pool().clone());
        let profile_id = Uuid::new_v4();
        store.create(profile_id, "Overflow fixture").await?;
        let mod_ids = [Uuid::new_v4(), Uuid::new_v4()];
        let load_orders = [i64::MAX - 1, i64::MAX];
        for (index, mod_id) in mod_ids.iter().enumerate() {
            sqlx::query(
                "INSERT INTO mods
                    (id, logical_id, repository_path, content_fingerprint, size_bytes,
                     installed_at, updated_at, lifecycle_state)
                 VALUES (?, ?, ?, 'fingerprint', 1, 1, 1, 'installed')",
            )
            .bind(mod_id.to_string())
            .bind(format!("overflow.mod.{index}"))
            .bind(format!("overflow-{index}"))
            .execute(database.pool())
            .await?;
            sqlx::query(
                "INSERT INTO profile_mods (profile_id, mod_id, enabled, load_order)
                 VALUES (?, ?, 1, ?)",
            )
            .bind(profile_id.to_string())
            .bind(mod_id.to_string())
            .bind(load_orders[index])
            .execute(database.pool())
            .await?;
        }

        assert!(
            store
                .reorder_enabled(profile_id, &[mod_ids[1], mod_ids[0]])
                .await
                .is_err()
        );
        let persisted = sqlx::query_scalar::<_, i64>(
            "SELECT load_order FROM profile_mods WHERE profile_id = ? ORDER BY load_order",
        )
        .bind(profile_id.to_string())
        .fetch_all(database.pool())
        .await?;
        assert_eq!(persisted, load_orders);
        Ok(())
    }
}
