use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use walkdir::WalkDir;

use crate::{
    core::mods::{
        FileSystemMetadataManager, ModScanner, RepositoryRelativePath, RepositoryRoot,
        repository::{
            REMOVAL_TOMBSTONE_PREFIX, REPOSITORY_MARKER_FILE, path_is_link_or_reparse_point,
        },
    },
    errors::AppError,
    models::{AuthorModMetadata, MetadataSourceKind, ModFile},
    utils::validate_relative_path,
};

const HASH_BUFFER_BYTES: usize = 128 * 1024;

#[derive(Debug, Clone)]
pub struct CachedModFile {
    pub size_bytes: u64,
    pub modified_at: i64,
    pub content_hash: String,
}

pub type ScanCache = HashMap<(String, String), CachedModFile>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanIssueLevel {
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct ScanIssue {
    pub level: ScanIssueLevel,
    pub repository_path: Option<String>,
    pub message: String,
}

impl ScanIssue {
    pub fn display_message(&self) -> String {
        match &self.repository_path {
            Some(path) => format!("{path}: {}", self.message),
            None => self.message.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScannedMod {
    pub root: PathBuf,
    pub repository_path: RepositoryRelativePath,
    pub author_metadata: AuthorModMetadata,
    pub files: Vec<ModFile>,
    pub size_bytes: u64,
    pub content_fingerprint: String,
    pub enabled_in_efmi: bool,
    pub issues: Vec<ScanIssue>,
}

impl ScannedMod {
    pub fn is_broken(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.level == ScanIssueLevel::Error)
    }
}

#[derive(Debug, Clone)]
pub struct RepositoryScan {
    pub mods: Vec<ScannedMod>,
    pub issues: Vec<ScanIssue>,
    pub hashed_files: u64,
    pub reused_hashes: u64,
    pub skipped_entries: u64,
    pub duration: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct FileSystemModScanner {
    metadata: FileSystemMetadataManager,
}

impl FileSystemModScanner {
    pub const fn new() -> Self {
        Self {
            metadata: FileSystemMetadataManager::new(),
        }
    }

    fn scan_repository_sync(
        &self,
        repository_root: RepositoryRoot,
        cache: ScanCache,
    ) -> Result<RepositoryScan, AppError> {
        let started = Instant::now();
        let mut paths = fs::read_dir(repository_root.path())
            .map_err(|source| AppError::file_system(repository_root.path(), source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| AppError::file_system(repository_root.path(), source))?;
        paths.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());

        let mut mods = Vec::new();
        let mut issues = Vec::new();
        let mut hashed_files = 0_u64;
        let mut reused_hashes = 0_u64;
        let mut skipped_entries = 0_u64;

        for entry in paths {
            let name = entry.file_name();
            if name.to_string_lossy() == REPOSITORY_MARKER_FILE {
                continue;
            }
            if name.to_string_lossy().starts_with(REMOVAL_TOMBSTONE_PREFIX) {
                skipped_entries += 1;
                continue;
            }
            let path = entry.path();
            if path_is_link_or_reparse_point(&path)? {
                skipped_entries += 1;
                issues.push(ScanIssue {
                    level: ScanIssueLevel::Warning,
                    repository_path: Some(name.to_string_lossy().into_owned()),
                    message: "仓库顶层链接或重解析点已跳过。".to_owned(),
                });
                continue;
            }
            let file_type = entry
                .file_type()
                .map_err(|source| AppError::file_system(&path, source))?;
            if !file_type.is_dir() {
                skipped_entries += 1;
                issues.push(ScanIssue {
                    level: ScanIssueLevel::Warning,
                    repository_path: Some(name.to_string_lossy().into_owned()),
                    message: "仓库顶层普通文件不是模组目录，已跳过。".to_owned(),
                });
                continue;
            }

            let relative = RepositoryRelativePath::new(PathBuf::from(name))?;
            let mod_root = repository_root.resolve_existing_mod_root(&relative)?;
            let counters = ScanCounters {
                hashed_files: &mut hashed_files,
                reused_hashes: &mut reused_hashes,
                skipped_entries: &mut skipped_entries,
            };
            mods.push(self.scan_mod_sync(mod_root, relative, &cache, counters)?);
        }

        resolve_duplicate_logical_ids(&mut mods)?;
        Ok(RepositoryScan {
            mods,
            issues,
            hashed_files,
            reused_hashes,
            skipped_entries,
            duration: started.elapsed(),
        })
    }

    fn scan_mod_sync(
        &self,
        root: PathBuf,
        repository_path: RepositoryRelativePath,
        cache: &ScanCache,
        counters: ScanCounters<'_>,
    ) -> Result<ScannedMod, AppError> {
        let repository_key = repository_path.storage_key();
        let enabled_in_efmi = !is_disabled_directory_name(&repository_key);
        let metadata_key = enabled_directory_name(&repository_key);
        let mut issues = Vec::new();
        let mut skipped_paths = Vec::new();
        let walker = WalkDir::new(&root)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| {
                if entry.depth() == 0 {
                    return true;
                }
                match path_is_link_or_reparse_point(entry.path()) {
                    Ok(false) => true,
                    Ok(true) => {
                        skipped_paths.push(entry.path().to_path_buf());
                        false
                    }
                    Err(_) => {
                        skipped_paths.push(entry.path().to_path_buf());
                        false
                    }
                }
            });

        let mut files = Vec::new();
        let mut normalized_paths = HashSet::new();
        let mut size_bytes = 0_u64;
        for entry in walker {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    issues.push(ScanIssue {
                        level: ScanIssueLevel::Error,
                        repository_path: Some(repository_key.clone()),
                        message: format!("无法遍历模组文件：{error}"),
                    });
                    continue;
                }
            };
            if entry.depth() == 0 || entry.file_type().is_dir() {
                continue;
            }
            if !entry.file_type().is_file() {
                *counters.skipped_entries += 1;
                issues.push(ScanIssue {
                    level: ScanIssueLevel::Error,
                    repository_path: Some(repository_key.clone()),
                    message: format!("跳过非普通文件 {}。", entry.path().display()),
                });
                continue;
            }

            let relative = entry.path().strip_prefix(&root).map_err(|_| {
                AppError::UnsafePath("扫描条目无法转换为模组内相对路径。".to_owned())
            })?;
            validate_relative_path(relative)?;
            let storage_path = relative
                .to_str()
                .ok_or_else(|| AppError::ModScan("模组文件路径不是有效 Unicode。".to_owned()))?
                .replace('\\', "/");
            if !normalized_paths.insert(storage_path.to_lowercase()) {
                issues.push(ScanIssue {
                    level: ScanIssueLevel::Error,
                    repository_path: Some(repository_key.clone()),
                    message: format!("检测到大小写不敏感的重复路径 {storage_path}。"),
                });
                *counters.skipped_entries += 1;
                continue;
            }

            let metadata = entry
                .metadata()
                .map_err(|source| AppError::ModScan(source.to_string()))?;
            let modified_at = modified_at_nanos(&metadata)?;
            let file_size = metadata.len();
            size_bytes = size_bytes.checked_add(file_size).ok_or_else(|| {
                AppError::DataIntegrity("模组文件总大小超过支持范围。".to_owned())
            })?;
            let cache_key = (repository_key.clone(), storage_path.clone());
            let content_hash = if let Some(cached) = cache.get(&cache_key).filter(|cached| {
                cached.size_bytes == file_size && cached.modified_at == modified_at
            }) {
                *counters.reused_hashes += 1;
                cached.content_hash.clone()
            } else {
                *counters.hashed_files += 1;
                match hash_file(entry.path()) {
                    Ok(hash) => hash,
                    Err(error) => {
                        issues.push(ScanIssue {
                            level: ScanIssueLevel::Error,
                            repository_path: Some(repository_key.clone()),
                            message: format!("无法计算 {storage_path} 的 Hash：{error}"),
                        });
                        String::new()
                    }
                }
            };

            files.push(ModFile {
                source_path: PathBuf::from(storage_path),
                deployment_target: None,
                size_bytes: file_size,
                content_hash: (!content_hash.is_empty()).then_some(content_hash),
                file_role: classify_file_role(relative),
                modified_at,
            });
        }

        for path in skipped_paths {
            *counters.skipped_entries += 1;
            let relative = path.strip_prefix(&root).unwrap_or(path.as_path());
            issues.push(ScanIssue {
                level: ScanIssueLevel::Error,
                repository_path: Some(repository_key.clone()),
                message: format!("链接或重解析点 {} 已跳过。", relative.display()),
            });
        }

        files.sort_by_key(|file| file.source_path.to_string_lossy().to_lowercase());
        let metadata_read = self.metadata.read_with_warnings(&root, metadata_key)?;
        issues.extend(metadata_read.warnings.into_iter().map(|message| ScanIssue {
            level: ScanIssueLevel::Warning,
            repository_path: Some(repository_key.clone()),
            message,
        }));
        let content_fingerprint = fingerprint_files(&files);

        Ok(ScannedMod {
            root,
            repository_path,
            author_metadata: metadata_read.metadata,
            files,
            size_bytes,
            content_fingerprint,
            enabled_in_efmi,
            issues,
        })
    }
}

fn is_disabled_directory_name(name: &str) -> bool {
    name.get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("DISABLED"))
}

fn enabled_directory_name(name: &str) -> &str {
    if !is_disabled_directory_name(name) {
        return name;
    }
    name.get(8..)
        .unwrap_or_default()
        .trim_start_matches(['_', '-', ' '])
}

#[async_trait]
impl ModScanner for FileSystemModScanner {
    async fn scan_repository(
        &self,
        repository_root: RepositoryRoot,
        cache: ScanCache,
    ) -> Result<RepositoryScan, AppError> {
        let scanner = self.clone();
        tokio::task::spawn_blocking(move || scanner.scan_repository_sync(repository_root, cache))
            .await?
    }

    async fn scan_candidate(&self, candidate_root: &Path) -> Result<ScannedMod, AppError> {
        let root = candidate_root.to_path_buf();
        let scanner = self.clone();
        tokio::task::spawn_blocking(move || {
            if !root.is_absolute() || path_is_link_or_reparse_point(&root)? {
                return Err(AppError::UnsafePath(
                    "候选模组根目录必须是非链接的绝对目录。".to_owned(),
                ));
            }
            let root =
                fs::canonicalize(&root).map_err(|source| AppError::file_system(&root, source))?;
            if !root.is_dir() {
                return Err(AppError::ModScan("候选模组根路径不是目录。".to_owned()));
            }
            let name = root
                .file_name()
                .ok_or_else(|| AppError::ModScan("候选模组目录没有名称。".to_owned()))?;
            let relative = RepositoryRelativePath::new(PathBuf::from(name))?;
            let mut hashed_files = 0;
            let mut reused_hashes = 0;
            let mut skipped_entries = 0;
            scanner.scan_mod_sync(
                root,
                relative,
                &ScanCache::new(),
                ScanCounters {
                    hashed_files: &mut hashed_files,
                    reused_hashes: &mut reused_hashes,
                    skipped_entries: &mut skipped_entries,
                },
            )
        })
        .await?
    }
}

struct ScanCounters<'a> {
    hashed_files: &'a mut u64,
    reused_hashes: &'a mut u64,
    skipped_entries: &'a mut u64,
}

fn modified_at_nanos(metadata: &fs::Metadata) -> Result<i64, AppError> {
    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let duration = modified.duration_since(UNIX_EPOCH).unwrap_or_default();
    i64::try_from(duration.as_nanos())
        .map_err(|_| AppError::DataIntegrity("文件修改时间超过支持范围。".to_owned()))
}

fn hash_file(path: &Path) -> Result<String, AppError> {
    let mut file = File::open(path).map_err(|source| AppError::file_system(path, source))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; HASH_BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| AppError::file_system(path, source))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn fingerprint_files(files: &[ModFile]) -> String {
    let mut hasher = blake3::Hasher::new();
    for file in files {
        hasher.update(
            file.source_path
                .to_string_lossy()
                .replace('\\', "/")
                .as_bytes(),
        );
        hasher.update(&[0]);
        hasher.update(&file.size_bytes.to_le_bytes());
        hasher.update(&[0]);
        match &file.content_hash {
            Some(hash) => hasher.update(hash.as_bytes()),
            None => hasher.update(b"unreadable"),
        };
        hasher.update(&[0xff]);
    }
    hasher.finalize().to_hex().to_string()
}

fn classify_file_role(path: &Path) -> String {
    if path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("mod.json"))
    {
        return "metadata".to_owned();
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "ini" => "configuration",
        "png" | "jpg" | "jpeg" | "webp" => "image",
        "dds" => "texture",
        "buf" | "ib" | "vb" => "model",
        _ => "content",
    }
    .to_owned()
}

fn resolve_duplicate_logical_ids(mods: &mut [ScannedMod]) -> Result<(), AppError> {
    let mut groups = HashMap::<String, Vec<usize>>::new();
    for (index, scanned_mod) in mods.iter().enumerate() {
        groups
            .entry(scanned_mod.author_metadata.logical_id.to_lowercase())
            .or_default()
            .push(index);
    }
    for indexes in groups.values().filter(|indexes| indexes.len() > 1) {
        if indexes
            .iter()
            .any(|index| mods[*index].author_metadata.source_kind == MetadataSourceKind::ModJson)
        {
            let paths = indexes
                .iter()
                .map(|index| mods[*index].repository_path.storage_key())
                .collect::<Vec<_>>()
                .join("、");
            return Err(AppError::ModMetadata(format!(
                "模组目录 {paths} 使用了相同的作者 ID，请修正后重新扫描。"
            )));
        }
        for index in indexes {
            let repository_path = mods[*index].repository_path.storage_key();
            let digest = blake3::hash(repository_path.to_lowercase().as_bytes());
            mods[*index].author_metadata.logical_id = format!("local.{}", &digest.to_hex()[..20]);
            mods[*index].issues.push(ScanIssue {
                level: ScanIssueLevel::Warning,
                repository_path: Some(repository_path),
                message: "存在同名启用/禁用目录，已按实际目录分别建立本地身份。".to_owned(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs, time::Duration};

    use crate::core::mods::{
        CachedModFile, FileSystemModScanner, ModScanner, RepositoryInitializationPolicy,
        RepositoryRoot, ScanCache,
    };

    fn cache_from_scan(scan: &super::RepositoryScan) -> ScanCache {
        let mut cache = HashMap::new();
        for scanned_mod in &scan.mods {
            for file in &scanned_mod.files {
                if let Some(content_hash) = &file.content_hash {
                    cache.insert(
                        (
                            scanned_mod.repository_path.storage_key(),
                            file.source_path.to_string_lossy().replace('\\', "/"),
                        ),
                        CachedModFile {
                            size_bytes: file.size_bytes,
                            modified_at: file.modified_at,
                            content_hash: content_hash.clone(),
                        },
                    );
                }
            }
        }
        cache
    }

    #[tokio::test]
    async fn reuses_hashes_for_unchanged_files() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let root = RepositoryRoot::open_or_initialize(
            directory.path(),
            RepositoryInitializationPolicy::EmptyOnly,
        )?;
        fs::create_dir(root.path().join("example"))?;
        fs::write(root.path().join("example/mod.ini"), b"[TextureOverride]")?;
        fs::write(root.path().join("example/model.buf"), b"model")?;

        let scanner = FileSystemModScanner::new();
        let first = scanner
            .scan_repository(root.clone(), ScanCache::new())
            .await?;
        assert_eq!(first.hashed_files, 2);
        let second = scanner
            .scan_repository(root, cache_from_scan(&first))
            .await?;
        assert_eq!(second.hashed_files, 0);
        assert_eq!(second.reused_hashes, 2);
        Ok(())
    }

    #[tokio::test]
    async fn scans_one_thousand_mod_fixture_within_regression_budget()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let root = RepositoryRoot::open_or_initialize(
            directory.path(),
            RepositoryInitializationPolicy::EmptyOnly,
        )?;
        for index in 0..1_000 {
            let mod_root = root.path().join(format!("mod-{index:04}"));
            fs::create_dir(&mod_root)?;
            fs::write(mod_root.join("content.ini"), format!("hash={index}"))?;
        }

        let scan = FileSystemModScanner::new()
            .scan_repository(root, ScanCache::new())
            .await?;
        assert_eq!(scan.mods.len(), 1_000);
        assert_eq!(scan.hashed_files, 1_000);
        assert!(scan.duration < Duration::from_secs(30));
        Ok(())
    }

    #[tokio::test]
    async fn rejects_case_insensitive_duplicate_author_ids()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let root = RepositoryRoot::open_or_initialize(
            directory.path(),
            RepositoryInitializationPolicy::EmptyOnly,
        )?;
        for (directory_name, logical_id) in
            [("first", "Author.Example"), ("second", "author.example")]
        {
            let mod_root = root.path().join(directory_name);
            fs::create_dir(&mod_root)?;
            fs::write(
                mod_root.join("mod.json"),
                format!(r#"{{"id":"{logical_id}","name":"Example"}}"#),
            )?;
        }

        let result = FileSystemModScanner::new()
            .scan_repository(root, ScanCache::new())
            .await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn keeps_same_named_active_and_disabled_folders_as_separate_local_mods()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let root = RepositoryRoot::open_or_initialize(
            directory.path(),
            RepositoryInitializationPolicy::EmptyOnly,
        )?;
        for name in ["Example Mod", "DISABLED_Example Mod"] {
            fs::create_dir(root.path().join(name))?;
            fs::write(root.path().join(name).join("main.ini"), b"[Constants]\n")?;
        }

        let scan = FileSystemModScanner::new()
            .scan_repository(root, ScanCache::new())
            .await?;
        assert_eq!(scan.mods.len(), 2);
        assert_ne!(
            scan.mods[0].author_metadata.logical_id,
            scan.mods[1].author_metadata.logical_id
        );
        assert_eq!(
            scan.mods.iter().filter(|item| item.enabled_in_efmi).count(),
            1
        );
        Ok(())
    }
}
