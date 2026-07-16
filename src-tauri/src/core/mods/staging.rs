use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Serialize, de::DeserializeOwned};
use tempfile::NamedTempFile;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::errors::AppError;

use super::repository::path_is_link_or_reparse_point;

pub const STAGING_MARKER_FILE: &str = ".aemm-staging.json";
pub const OPERATION_MARKER_FILE: &str = ".aemm-operation.json";
const STAGING_MARKER_KIND: &str = "aemm-mod-staging";
const OPERATION_MARKER_KIND: &str = "aemm-mod-install-operation";
const MARKER_VERSION: u32 = 1;
const MAX_JSON_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StagingInitializationPolicy {
    EmptyOnly,
    TrustedAemmDefault,
}

#[derive(Debug, Clone)]
pub struct StagingRoot {
    canonical_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct StagingOperation {
    operation_id: Uuid,
    path: PathBuf,
}

impl StagingRoot {
    pub fn open_or_initialize(
        path: &Path,
        policy: StagingInitializationPolicy,
    ) -> Result<Self, AppError> {
        if !path.is_absolute() || path.as_os_str().is_empty() {
            return Err(AppError::UnsafePath(
                "安装暂存目录必须是非空的绝对路径。".to_owned(),
            ));
        }
        if !path.exists() {
            fs::create_dir_all(path).map_err(|source| AppError::file_system(path, source))?;
        }
        if path_is_link_or_reparse_point(path)? {
            return Err(AppError::UnsafePath(
                "安装暂存目录不能是链接、目录联接或其他重解析点。".to_owned(),
            ));
        }
        let canonical_path =
            fs::canonicalize(path).map_err(|source| AppError::file_system(path, source))?;
        if !canonical_path.is_dir() {
            return Err(AppError::UnsafePath("安装暂存路径不是目录。".to_owned()));
        }

        let root = Self { canonical_path };
        let marker_path = root.canonical_path.join(STAGING_MARKER_FILE);
        if marker_path.exists() {
            validate_marker(
                &marker_path,
                STAGING_MARKER_KIND,
                None,
                "安装暂存目录所有权标记",
            )?;
            return Ok(root);
        }
        if policy == StagingInitializationPolicy::EmptyOnly
            && directory_has_entries(&root.canonical_path)?
        {
            return Err(AppError::UnsafePath(
                "拒绝接管缺少 AEMM 所有权标记的非空暂存目录。".to_owned(),
            ));
        }
        write_marker(
            &marker_path,
            &OwnershipMarker {
                kind: STAGING_MARKER_KIND.to_owned(),
                schema_version: MARKER_VERSION,
                operation_id: None,
            },
        )?;
        tracing::info!(staging_root = %root.canonical_path.display(), "initialized AEMM staging ownership marker");
        Ok(root)
    }

    pub fn path(&self) -> &Path {
        &self.canonical_path
    }

    pub fn create_operation(&self, operation_id: Uuid) -> Result<StagingOperation, AppError> {
        let path = self.canonical_path.join(operation_id.to_string());
        fs::create_dir(&path).map_err(|source| AppError::file_system(&path, source))?;
        let marker_path = path.join(OPERATION_MARKER_FILE);
        if let Err(error) = write_marker(
            &marker_path,
            &OwnershipMarker {
                kind: OPERATION_MARKER_KIND.to_owned(),
                schema_version: MARKER_VERSION,
                operation_id: Some(operation_id),
            },
        ) {
            if let Err(cleanup_error) = fs::remove_dir(&path) {
                tracing::error!(operation_id = %operation_id, error = %cleanup_error, "failed to remove incomplete staging operation directory");
            }
            return Err(error);
        }
        Ok(StagingOperation { operation_id, path })
    }

    pub fn operation(&self, operation_id: Uuid) -> Result<StagingOperation, AppError> {
        let path = self.canonical_path.join(operation_id.to_string());
        if !path.exists() || path_is_link_or_reparse_point(&path)? {
            return Err(AppError::NotAvailable(
                "安装操作不存在或暂存目录不安全。".to_owned(),
            ));
        }
        let canonical =
            fs::canonicalize(&path).map_err(|source| AppError::file_system(&path, source))?;
        if canonical.parent() != Some(self.canonical_path.as_path()) || !canonical.is_dir() {
            return Err(AppError::UnsafePath(
                "安装操作必须是 AEMM 暂存目录的直属子目录。".to_owned(),
            ));
        }
        validate_marker(
            &canonical.join(OPERATION_MARKER_FILE),
            OPERATION_MARKER_KIND,
            Some(operation_id),
            "安装操作所有权标记",
        )?;
        Ok(StagingOperation {
            operation_id,
            path: canonical,
        })
    }

    pub fn operation_ids(&self) -> Result<Vec<Uuid>, AppError> {
        let mut result = Vec::new();
        for entry in fs::read_dir(&self.canonical_path)
            .map_err(|source| AppError::file_system(&self.canonical_path, source))?
        {
            let entry =
                entry.map_err(|source| AppError::file_system(&self.canonical_path, source))?;
            if entry.file_name().to_string_lossy() == STAGING_MARKER_FILE {
                continue;
            }
            let Ok(operation_id) = Uuid::parse_str(entry.file_name().to_string_lossy().as_ref())
            else {
                continue;
            };
            match self.operation(operation_id) {
                Ok(_) => result.push(operation_id),
                Err(error) => {
                    tracing::warn!(operation_id = %operation_id, error = %error, "ignored invalid staging operation")
                }
            }
        }
        result.sort_unstable();
        Ok(result)
    }

    pub fn remove_operation(&self, operation_id: Uuid) -> Result<(), AppError> {
        let operation = self.operation(operation_id)?;
        remove_owned_tree(&operation.path)
    }
}

impl StagingOperation {
    pub fn operation_id(&self) -> Uuid {
        self.operation_id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn payload_path(&self) -> PathBuf {
        self.path.join("payload")
    }

    pub fn write_json<T: Serialize>(&self, file_name: &str, value: &T) -> Result<(), AppError> {
        validate_operation_file_name(file_name)?;
        let mut contents = serde_json::to_vec_pretty(value).map_err(AppError::ConfigFormat)?;
        contents.push(b'\n');
        if u64::try_from(contents.len()).unwrap_or(u64::MAX) > MAX_JSON_BYTES {
            return Err(AppError::DataIntegrity(
                "安装操作记录超过允许大小。".to_owned(),
            ));
        }
        atomic_replace(&self.path.join(file_name), &contents)
    }

    pub fn read_json<T: DeserializeOwned>(&self, file_name: &str) -> Result<T, AppError> {
        validate_operation_file_name(file_name)?;
        let path = self.path.join(file_name);
        if path_is_link_or_reparse_point(&path)? {
            return Err(AppError::UnsafePath(
                "安装操作记录不能是链接或重解析点。".to_owned(),
            ));
        }
        let metadata =
            fs::metadata(&path).map_err(|source| AppError::file_system(&path, source))?;
        if !metadata.is_file() || metadata.len() > MAX_JSON_BYTES {
            return Err(AppError::DataIntegrity(
                "安装操作记录无效或大小异常。".to_owned(),
            ));
        }
        let mut file = File::open(&path).map_err(|source| AppError::file_system(&path, source))?;
        let mut contents = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
        file.read_to_end(&mut contents)
            .map_err(|source| AppError::file_system(&path, source))?;
        serde_json::from_slice(&contents).map_err(AppError::ConfigFormat)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OwnershipMarker {
    kind: String,
    schema_version: u32,
    operation_id: Option<Uuid>,
}

fn write_marker(path: &Path, marker: &OwnershipMarker) -> Result<(), AppError> {
    let mut contents = serde_json::to_vec_pretty(marker).map_err(AppError::ConfigFormat)?;
    contents.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| AppError::file_system(path, source))?;
    file.write_all(&contents)
        .map_err(|source| AppError::file_system(path, source))?;
    file.sync_all()
        .map_err(|source| AppError::file_system(path, source))
}

fn validate_marker(
    path: &Path,
    expected_kind: &str,
    expected_operation_id: Option<Uuid>,
    label: &str,
) -> Result<(), AppError> {
    if path_is_link_or_reparse_point(path)? {
        return Err(AppError::UnsafePath(format!(
            "{label}不能是链接或重解析点。"
        )));
    }
    let metadata = fs::metadata(path).map_err(|source| AppError::file_system(path, source))?;
    if !metadata.is_file() || metadata.len() > MAX_JSON_BYTES {
        return Err(AppError::UnsafePath(format!("{label}无效或大小异常。")));
    }
    let marker: OwnershipMarker = serde_json::from_reader(
        File::open(path).map_err(|source| AppError::file_system(path, source))?,
    )
    .map_err(AppError::ConfigFormat)?;
    if marker.kind != expected_kind
        || marker.schema_version != MARKER_VERSION
        || marker.operation_id != expected_operation_id
    {
        return Err(AppError::UnsafePath(format!("{label}内容无效。")));
    }
    Ok(())
}

fn validate_operation_file_name(file_name: &str) -> Result<(), AppError> {
    if file_name.is_empty()
        || file_name.contains(['/', '\\', '\0'])
        || file_name == OPERATION_MARKER_FILE
    {
        return Err(AppError::UnsafePath("安装操作记录文件名无效。".to_owned()));
    }
    Ok(())
}

fn atomic_replace(path: &Path, contents: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| {
        AppError::PathResolution(format!("{} has no parent directory", path.display()))
    })?;
    let mut temporary =
        NamedTempFile::new_in(parent).map_err(|source| AppError::file_system(parent, source))?;
    temporary
        .write_all(contents)
        .map_err(|source| AppError::file_system(path, source))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|source| AppError::file_system(path, source))?;

    let backup = path.with_extension("json.bak");
    if backup.exists() {
        fs::remove_file(&backup).map_err(|source| AppError::file_system(&backup, source))?;
    }
    let had_existing = path.exists();
    if had_existing {
        fs::rename(path, &backup).map_err(|source| AppError::file_system(path, source))?;
    }
    if let Err(error) = temporary.persist(path) {
        if had_existing && backup.exists() {
            fs::rename(&backup, path).map_err(|source| AppError::file_system(path, source))?;
        }
        return Err(AppError::file_system(path, error.error));
    }
    if had_existing && backup.exists() {
        fs::remove_file(&backup).map_err(|source| AppError::file_system(&backup, source))?;
    }
    Ok(())
}

fn remove_owned_tree(root: &Path) -> Result<(), AppError> {
    if path_is_link_or_reparse_point(root)? {
        return Err(AppError::UnsafePath(
            "拒绝删除链接或重解析点形式的安装操作目录。".to_owned(),
        ));
    }
    let entries = WalkDir::new(root)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::UnsafePath(format!("无法安全遍历安装操作目录：{error}")))?;
    for entry in &entries {
        if path_is_link_or_reparse_point(entry.path())? {
            return Err(AppError::UnsafePath(format!(
                "安装操作目录包含链接或重解析点 {}，已拒绝删除。",
                entry.path().display()
            )));
        }
    }
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
    fs::remove_dir(root).map_err(|source| AppError::file_system(root, source))
}

fn directory_has_entries(path: &Path) -> Result<bool, AppError> {
    let mut entries = fs::read_dir(path).map_err(|source| AppError::file_system(path, source))?;
    match entries.next() {
        Some(Ok(_)) => Ok(true),
        Some(Err(source)) => Err(AppError::file_system(path, source)),
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{StagingInitializationPolicy, StagingRoot};

    #[test]
    fn creates_and_removes_only_owned_operation_directories()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let root = StagingRoot::open_or_initialize(
            directory.path(),
            StagingInitializationPolicy::EmptyOnly,
        )?;
        let operation_id = uuid::Uuid::new_v4();
        let operation = root.create_operation(operation_id)?;
        fs::create_dir(operation.payload_path())?;
        fs::write(operation.payload_path().join("file.ini"), b"safe")?;

        root.remove_operation(operation_id)?;
        assert!(!operation.path().exists());
        assert!(root.path().exists());
        Ok(())
    }

    #[test]
    fn refuses_non_empty_unowned_custom_staging() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        fs::write(directory.path().join("unrelated.txt"), b"user data")?;
        assert!(
            StagingRoot::open_or_initialize(
                directory.path(),
                StagingInitializationPolicy::EmptyOnly
            )
            .is_err()
        );
        assert!(directory.path().join("unrelated.txt").exists());
        Ok(())
    }
}
