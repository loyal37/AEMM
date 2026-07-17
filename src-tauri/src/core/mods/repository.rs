use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::{errors::AppError, utils::validate_relative_path};

pub const REPOSITORY_MARKER_FILE: &str = ".aemm-repository.json";
pub const REMOVAL_TOMBSTONE_PREFIX: &str = ".aemm-remove-";
const REMOVAL_MARKER_FILE: &str = ".aemm-removal.json";
const REPOSITORY_MARKER_KIND: &str = "aemm-mod-repository";
const REMOVAL_MARKER_KIND: &str = "aemm-mod-removal";
const REPOSITORY_MARKER_VERSION: u32 = 1;
const MAX_MARKER_BYTES: u64 = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryInitializationPolicy {
    EmptyOnly,
    TrustedAemmDefault,
}

#[derive(Debug, Clone)]
pub struct RepositoryRoot {
    canonical_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRemoval {
    mod_id: Uuid,
    original_relative: RepositoryRelativePath,
    tombstone_relative: RepositoryRelativePath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingRemovalRecovery {
    OriginalPresent,
    Restored,
    Missing,
}

impl RepositoryRemoval {
    pub fn new(
        mod_id: Uuid,
        original_relative: RepositoryRelativePath,
        tombstone_relative: RepositoryRelativePath,
    ) -> Result<Self, AppError> {
        if !original_relative.is_direct_child()
            || !tombstone_relative.is_direct_child()
            || tombstone_relative.storage_key() != removal_tombstone_name(mod_id)
        {
            return Err(AppError::UnsafePath(
                "模组移除路径必须是匹配 UUID 的仓库直属目录。".to_owned(),
            ));
        }
        Ok(Self {
            mod_id,
            original_relative,
            tombstone_relative,
        })
    }

    pub fn mod_id(&self) -> Uuid {
        self.mod_id
    }

    pub fn original_relative(&self) -> &RepositoryRelativePath {
        &self.original_relative
    }

    pub fn tombstone_relative(&self) -> &RepositoryRelativePath {
        &self.tombstone_relative
    }
}

impl RepositoryRoot {
    pub fn open_or_initialize(
        path: &Path,
        policy: RepositoryInitializationPolicy,
    ) -> Result<Self, AppError> {
        if !path.is_absolute() || path.as_os_str().is_empty() {
            return Err(AppError::UnsafePath(
                "模组仓库必须是非空的绝对路径。".to_owned(),
            ));
        }

        if !path.exists() {
            fs::create_dir_all(path).map_err(|source| AppError::file_system(path, source))?;
        }
        if path_is_link_or_reparse_point(path)? {
            return Err(AppError::UnsafePath(
                "模组仓库根目录不能是符号链接、目录联接或其他重解析点。".to_owned(),
            ));
        }

        let canonical_path =
            fs::canonicalize(path).map_err(|source| AppError::file_system(path, source))?;
        if !canonical_path.is_dir() {
            return Err(AppError::UnsafePath("模组仓库路径不是目录。".to_owned()));
        }

        let root = Self { canonical_path };
        let marker_path = root.canonical_path.join(REPOSITORY_MARKER_FILE);
        if marker_path.exists() {
            root.validate_marker(&marker_path)?;
            return Ok(root);
        }

        if policy == RepositoryInitializationPolicy::EmptyOnly
            && directory_has_entries(&root.canonical_path)?
        {
            return Err(AppError::UnsafePath(
                "拒绝接管缺少 AEMM 所有权标记的非空目录；请使用空目录或已初始化的仓库。".to_owned(),
            ));
        }

        root.create_marker(&marker_path)?;
        Ok(root)
    }

    pub fn path(&self) -> &Path {
        &self.canonical_path
    }

    pub fn resolve_existing(&self, relative: &RepositoryRelativePath) -> Result<PathBuf, AppError> {
        let candidate = self.canonical_path.join(relative.as_path());
        let mut current = self.canonical_path.clone();
        for component in relative.as_path().components() {
            current.push(component.as_os_str());
            if path_is_link_or_reparse_point(&current)? {
                return Err(AppError::UnsafePath(format!(
                    "仓库条目 {} 包含不允许的链接或重解析点。",
                    relative.storage_key()
                )));
            }
        }
        let canonical = fs::canonicalize(&candidate)
            .map_err(|source| AppError::file_system(&candidate, source))?;
        if canonical == self.canonical_path || !canonical.starts_with(&self.canonical_path) {
            return Err(AppError::UnsafePath(
                "仓库条目解析到了仓库根目录之外。".to_owned(),
            ));
        }
        Ok(canonical)
    }

    pub fn resolve_existing_mod_root(
        &self,
        relative: &RepositoryRelativePath,
    ) -> Result<PathBuf, AppError> {
        if !relative.is_direct_child() || relative.storage_key() == REPOSITORY_MARKER_FILE {
            return Err(AppError::UnsafePath(
                "模组根目录必须是仓库中的直属子目录。".to_owned(),
            ));
        }
        let path = self.resolve_existing(relative)?;
        if !path.is_dir() {
            return Err(AppError::UnsafePath("模组根路径不是目录。".to_owned()));
        }
        Ok(path)
    }

    pub fn planned_mod_path(&self, relative: &RepositoryRelativePath) -> Result<PathBuf, AppError> {
        if !relative.is_direct_child() || relative.storage_key() == REPOSITORY_MARKER_FILE {
            return Err(AppError::UnsafePath(
                "安装目标必须是仓库中的直属模组目录。".to_owned(),
            ));
        }
        Ok(self.canonical_path.join(relative.as_path()))
    }

    pub fn remove_mod_root(&self, relative: &RepositoryRelativePath) -> Result<(), AppError> {
        let root = self.resolve_existing_mod_root(relative)?;
        let entries = validated_tree_entries(&root, "待删除的仓库目录")?;
        for entry in entries {
            let path = entry.path();
            if path == root {
                continue;
            }
            if entry.file_type().is_dir() {
                fs::remove_dir(path).map_err(|source| AppError::file_system(path, source))?;
            } else {
                fs::remove_file(path).map_err(|source| AppError::file_system(path, source))?;
            }
        }
        fs::remove_dir(&root).map_err(|source| AppError::file_system(&root, source))
    }

    pub fn quarantine_mod_root(
        &self,
        relative: &RepositoryRelativePath,
        mod_id: Uuid,
    ) -> Result<Option<RepositoryRemoval>, AppError> {
        if !relative.is_direct_child() {
            return Err(AppError::UnsafePath(
                "待卸载模组必须是仓库直属目录。".to_owned(),
            ));
        }
        let candidate = self.planned_mod_path(relative)?;
        if !path_entry_exists(&candidate)? {
            return Ok(None);
        }
        let root = self.resolve_existing_mod_root(relative)?;
        validated_tree_entries(&root, "待卸载模组目录")?;
        if path_entry_exists(&root.join(REMOVAL_MARKER_FILE))? {
            return Err(AppError::UnsafePath(
                "模组目录包含 AEMM 保留的移除标记文件，已拒绝卸载。".to_owned(),
            ));
        }

        let tombstone_relative = RepositoryRelativePath::new(removal_tombstone_name(mod_id))?;
        let removal = RepositoryRemoval::new(mod_id, relative.clone(), tombstone_relative)?;
        let tombstone = self.planned_mod_path(removal.tombstone_relative())?;
        if path_entry_exists(&tombstone)? {
            return Err(AppError::UnsafePath(format!(
                "模组 {mod_id} 的移除暂存目录已存在；请先完成恢复。"
            )));
        }
        fs::rename(&root, &tombstone).map_err(|source| AppError::file_system(&root, source))?;
        if let Err(error) = write_removal_marker(&tombstone, &removal) {
            let marker = tombstone.join(REMOVAL_MARKER_FILE);
            if marker.is_file()
                && let Err(cleanup_error) = fs::remove_file(&marker)
            {
                tracing::error!(path = %marker.display(), error = %cleanup_error, "failed to remove incomplete mod-removal marker");
            }
            if let Err(restore_error) = fs::rename(&tombstone, &root) {
                tracing::error!(mod_id = %mod_id, error = %restore_error, "failed to restore mod after removal-marker write failure");
                return Err(AppError::file_system(&root, restore_error));
            }
            return Err(error);
        }
        Ok(Some(removal))
    }

    pub fn restore_quarantined_mod(
        &self,
        removal: &RepositoryRemoval,
        allow_missing_marker: bool,
    ) -> Result<(), AppError> {
        let original = self.planned_mod_path(removal.original_relative())?;
        let tombstone = self.planned_mod_path(removal.tombstone_relative())?;
        if path_entry_exists(&original)? {
            return Err(AppError::UnsafePath(
                "恢复模组时原路径已被占用；已保留移除暂存目录。".to_owned(),
            ));
        }
        if !path_entry_exists(&tombstone)? || path_is_link_or_reparse_point(&tombstone)? {
            return Err(AppError::UnsafePath(
                "恢复模组所需的移除暂存目录缺失或不安全。".to_owned(),
            ));
        }
        let marker_path = tombstone.join(REMOVAL_MARKER_FILE);
        let marker_existed = path_entry_exists(&marker_path)?;
        if marker_existed {
            validate_removal_marker(&marker_path, removal)?;
            fs::remove_file(&marker_path)
                .map_err(|source| AppError::file_system(&marker_path, source))?;
        } else if !allow_missing_marker {
            return Err(AppError::UnsafePath(
                "移除暂存目录缺少 AEMM 所有权标记。".to_owned(),
            ));
        }
        if let Err(source) = fs::rename(&tombstone, &original) {
            if marker_existed && let Err(marker_error) = write_removal_marker(&tombstone, removal) {
                tracing::error!(mod_id = %removal.mod_id(), error = %marker_error, "failed to restore removal marker after rollback rename failure");
            }
            return Err(AppError::file_system(&original, source));
        }
        Ok(())
    }

    pub fn inspect_removal_tombstone(
        &self,
        relative: &RepositoryRelativePath,
    ) -> Result<RepositoryRemoval, AppError> {
        if !relative.is_direct_child()
            || !relative.storage_key().starts_with(REMOVAL_TOMBSTONE_PREFIX)
        {
            return Err(AppError::UnsafePath(
                "移除暂存目录名称不受 AEMM 管理。".to_owned(),
            ));
        }
        let tombstone = self.resolve_existing_mod_root(relative)?;
        read_removal_marker(&tombstone.join(REMOVAL_MARKER_FILE), relative)
    }

    pub fn recover_pending_removal(
        &self,
        removal: &RepositoryRemoval,
    ) -> Result<PendingRemovalRecovery, AppError> {
        let original = self.planned_mod_path(removal.original_relative())?;
        let tombstone = self.planned_mod_path(removal.tombstone_relative())?;
        let original_exists = path_entry_exists(&original)?;
        let tombstone_exists = path_entry_exists(&tombstone)?;
        match (original_exists, tombstone_exists) {
            (true, false) => {
                self.resolve_existing_mod_root(removal.original_relative())?;
                Ok(PendingRemovalRecovery::OriginalPresent)
            }
            (false, true) => {
                self.restore_quarantined_mod(removal, true)?;
                Ok(PendingRemovalRecovery::Restored)
            }
            (false, false) => Ok(PendingRemovalRecovery::Missing),
            (true, true) => Err(AppError::UnsafePath(
                "待恢复模组的原目录和移除暂存目录同时存在；已保留两者供人工检查。".to_owned(),
            )),
        }
    }

    pub fn removal_tombstones(&self) -> Result<Vec<RepositoryRelativePath>, AppError> {
        let mut result = Vec::new();
        for entry in fs::read_dir(&self.canonical_path)
            .map_err(|source| AppError::file_system(&self.canonical_path, source))?
        {
            let entry =
                entry.map_err(|source| AppError::file_system(&self.canonical_path, source))?;
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with(REMOVAL_TOMBSTONE_PREFIX) {
                continue;
            }
            result.push(RepositoryRelativePath::new(PathBuf::from(name))?);
        }
        result.sort_by_key(RepositoryRelativePath::storage_key);
        Ok(result)
    }

    pub fn has_repository_content(&self) -> Result<bool, AppError> {
        for entry in fs::read_dir(&self.canonical_path)
            .map_err(|source| AppError::file_system(&self.canonical_path, source))?
        {
            let entry =
                entry.map_err(|source| AppError::file_system(&self.canonical_path, source))?;
            if entry.file_name() != REPOSITORY_MARKER_FILE {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn finalize_quarantined_mod(&self, removal: &RepositoryRemoval) -> Result<(), AppError> {
        let tombstone = self.resolve_existing_mod_root(removal.tombstone_relative())?;
        let marker_path = tombstone.join(REMOVAL_MARKER_FILE);
        validate_removal_marker(&marker_path, removal)?;
        let entries = validated_tree_entries(&tombstone, "模组移除暂存目录")?;
        for entry in entries {
            let path = entry.path();
            if path == tombstone || path == marker_path {
                continue;
            }
            if entry.file_type().is_dir() {
                fs::remove_dir(path).map_err(|source| AppError::file_system(path, source))?;
            } else {
                fs::remove_file(path).map_err(|source| AppError::file_system(path, source))?;
            }
        }
        let unexpected = fs::read_dir(&tombstone)
            .map_err(|source| AppError::file_system(&tombstone, source))?
            .filter_map(Result::ok)
            .any(|entry| entry.file_name() != REMOVAL_MARKER_FILE);
        if unexpected {
            return Err(AppError::UnsafePath(
                "移除清理期间出现未授权的新条目；已保留所有权标记。".to_owned(),
            ));
        }
        fs::remove_file(&marker_path)
            .map_err(|source| AppError::file_system(&marker_path, source))?;
        if let Err(source) = fs::remove_dir(&tombstone) {
            if let Err(marker_error) = write_removal_marker(&tombstone, removal) {
                tracing::error!(mod_id = %removal.mod_id(), error = %marker_error, "failed to restore removal marker after final directory cleanup failure");
            }
            return Err(AppError::file_system(&tombstone, source));
        }
        Ok(())
    }

    fn create_marker(&self, marker_path: &Path) -> Result<(), AppError> {
        let marker = RepositoryMarker {
            kind: REPOSITORY_MARKER_KIND.to_owned(),
            schema_version: REPOSITORY_MARKER_VERSION,
        };
        let mut contents = serde_json::to_vec_pretty(&marker).map_err(AppError::ConfigFormat)?;
        contents.push(b'\n');

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(marker_path)
        {
            Ok(mut file) => {
                file.write_all(&contents)
                    .map_err(|source| AppError::file_system(marker_path, source))?;
                file.sync_all()
                    .map_err(|source| AppError::file_system(marker_path, source))?;
                tracing::info!(repository_root = %self.canonical_path.display(), "initialized AEMM repository ownership marker");
                Ok(())
            }
            Err(source) if source.kind() == ErrorKind::AlreadyExists => {
                self.validate_marker(marker_path)
            }
            Err(source) => Err(AppError::file_system(marker_path, source)),
        }
    }

    fn validate_marker(&self, marker_path: &Path) -> Result<(), AppError> {
        if path_is_link_or_reparse_point(marker_path)? {
            return Err(AppError::UnsafePath(
                "模组仓库所有权标记不能是链接或重解析点。".to_owned(),
            ));
        }
        let metadata = fs::metadata(marker_path)
            .map_err(|source| AppError::file_system(marker_path, source))?;
        if !metadata.is_file() || metadata.len() > MAX_MARKER_BYTES {
            return Err(AppError::UnsafePath(
                "模组仓库所有权标记无效或大小异常。".to_owned(),
            ));
        }
        let file =
            File::open(marker_path).map_err(|source| AppError::file_system(marker_path, source))?;
        let marker: RepositoryMarker =
            serde_json::from_reader(file).map_err(AppError::ConfigFormat)?;
        if marker.kind != REPOSITORY_MARKER_KIND
            || marker.schema_version != REPOSITORY_MARKER_VERSION
        {
            return Err(AppError::UnsafePath(
                "模组仓库所有权标记类型或版本不受支持。".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemovalMarker {
    kind: String,
    schema_version: u32,
    mod_id: Uuid,
    original_repository_path: String,
    tombstone_repository_path: String,
}

fn removal_tombstone_name(mod_id: Uuid) -> String {
    format!("{REMOVAL_TOMBSTONE_PREFIX}{mod_id}")
}

fn write_removal_marker(root: &Path, removal: &RepositoryRemoval) -> Result<(), AppError> {
    let marker_path = root.join(REMOVAL_MARKER_FILE);
    let marker = RemovalMarker {
        kind: REMOVAL_MARKER_KIND.to_owned(),
        schema_version: REPOSITORY_MARKER_VERSION,
        mod_id: removal.mod_id(),
        original_repository_path: removal.original_relative().storage_key(),
        tombstone_repository_path: removal.tombstone_relative().storage_key(),
    };
    let mut contents = serde_json::to_vec_pretty(&marker).map_err(AppError::ConfigFormat)?;
    contents.push(b'\n');
    if u64::try_from(contents.len()).unwrap_or(u64::MAX) > MAX_MARKER_BYTES {
        return Err(AppError::DataIntegrity(
            "模组移除所有权标记超过安全尺寸限制。".to_owned(),
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker_path)
        .map_err(|source| AppError::file_system(&marker_path, source))?;
    file.write_all(&contents)
        .map_err(|source| AppError::file_system(&marker_path, source))?;
    file.sync_all()
        .map_err(|source| AppError::file_system(&marker_path, source))
}

fn read_removal_marker(
    marker_path: &Path,
    tombstone_relative: &RepositoryRelativePath,
) -> Result<RepositoryRemoval, AppError> {
    if path_is_link_or_reparse_point(marker_path)? {
        return Err(AppError::UnsafePath(
            "模组移除所有权标记不能是链接或重解析点。".to_owned(),
        ));
    }
    let metadata =
        fs::metadata(marker_path).map_err(|source| AppError::file_system(marker_path, source))?;
    if !metadata.is_file() || metadata.len() > MAX_MARKER_BYTES {
        return Err(AppError::UnsafePath(
            "模组移除所有权标记缺失、类型错误或尺寸异常。".to_owned(),
        ));
    }
    let marker: RemovalMarker = serde_json::from_reader(
        File::open(marker_path).map_err(|source| AppError::file_system(marker_path, source))?,
    )
    .map_err(AppError::ConfigFormat)?;
    if marker.kind != REMOVAL_MARKER_KIND
        || marker.schema_version != REPOSITORY_MARKER_VERSION
        || marker.tombstone_repository_path != tombstone_relative.storage_key()
    {
        return Err(AppError::UnsafePath(
            "模组移除所有权标记内容无效。".to_owned(),
        ));
    }
    RepositoryRemoval::new(
        marker.mod_id,
        RepositoryRelativePath::new(marker.original_repository_path)?,
        tombstone_relative.clone(),
    )
}

fn validate_removal_marker(
    marker_path: &Path,
    expected: &RepositoryRemoval,
) -> Result<(), AppError> {
    let actual = read_removal_marker(marker_path, expected.tombstone_relative())?;
    if actual != *expected {
        return Err(AppError::UnsafePath(
            "模组移除所有权标记与当前操作不一致。".to_owned(),
        ));
    }
    Ok(())
}

fn validated_tree_entries(root: &Path, label: &str) -> Result<Vec<walkdir::DirEntry>, AppError> {
    let entries = WalkDir::new(root)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::UnsafePath(format!("无法安全遍历{label}：{error}")))?;
    for entry in &entries {
        if path_is_link_or_reparse_point(entry.path())? {
            return Err(AppError::UnsafePath(format!(
                "{label}包含链接或重解析点 {}，已拒绝删除。",
                entry.path().display()
            )));
        }
    }
    Ok(entries)
}

fn path_entry_exists(path: &Path) -> Result<bool, AppError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(false),
        Err(source) => Err(AppError::file_system(path, source)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepositoryRelativePath(PathBuf);

impl RepositoryRelativePath {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, AppError> {
        let path = path.into();
        validate_relative_path(&path)?;
        if path
            .components()
            .any(|component| matches!(component, Component::CurDir))
        {
            return Err(AppError::UnsafePath(
                "仓库相对路径不能包含当前目录组件。".to_owned(),
            ));
        }
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn storage_key(&self) -> String {
        self.0.to_string_lossy().replace('\\', "/")
    }

    pub fn is_direct_child(&self) -> bool {
        self.0.components().count() == 1
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepositoryMarker {
    kind: String,
    schema_version: u32,
}

fn directory_has_entries(path: &Path) -> Result<bool, AppError> {
    let mut entries = fs::read_dir(path).map_err(|source| AppError::file_system(path, source))?;
    match entries.next() {
        Some(Ok(_)) => Ok(true),
        Some(Err(source)) => Err(AppError::file_system(path, source)),
        None => Ok(false),
    }
}

pub(crate) fn path_is_link_or_reparse_point(path: &Path) -> Result<bool, AppError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|source| AppError::file_system(path, source))?;
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        REPOSITORY_MARKER_FILE, RepositoryInitializationPolicy, RepositoryRelativePath,
        RepositoryRoot,
    };

    #[test]
    fn initializes_empty_repository_and_resolves_direct_mod_root()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let root = RepositoryRoot::open_or_initialize(
            directory.path(),
            RepositoryInitializationPolicy::EmptyOnly,
        )?;
        assert!(root.path().join(REPOSITORY_MARKER_FILE).is_file());

        fs::create_dir(root.path().join("example-mod"))?;
        let relative = RepositoryRelativePath::new("example-mod")?;
        assert_eq!(
            root.resolve_existing_mod_root(&relative)?,
            root.path().join("example-mod")
        );
        Ok(())
    }

    #[test]
    fn refuses_non_empty_unowned_directory() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        fs::write(directory.path().join("unrelated.txt"), b"user data")?;

        let result = RepositoryRoot::open_or_initialize(
            directory.path(),
            RepositoryInitializationPolicy::EmptyOnly,
        );
        assert!(result.is_err());
        assert!(!directory.path().join(REPOSITORY_MARKER_FILE).exists());
        Ok(())
    }

    #[test]
    fn rejects_nested_path_as_mod_root() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let root = RepositoryRoot::open_or_initialize(
            directory.path(),
            RepositoryInitializationPolicy::EmptyOnly,
        )?;
        fs::create_dir_all(root.path().join("author/mod"))?;

        let relative = RepositoryRelativePath::new("author/mod")?;
        assert!(root.resolve_existing_mod_root(&relative).is_err());
        Ok(())
    }

    #[test]
    fn rejects_tampered_repository_marker() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let root = RepositoryRoot::open_or_initialize(
            directory.path(),
            RepositoryInitializationPolicy::EmptyOnly,
        )?;
        fs::write(
            root.path().join(REPOSITORY_MARKER_FILE),
            br#"{"kind":"not-aemm","schemaVersion":1}"#,
        )?;

        assert!(
            RepositoryRoot::open_or_initialize(
                directory.path(),
                RepositoryInitializationPolicy::EmptyOnly,
            )
            .is_err()
        );
        Ok(())
    }
}
