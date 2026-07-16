use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use tokio::sync::Mutex;

use crate::{
    core::mods::{
        FileSystemModScanner, ModScanner, RepositoryInitializationPolicy, RepositoryRoot,
    },
    database::{Database, ModStore},
    errors::AppError,
    models::{LocalModMetadata, ModListItem, ModScanResult},
};

use super::SettingsService;

const MAX_OVERRIDE_LENGTH: usize = 512;
const MAX_DESCRIPTION_LENGTH: usize = 32 * 1024;
const MAX_NOTES_LENGTH: usize = 32 * 1024;
const MAX_TAGS: usize = 64;
const MAX_TAG_LENGTH: usize = 64;
const MAX_REPORTED_ISSUES: usize = 200;

#[derive(Debug)]
pub struct ModService {
    settings: SettingsService,
    default_repository_path: PathBuf,
    scanner: FileSystemModScanner,
    store: ModStore,
    scan_lock: Arc<Mutex<()>>,
}

impl ModService {
    pub fn new(
        settings: SettingsService,
        database: &Database,
        default_repository_path: PathBuf,
    ) -> Self {
        Self {
            settings,
            default_repository_path,
            scanner: FileSystemModScanner::new(),
            store: ModStore::new(database.pool().clone()),
            scan_lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn scan_repository(&self) -> Result<ModScanResult, AppError> {
        let _scan_guard = self.scan_lock.lock().await;
        let started = Instant::now();
        let settings = self.settings.get().await;
        let configured_path = settings.storage.repository_path;
        let policy =
            if paths_equal_case_insensitive(&configured_path, &self.default_repository_path) {
                RepositoryInitializationPolicy::TrustedAemmDefault
            } else {
                RepositoryInitializationPolicy::EmptyOnly
            };
        let root_path = configured_path.clone();
        let root = tokio::task::spawn_blocking(move || {
            RepositoryRoot::open_or_initialize(&root_path, policy)
        })
        .await??;

        tracing::info!(repository_root = %root.path().display(), "starting mod repository scan");
        let cache = self.store.load_scan_cache().await?;
        let scan = self.scanner.scan_repository(root, cache).await?;
        let sync = self.store.synchronize(&scan).await?;
        let discovered = u64::try_from(scan.mods.len())
            .map_err(|_| AppError::DataIntegrity("mod count overflow".to_owned()))?;
        let mut issues = scan
            .issues
            .iter()
            .chain(scan.mods.iter().flat_map(|item| item.issues.iter()))
            .map(|issue| issue.display_message())
            .take(MAX_REPORTED_ISSUES)
            .collect::<Vec<_>>();
        let total_issue_count = scan.issues.len()
            + scan
                .mods
                .iter()
                .map(|item| item.issues.len())
                .sum::<usize>();
        if total_issue_count > MAX_REPORTED_ISSUES {
            issues.push(format!(
                "另有 {} 条扫描提示未在结果中展开，请查看日志。",
                total_issue_count - MAX_REPORTED_ISSUES
            ));
        }
        for issue in &issues {
            tracing::warn!(scan_issue = issue, "mod repository scan issue");
        }

        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let result = ModScanResult {
            discovered,
            added: sync.added,
            updated: sync.updated,
            unchanged: sync.unchanged,
            broken: sync.broken,
            missing: sync.missing,
            hashed_files: scan.hashed_files,
            reused_hashes: scan.reused_hashes,
            skipped_entries: scan.skipped_entries,
            duration_ms,
            issues,
        };
        tracing::info!(
            discovered = result.discovered,
            added = result.added,
            updated = result.updated,
            unchanged = result.unchanged,
            broken = result.broken,
            missing = result.missing,
            hashed_files = result.hashed_files,
            reused_hashes = result.reused_hashes,
            duration_ms = result.duration_ms,
            filesystem_duration_ms = scan.duration.as_millis(),
            "mod repository scan completed"
        );
        Ok(result)
    }

    pub async fn list(&self) -> Result<Vec<ModListItem>, AppError> {
        self.store.list().await
    }

    pub async fn update_local_metadata(
        &self,
        mod_id: uuid::Uuid,
        metadata: LocalModMetadata,
    ) -> Result<ModListItem, AppError> {
        let metadata = validate_local_metadata(metadata)?;
        self.store.update_local_metadata(mod_id, &metadata).await
    }
}

fn validate_local_metadata(mut metadata: LocalModMetadata) -> Result<LocalModMetadata, AppError> {
    metadata.display_name_override = normalize_optional(
        metadata.display_name_override,
        "本地显示名称",
        MAX_OVERRIDE_LENGTH,
    )?;
    metadata.category_override =
        normalize_optional(metadata.category_override, "本地分类", MAX_OVERRIDE_LENGTH)?;
    metadata.description_override = normalize_optional(
        metadata.description_override,
        "本地描述",
        MAX_DESCRIPTION_LENGTH,
    )?;
    metadata.notes = normalize_optional(metadata.notes, "本地备注", MAX_NOTES_LENGTH)?;
    if metadata.tags.len() > MAX_TAGS {
        return Err(AppError::ModMetadata(format!(
            "标签数量不能超过 {MAX_TAGS}。"
        )));
    }
    let mut seen = HashSet::new();
    let mut tags = Vec::with_capacity(metadata.tags.len());
    for tag in metadata.tags {
        let tag = tag.trim().to_owned();
        if tag.is_empty() || tag.chars().count() > MAX_TAG_LENGTH || tag.contains('\0') {
            return Err(AppError::ModMetadata(format!(
                "标签不能为空、包含 NUL 或超过 {MAX_TAG_LENGTH} 个字符。"
            )));
        }
        if seen.insert(tag.to_lowercase()) {
            tags.push(tag);
        }
    }
    metadata.tags = tags;
    Ok(metadata)
}

fn normalize_optional(
    value: Option<String>,
    label: &str,
    max_length: usize,
) -> Result<Option<String>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > max_length || value.contains('\0') {
        return Err(AppError::ModMetadata(format!(
            "{label}不能包含 NUL 且不能超过 {max_length} 个字符。"
        )));
    }
    Ok(Some(value))
}

fn paths_equal_case_insensitive(left: &Path, right: &Path) -> bool {
    normalized_path_key(left) == normalized_path_key(right)
}

fn normalized_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use crate::{models::LocalModMetadata, services::mods::validate_local_metadata};

    #[test]
    fn normalizes_and_deduplicates_local_tags() -> Result<(), Box<dyn std::error::Error>> {
        let metadata = validate_local_metadata(LocalModMetadata {
            display_name_override: Some("  Local Name  ".to_owned()),
            tags: vec!["Character".to_owned(), "character".to_owned()],
            ..LocalModMetadata::default()
        })?;
        assert_eq!(
            metadata.display_name_override.as_deref(),
            Some("Local Name")
        );
        assert_eq!(metadata.tags, vec!["Character"]);
        Ok(())
    }
}
