use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{errors::AppError, utils::validate_relative_path};

pub const REPOSITORY_MARKER_FILE: &str = ".aemm-repository.json";
const REPOSITORY_MARKER_KIND: &str = "aemm-mod-repository";
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
