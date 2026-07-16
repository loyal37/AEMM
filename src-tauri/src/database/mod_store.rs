use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::SystemTime,
};

use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    core::mods::{CachedModFile, RepositoryScan, ScanCache},
    errors::AppError,
    models::{
        LocalModMetadata, MetadataSourceKind, ModDetails, ModFileDetails, ModLifecycleState,
        ModListItem,
    },
};

#[derive(Debug, Clone)]
pub struct ModStore {
    pool: SqlitePool,
}

#[derive(Debug, Clone, Default)]
pub struct ModSyncOutcome {
    pub added: u64,
    pub updated: u64,
    pub unchanged: u64,
    pub broken: u64,
    pub missing: u64,
}

#[derive(Debug, Clone)]
pub struct ModContentReference {
    pub repository_path: PathBuf,
    pub preview_path: Option<PathBuf>,
}

impl ModStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn load_scan_cache(&self) -> Result<ScanCache, AppError> {
        let rows = sqlx::query(
            "SELECT m.repository_path, f.source_path, f.size_bytes, f.modified_at, f.content_hash
             FROM mod_files f
             JOIN mods m ON m.id = f.mod_id
             WHERE f.content_hash IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut cache = ScanCache::with_capacity(rows.len());
        for row in rows {
            let size_bytes = non_negative_u64(row.try_get::<i64, _>("size_bytes")?, "size_bytes")?;
            cache.insert(
                (
                    row.try_get::<String, _>("repository_path")?,
                    row.try_get::<String, _>("source_path")?,
                ),
                CachedModFile {
                    size_bytes,
                    modified_at: row.try_get("modified_at")?,
                    content_hash: row.try_get("content_hash")?,
                },
            );
        }
        Ok(cache)
    }

    pub async fn synchronize(&self, scan: &RepositoryScan) -> Result<ModSyncOutcome, AppError> {
        let now = unix_timestamp_seconds()?;
        let mut transaction = self.pool.begin().await?;
        let mut outcome = ModSyncOutcome::default();
        let mut seen_ids = HashSet::new();

        for scanned_mod in &scan.mods {
            let repository_path = scanned_mod.repository_path.storage_key();
            let logical_id = &scanned_mod.author_metadata.logical_id;
            let existing_rows = sqlx::query(
                "SELECT id, repository_path, content_fingerprint, lifecycle_state
                 FROM mods
                 WHERE logical_id = ? COLLATE NOCASE OR repository_path = ? COLLATE NOCASE",
            )
            .bind(logical_id)
            .bind(&repository_path)
            .fetch_all(&mut *transaction)
            .await?;
            if existing_rows.len() > 1 {
                return Err(AppError::DataIntegrity(format!(
                    "逻辑 ID {logical_id} 与仓库路径 {repository_path} 指向不同数据库记录。"
                )));
            }

            let lifecycle_state = if scanned_mod.is_broken() {
                outcome.broken += 1;
                ModLifecycleState::Broken
            } else {
                ModLifecycleState::Installed
            };
            let lifecycle_value = lifecycle_to_database(lifecycle_state);
            let size_bytes = checked_i64(scanned_mod.size_bytes, "mod size")?;

            let (mod_id, is_new, content_changed) = if let Some(row) = existing_rows.first() {
                let mod_id: String = row.try_get("id")?;
                let old_path: String = row.try_get("repository_path")?;
                let old_fingerprint: Option<String> = row.try_get("content_fingerprint")?;
                let old_state: String = row.try_get("lifecycle_state")?;
                let changed = old_path != repository_path
                    || old_fingerprint.as_deref() != Some(&scanned_mod.content_fingerprint)
                    || old_state != lifecycle_value;
                if changed {
                    outcome.updated += 1;
                    sqlx::query(
                        "UPDATE mods
                         SET logical_id = ?, repository_path = ?, content_fingerprint = ?,
                             size_bytes = ?, updated_at = ?, lifecycle_state = ?
                         WHERE id = ?",
                    )
                    .bind(logical_id)
                    .bind(&repository_path)
                    .bind(&scanned_mod.content_fingerprint)
                    .bind(size_bytes)
                    .bind(now)
                    .bind(lifecycle_value)
                    .bind(&mod_id)
                    .execute(&mut *transaction)
                    .await?;
                } else {
                    outcome.unchanged += 1;
                }
                (mod_id, false, changed)
            } else {
                let mod_id = Uuid::new_v4().to_string();
                outcome.added += 1;
                sqlx::query(
                    "INSERT INTO mods (
                        id, logical_id, repository_path, content_fingerprint, size_bytes,
                        installed_at, updated_at, lifecycle_state
                     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&mod_id)
                .bind(logical_id)
                .bind(&repository_path)
                .bind(&scanned_mod.content_fingerprint)
                .bind(size_bytes)
                .bind(now)
                .bind(now)
                .bind(lifecycle_value)
                .execute(&mut *transaction)
                .await?;
                (mod_id, true, true)
            };
            seen_ids.insert(mod_id.clone());

            if content_changed {
                let metadata = &scanned_mod.author_metadata;
                let original_json = metadata
                    .original_document
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(AppError::ConfigFormat)?;
                sqlx::query(
                    "INSERT INTO mod_author_metadata (
                        mod_id, name, author, version, description, category, game_version,
                        website, preview_path, original_json, source_kind
                     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(mod_id) DO UPDATE SET
                        name = excluded.name,
                        author = excluded.author,
                        version = excluded.version,
                        description = excluded.description,
                        category = excluded.category,
                        game_version = excluded.game_version,
                        website = excluded.website,
                        preview_path = excluded.preview_path,
                        original_json = excluded.original_json,
                        source_kind = excluded.source_kind",
                )
                .bind(&mod_id)
                .bind(&metadata.name)
                .bind(&metadata.author)
                .bind(&metadata.version)
                .bind(&metadata.description)
                .bind(&metadata.category)
                .bind(&metadata.game_version)
                .bind(&metadata.website)
                .bind(
                    metadata
                        .preview_path
                        .as_ref()
                        .map(|path| storage_path(path)),
                )
                .bind(original_json)
                .bind(metadata_source_to_database(metadata.source_kind))
                .execute(&mut *transaction)
                .await?;
            }

            if is_new {
                sqlx::query(
                    "INSERT INTO mod_local_metadata (
                        mod_id, favorite, updated_at, tags_json
                     ) VALUES (?, 0, ?, '[]')
                     ON CONFLICT(mod_id) DO NOTHING",
                )
                .bind(&mod_id)
                .bind(now)
                .execute(&mut *transaction)
                .await?;
            }

            let existing_file_rows =
                sqlx::query("SELECT source_path FROM mod_files WHERE mod_id = ?")
                    .bind(&mod_id)
                    .fetch_all(&mut *transaction)
                    .await?;
            let current_paths = scanned_mod
                .files
                .iter()
                .map(|file| storage_path(&file.source_path))
                .collect::<HashSet<_>>();
            for row in existing_file_rows {
                let source_path: String = row.try_get("source_path")?;
                if !current_paths.contains(&source_path) {
                    sqlx::query("DELETE FROM mod_files WHERE mod_id = ? AND source_path = ?")
                        .bind(&mod_id)
                        .bind(source_path)
                        .execute(&mut *transaction)
                        .await?;
                }
            }
            for file in &scanned_mod.files {
                sqlx::query(
                    "INSERT INTO mod_files (
                        mod_id, source_path, deployment_target, size_bytes,
                        content_hash, file_role, modified_at
                     ) VALUES (?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(mod_id, source_path) DO UPDATE SET
                        deployment_target = excluded.deployment_target,
                        size_bytes = excluded.size_bytes,
                        content_hash = excluded.content_hash,
                        file_role = excluded.file_role,
                        modified_at = excluded.modified_at
                     WHERE mod_files.deployment_target IS NOT excluded.deployment_target
                        OR mod_files.size_bytes != excluded.size_bytes
                        OR mod_files.content_hash IS NOT excluded.content_hash
                        OR mod_files.file_role != excluded.file_role
                        OR mod_files.modified_at != excluded.modified_at",
                )
                .bind(&mod_id)
                .bind(storage_path(&file.source_path))
                .bind(
                    file.deployment_target
                        .as_ref()
                        .map(|path| storage_path(path)),
                )
                .bind(checked_i64(file.size_bytes, "file size")?)
                .bind(&file.content_hash)
                .bind(&file.file_role)
                .bind(file.modified_at)
                .execute(&mut *transaction)
                .await?;
            }
        }

        let stored_mods = sqlx::query("SELECT id, lifecycle_state FROM mods")
            .fetch_all(&mut *transaction)
            .await?;
        for row in stored_mods {
            let mod_id: String = row.try_get("id")?;
            let lifecycle_state: String = row.try_get("lifecycle_state")?;
            if !seen_ids.contains(&mod_id) && lifecycle_state != "broken" {
                sqlx::query(
                    "UPDATE mods SET lifecycle_state = 'broken', updated_at = ? WHERE id = ?",
                )
                .bind(now)
                .bind(mod_id)
                .execute(&mut *transaction)
                .await?;
                outcome.missing += 1;
            }
        }

        transaction.commit().await?;
        Ok(outcome)
    }

    pub async fn list(&self) -> Result<Vec<ModListItem>, AppError> {
        let rows = sqlx::query(
            "SELECT
                m.id, m.logical_id, m.repository_path, m.size_bytes,
                m.installed_at, m.updated_at, m.lifecycle_state,
                COALESCE(l.display_name_override, a.name) AS display_name,
                a.author, a.version,
                COALESCE(l.description_override, a.description) AS description,
                COALESCE(l.category_override, a.category) AS category,
                a.preview_path, COALESCE(l.favorite, 0) AS favorite,
                COUNT(f.id) AS file_count
             FROM mods m
             JOIN mod_author_metadata a ON a.mod_id = m.id
             LEFT JOIN mod_local_metadata l ON l.mod_id = m.id
             LEFT JOIN mod_files f ON f.mod_id = m.id
             GROUP BY m.id
             ORDER BY m.updated_at DESC, display_name COLLATE NOCASE ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_list_item).collect()
    }

    pub async fn details(&self, mod_id: Uuid) -> Result<ModDetails, AppError> {
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT
                m.id, m.logical_id, m.repository_path, m.size_bytes,
                m.installed_at, m.updated_at, m.lifecycle_state,
                COALESCE(l.display_name_override, a.name) AS display_name,
                a.author, a.version,
                COALESCE(l.description_override, a.description) AS description,
                COALESCE(l.category_override, a.category) AS category,
                a.preview_path, COALESCE(l.favorite, 0) AS favorite,
                COUNT(f.id) AS file_count,
                a.name AS author_name, a.description AS author_description,
                a.category AS author_category, a.game_version, a.website, a.source_kind,
                l.display_name_override, l.category_override, l.description_override,
                l.notes, COALESCE(l.tags_json, '[]') AS tags_json
             FROM mods m
             JOIN mod_author_metadata a ON a.mod_id = m.id
             LEFT JOIN mod_local_metadata l ON l.mod_id = m.id
             LEFT JOIN mod_files f ON f.mod_id = m.id
             WHERE m.id = ?
             GROUP BY m.id",
        )
        .bind(mod_id.to_string())
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| AppError::NotAvailable("模组记录不存在。".to_owned()))?;

        let tags_json: String = row.try_get("tags_json")?;
        let tags = serde_json::from_str::<Vec<String>>(&tags_json).map_err(|error| {
            AppError::DataIntegrity(format!("模组 {mod_id} 的本地标签数据无效：{error}"))
        })?;
        let metadata_source =
            metadata_source_from_database(&row.try_get::<String, _>("source_kind")?)?;
        let item = row_to_list_item(&row)?;
        let file_rows = sqlx::query(
            "SELECT source_path, size_bytes, content_hash, file_role, modified_at
             FROM mod_files
             WHERE mod_id = ?
             ORDER BY source_path COLLATE NOCASE ASC",
        )
        .bind(mod_id.to_string())
        .fetch_all(&mut *transaction)
        .await?;
        transaction.commit().await?;
        let files = file_rows
            .iter()
            .map(row_to_file_details)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ModDetails {
            item,
            author_name: row.try_get("author_name")?,
            author_description: row.try_get("author_description")?,
            author_category: row.try_get("author_category")?,
            game_version: row.try_get("game_version")?,
            website: row.try_get("website")?,
            metadata_source,
            local_metadata: LocalModMetadata {
                display_name_override: row.try_get("display_name_override")?,
                category_override: row.try_get("category_override")?,
                description_override: row.try_get("description_override")?,
                favorite: row.try_get::<i64, _>("favorite")? != 0,
                notes: row.try_get("notes")?,
                tags,
            },
            files,
        })
    }

    pub async fn content_reference(&self, mod_id: Uuid) -> Result<ModContentReference, AppError> {
        let row = sqlx::query(
            "SELECT m.repository_path, a.preview_path
             FROM mods m
             JOIN mod_author_metadata a ON a.mod_id = m.id
             WHERE m.id = ?",
        )
        .bind(mod_id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotAvailable("模组记录不存在。".to_owned()))?;
        Ok(ModContentReference {
            repository_path: PathBuf::from(row.try_get::<String, _>("repository_path")?),
            preview_path: row
                .try_get::<Option<String>, _>("preview_path")?
                .map(PathBuf::from),
        })
    }

    pub async fn set_favorite(&self, mod_ids: &[Uuid], favorite: bool) -> Result<u64, AppError> {
        let now = unix_timestamp_seconds()?;
        let mut transaction = self.pool.begin().await?;
        for mod_id in mod_ids {
            let result = sqlx::query(
                "UPDATE mod_local_metadata SET favorite = ?, updated_at = ? WHERE mod_id = ?",
            )
            .bind(i64::from(favorite))
            .bind(now)
            .bind(mod_id.to_string())
            .execute(&mut *transaction)
            .await?;
            if result.rows_affected() != 1 {
                return Err(AppError::NotAvailable(format!(
                    "模组 {mod_id} 不存在，请重新扫描后再试。"
                )));
            }
        }
        transaction.commit().await?;
        u64::try_from(mod_ids.len())
            .map_err(|_| AppError::DataIntegrity("收藏更新数量超过支持范围。".to_owned()))
    }

    pub async fn update_local_metadata(
        &self,
        mod_id: Uuid,
        metadata: &LocalModMetadata,
    ) -> Result<ModListItem, AppError> {
        let now = unix_timestamp_seconds()?;
        let tags_json = serde_json::to_string(&metadata.tags).map_err(AppError::ConfigFormat)?;
        let result = sqlx::query(
            "UPDATE mod_local_metadata
             SET display_name_override = ?, category_override = ?, description_override = ?,
                 favorite = ?, notes = ?, tags_json = ?, updated_at = ?
             WHERE mod_id = ?",
        )
        .bind(&metadata.display_name_override)
        .bind(&metadata.category_override)
        .bind(&metadata.description_override)
        .bind(i64::from(metadata.favorite))
        .bind(&metadata.notes)
        .bind(tags_json)
        .bind(now)
        .bind(mod_id.to_string())
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(AppError::NotAvailable(
                "要修改的模组不存在，请先重新扫描。".to_owned(),
            ));
        }
        self.get(mod_id).await
    }

    async fn get(&self, mod_id: Uuid) -> Result<ModListItem, AppError> {
        let row = sqlx::query(
            "SELECT
                m.id, m.logical_id, m.repository_path, m.size_bytes,
                m.installed_at, m.updated_at, m.lifecycle_state,
                COALESCE(l.display_name_override, a.name) AS display_name,
                a.author, a.version,
                COALESCE(l.description_override, a.description) AS description,
                COALESCE(l.category_override, a.category) AS category,
                a.preview_path, COALESCE(l.favorite, 0) AS favorite,
                COUNT(f.id) AS file_count
             FROM mods m
             JOIN mod_author_metadata a ON a.mod_id = m.id
             LEFT JOIN mod_local_metadata l ON l.mod_id = m.id
             LEFT JOIN mod_files f ON f.mod_id = m.id
             WHERE m.id = ?
             GROUP BY m.id",
        )
        .bind(mod_id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotAvailable("模组记录不存在。".to_owned()))?;
        row_to_list_item(&row)
    }
}

fn row_to_list_item(row: &sqlx::sqlite::SqliteRow) -> Result<ModListItem, AppError> {
    let id_value: String = row.try_get("id")?;
    let id = Uuid::parse_str(&id_value)
        .map_err(|_| AppError::DataIntegrity(format!("无效的模组 UUID：{id_value}")))?;
    Ok(ModListItem {
        id,
        logical_id: row.try_get("logical_id")?,
        repository_path: PathBuf::from(row.try_get::<String, _>("repository_path")?),
        name: row.try_get("display_name")?,
        author: row.try_get("author")?,
        version: row.try_get("version")?,
        description: row.try_get("description")?,
        category: row.try_get("category")?,
        preview_path: row
            .try_get::<Option<String>, _>("preview_path")?
            .map(PathBuf::from),
        favorite: row.try_get::<i64, _>("favorite")? != 0,
        size_bytes: non_negative_u64(row.try_get("size_bytes")?, "size_bytes")?,
        file_count: non_negative_u64(row.try_get("file_count")?, "file_count")?,
        installed_at: row.try_get("installed_at")?,
        updated_at: row.try_get("updated_at")?,
        lifecycle_state: lifecycle_from_database(&row.try_get::<String, _>("lifecycle_state")?)?,
    })
}

fn row_to_file_details(row: &sqlx::sqlite::SqliteRow) -> Result<ModFileDetails, AppError> {
    let modified_at_nanos: i64 = row.try_get("modified_at")?;
    if modified_at_nanos < 0 {
        return Err(AppError::DataIntegrity(
            "模组文件修改时间不能为负数。".to_owned(),
        ));
    }
    Ok(ModFileDetails {
        source_path: PathBuf::from(row.try_get::<String, _>("source_path")?),
        size_bytes: non_negative_u64(row.try_get("size_bytes")?, "size_bytes")?,
        content_hash: row.try_get("content_hash")?,
        file_role: row.try_get("file_role")?,
        modified_at_ms: modified_at_nanos / 1_000_000,
    })
}

fn lifecycle_to_database(value: ModLifecycleState) -> &'static str {
    match value {
        ModLifecycleState::Installing => "installing",
        ModLifecycleState::Installed => "installed",
        ModLifecycleState::Broken => "broken",
        ModLifecycleState::Removing => "removing",
    }
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

fn metadata_source_to_database(value: MetadataSourceKind) -> &'static str {
    match value {
        MetadataSourceKind::ModJson => "mod_json",
        MetadataSourceKind::Inferred => "inferred",
    }
}

fn metadata_source_from_database(value: &str) -> Result<MetadataSourceKind, AppError> {
    match value {
        "mod_json" => Ok(MetadataSourceKind::ModJson),
        "inferred" => Ok(MetadataSourceKind::Inferred),
        _ => Err(AppError::DataIntegrity(format!(
            "未知的元数据来源：{value}"
        ))),
    }
}

fn storage_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn checked_i64(value: u64, label: &str) -> Result<i64, AppError> {
    i64::try_from(value)
        .map_err(|_| AppError::DataIntegrity(format!("{label} exceeds SQLite integer range")))
}

fn non_negative_u64(value: i64, label: &str) -> Result<u64, AppError> {
    u64::try_from(value)
        .map_err(|_| AppError::DataIntegrity(format!("{label} is negative in SQLite")))
}

fn unix_timestamp_seconds() -> Result<i64, AppError> {
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| AppError::DataIntegrity("系统时间早于 Unix Epoch。".to_owned()))?;
    i64::try_from(duration.as_secs())
        .map_err(|_| AppError::DataIntegrity("系统时间超过支持范围。".to_owned()))
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs};

    use crate::{
        core::mods::{
            FileSystemModScanner, ModScanner, RepositoryInitializationPolicy, RepositoryRoot,
        },
        database::{Database, ModStore},
        models::LocalModMetadata,
    };

    #[tokio::test]
    async fn synchronizes_scan_and_preserves_local_overrides()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let database = Database::connect(&directory.path().join("mods.db")).await?;
        let store = ModStore::new(database.pool().clone());
        let repository_path = directory.path().join("repository");
        fs::create_dir(&repository_path)?;
        let root = RepositoryRoot::open_or_initialize(
            &repository_path,
            RepositoryInitializationPolicy::EmptyOnly,
        )?;
        fs::create_dir(root.path().join("example"))?;
        fs::write(
            root.path().join("example/mod.json"),
            br#"{"id":"author.example","name":"Example"}"#,
        )?;
        fs::write(root.path().join("example/content.ini"), b"content")?;

        let scan = FileSystemModScanner::new()
            .scan_repository(root.clone(), HashMap::new())
            .await?;
        let first = store.synchronize(&scan).await?;
        assert_eq!(first.added, 1);
        let item = store.list().await?.remove(0);
        store
            .update_local_metadata(
                item.id,
                &LocalModMetadata {
                    display_name_override: Some("Local Name".to_owned()),
                    favorite: true,
                    ..LocalModMetadata::default()
                },
            )
            .await?;
        let details = store.details(item.id).await?;
        assert_eq!(details.author_name, "Example");
        assert_eq!(
            details.local_metadata.display_name_override.as_deref(),
            Some("Local Name")
        );
        assert_eq!(details.files.len(), 2);

        assert_eq!(store.set_favorite(&[item.id], false).await?, 1);
        assert!(!store.details(item.id).await?.item.favorite);
        assert_eq!(store.set_favorite(&[item.id], true).await?, 1);

        let cache = store.load_scan_cache().await?;
        let second_scan = FileSystemModScanner::new()
            .scan_repository(root, cache)
            .await?;
        let second = store.synchronize(&second_scan).await?;
        assert_eq!(second.unchanged, 1);
        let item = store.list().await?.remove(0);
        assert_eq!(item.name, "Local Name");
        assert!(item.favorite);
        Ok(())
    }

    #[tokio::test]
    async fn marks_missing_repository_mod_as_broken() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let database = Database::connect(&directory.path().join("mods.db")).await?;
        let store = ModStore::new(database.pool().clone());
        let repository_path = directory.path().join("repository");
        fs::create_dir(&repository_path)?;
        let root = RepositoryRoot::open_or_initialize(
            &repository_path,
            RepositoryInitializationPolicy::EmptyOnly,
        )?;
        fs::create_dir(root.path().join("example"))?;
        fs::write(root.path().join("example/content.ini"), b"content")?;
        let first_scan = FileSystemModScanner::new()
            .scan_repository(root.clone(), HashMap::new())
            .await?;
        store.synchronize(&first_scan).await?;
        fs::remove_dir_all(root.path().join("example"))?;

        let empty_scan = FileSystemModScanner::new()
            .scan_repository(root, store.load_scan_cache().await?)
            .await?;
        let result = store.synchronize(&empty_scan).await?;
        assert_eq!(result.missing, 1);
        assert!(matches!(
            store.list().await?.remove(0).lifecycle_state,
            crate::models::ModLifecycleState::Broken
        ));
        Ok(())
    }
}
