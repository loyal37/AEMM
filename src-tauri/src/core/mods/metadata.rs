use std::{fs, path::Path, path::PathBuf};

use async_trait::async_trait;
use serde::Deserialize;

use crate::{
    core::mods::{ModMetadataManager, repository::path_is_link_or_reparse_point},
    errors::AppError,
    models::{AuthorModMetadata, MetadataSourceKind},
    utils::validate_relative_path,
};

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_ID_LENGTH: usize = 128;
const MAX_NAME_LENGTH: usize = 256;
const MAX_SHORT_FIELD_LENGTH: usize = 256;
const MAX_DESCRIPTION_LENGTH: usize = 32 * 1024;
const MAX_WEBSITE_LENGTH: usize = 2048;

#[derive(Debug, Clone, Default)]
pub struct FileSystemMetadataManager;

#[derive(Debug, Clone)]
pub struct MetadataRead {
    pub metadata: AuthorModMetadata,
    pub warnings: Vec<String>,
}

impl FileSystemMetadataManager {
    pub const fn new() -> Self {
        Self
    }

    pub fn read_with_warnings(
        &self,
        mod_root: &Path,
        repository_key: &str,
    ) -> Result<MetadataRead, AppError> {
        let manifest_path = mod_root.join("mod.json");
        if !manifest_path.exists() {
            return Ok(infer_metadata(mod_root, repository_key, None, Vec::new()));
        }
        if path_is_link_or_reparse_point(&manifest_path)? {
            return Ok(infer_metadata(
                mod_root,
                repository_key,
                None,
                vec!["mod.json 是链接或重解析点，已忽略并使用推断元数据。".to_owned()],
            ));
        }

        let metadata = fs::metadata(&manifest_path)
            .map_err(|source| AppError::file_system(&manifest_path, source))?;
        if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
            return Ok(infer_metadata(
                mod_root,
                repository_key,
                None,
                vec!["mod.json 不是普通文件或超过 1 MiB，已使用推断元数据。".to_owned()],
            ));
        }

        let contents = fs::read(&manifest_path)
            .map_err(|source| AppError::file_system(&manifest_path, source))?;
        let original = match serde_json::from_slice::<serde_json::Value>(&contents) {
            Ok(value) => value,
            Err(_) => {
                return Ok(infer_metadata(
                    mod_root,
                    repository_key,
                    None,
                    vec!["mod.json 不是有效 JSON，已使用推断元数据。".to_owned()],
                ));
            }
        };
        let manifest = match serde_json::from_value::<AuthorManifest>(original.clone()) {
            Ok(value) => value,
            Err(_) => {
                return Ok(infer_metadata(
                    mod_root,
                    repository_key,
                    Some(original),
                    vec!["mod.json 字段类型无效，已保留原文并使用推断元数据。".to_owned()],
                ));
            }
        };

        match manifest_to_metadata(mod_root, manifest, original) {
            Ok(read) => Ok(read),
            Err(message) => Ok(infer_metadata(
                mod_root,
                repository_key,
                Some(message.original),
                vec![message.message],
            )),
        }
    }
}

#[async_trait]
impl ModMetadataManager for FileSystemMetadataManager {
    async fn read_author_metadata(&self, mod_root: &Path) -> Result<AuthorModMetadata, AppError> {
        let root = mod_root.to_path_buf();
        let repository_key = root
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_owned());
        tokio::task::spawn_blocking(move || {
            Self::new()
                .read_with_warnings(&root, &repository_key)
                .map(|read| read.metadata)
        })
        .await?
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthorManifest {
    id: Option<String>,
    name: Option<String>,
    author: Option<String>,
    version: Option<String>,
    description: Option<String>,
    category: Option<String>,
    game_version: Option<String>,
    website: Option<String>,
    preview: Option<String>,
}

struct ManifestValidationError {
    message: String,
    original: serde_json::Value,
}

fn manifest_to_metadata(
    mod_root: &Path,
    manifest: AuthorManifest,
    original: serde_json::Value,
) -> Result<MetadataRead, ManifestValidationError> {
    let logical_id = required_field(manifest.id, "id", MAX_ID_LENGTH, &original)?;
    if !valid_logical_id(&logical_id) {
        return Err(ManifestValidationError {
            message: "mod.json 的 id 仅允许 ASCII 字母、数字、点、下划线和连字符，且不能以点开头或结尾；已使用推断元数据。".to_owned(),
            original,
        });
    }
    let name = required_field(manifest.name, "name", MAX_NAME_LENGTH, &original)?;
    let author = optional_field(manifest.author, MAX_SHORT_FIELD_LENGTH);
    let version = optional_field(manifest.version, MAX_SHORT_FIELD_LENGTH);
    let description = optional_field(manifest.description, MAX_DESCRIPTION_LENGTH);
    let category = optional_field(manifest.category, MAX_SHORT_FIELD_LENGTH);
    let game_version = optional_field(manifest.game_version, MAX_SHORT_FIELD_LENGTH);
    let website = optional_field(manifest.website, MAX_WEBSITE_LENGTH);
    let mut warnings = Vec::new();
    let preview_path =
        manifest
            .preview
            .and_then(|value| match validate_preview_path(mod_root, &value) {
                Ok(path) => Some(path),
                Err(message) => {
                    warnings.push(message);
                    None
                }
            });

    Ok(MetadataRead {
        metadata: AuthorModMetadata {
            logical_id,
            name,
            author,
            version,
            description,
            category,
            game_version,
            website,
            preview_path,
            original_document: Some(original),
            source_kind: MetadataSourceKind::ModJson,
        },
        warnings,
    })
}

fn infer_metadata(
    mod_root: &Path,
    repository_key: &str,
    original_document: Option<serde_json::Value>,
    mut warnings: Vec<String>,
) -> MetadataRead {
    let name = mod_root
        .file_name()
        .map(|value| value.to_string_lossy().trim().to_owned())
        .map(|value| strip_disabled_prefix(&value).to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "未命名模组".to_owned());
    let digest = blake3::hash(repository_key.to_lowercase().as_bytes());
    let logical_id = format!("local.{}", &digest.to_hex()[..20]);
    let preview_path = infer_preview(mod_root);
    if original_document.is_some() {
        warnings.push("作者原始 mod.json 未被修改。".to_owned());
    }

    MetadataRead {
        metadata: AuthorModMetadata {
            logical_id,
            name,
            author: None,
            version: None,
            description: None,
            category: None,
            game_version: None,
            website: None,
            preview_path,
            original_document,
            source_kind: MetadataSourceKind::Inferred,
        },
        warnings,
    }
}

fn strip_disabled_prefix(value: &str) -> &str {
    if value
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("DISABLED"))
    {
        value
            .get(8..)
            .unwrap_or_default()
            .trim_start_matches(['_', '-', ' '])
    } else {
        value
    }
}

fn required_field(
    value: Option<String>,
    field: &str,
    max_length: usize,
    original: &serde_json::Value,
) -> Result<String, ManifestValidationError> {
    let value = value.unwrap_or_default().trim().to_owned();
    if value.is_empty() || value.len() > max_length {
        return Err(ManifestValidationError {
            message: format!(
                "mod.json 的 {field} 不能为空且不能超过 {max_length} 字节；已使用推断元数据。"
            ),
            original: original.clone(),
        });
    }
    Ok(value)
}

fn optional_field(value: Option<String>, max_length: usize) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.chars().take(max_length).collect())
        }
    })
}

fn valid_logical_id(value: &str) -> bool {
    !value.starts_with('.')
        && !value.ends_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn validate_preview_path(mod_root: &Path, value: &str) -> Result<PathBuf, String> {
    let relative = PathBuf::from(value);
    validate_relative_path(&relative)
        .map_err(|_| "mod.json 的 preview 路径不安全，已忽略。".to_owned())?;
    let candidate = mod_root.join(&relative);
    if !candidate.exists() {
        return Err("mod.json 指定的 preview 文件不存在，已忽略。".to_owned());
    }
    if path_is_link_or_reparse_point(&candidate)
        .map_err(|_| "无法验证 preview 路径，已忽略。".to_owned())?
    {
        return Err("mod.json 指定的 preview 是链接或重解析点，已忽略。".to_owned());
    }
    let root = fs::canonicalize(mod_root)
        .map_err(|_| "无法验证模组根目录，已忽略 preview。".to_owned())?;
    let canonical =
        fs::canonicalize(&candidate).map_err(|_| "无法验证 preview 文件，已忽略。".to_owned())?;
    if !canonical.is_file() || canonical == root || !canonical.starts_with(&root) {
        return Err("mod.json 的 preview 路径超出模组目录，已忽略。".to_owned());
    }
    Ok(relative)
}

fn infer_preview(mod_root: &Path) -> Option<PathBuf> {
    for candidate in [
        "preview.png",
        "preview.jpg",
        "preview.jpeg",
        "thumbnail.png",
        "cover.png",
    ] {
        let path = mod_root.join(candidate);
        if path.is_file() && path_is_link_or_reparse_point(&path).is_ok_and(|is_link| !is_link) {
            return Some(PathBuf::from(candidate));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{core::mods::FileSystemMetadataManager, models::MetadataSourceKind};

    #[test]
    fn reads_author_manifest_without_rewriting_it() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let manifest = br#"{
          "id": "author.example",
          "name": "Example Mod",
          "author": "Author",
          "version": "1.0.0",
          "preview": "preview.png",
          "futureField": true
        }"#;
        fs::write(directory.path().join("mod.json"), manifest)?;
        fs::write(directory.path().join("preview.png"), b"preview")?;

        let read =
            FileSystemMetadataManager::new().read_with_warnings(directory.path(), "example")?;
        assert_eq!(read.metadata.logical_id, "author.example");
        assert_eq!(read.metadata.source_kind, MetadataSourceKind::ModJson);
        assert_eq!(fs::read(directory.path().join("mod.json"))?, manifest);
        Ok(())
    }

    #[test]
    fn infers_stable_local_metadata_for_missing_manifest() -> Result<(), Box<dyn std::error::Error>>
    {
        let parent = tempfile::tempdir()?;
        let root = parent.path().join("My Character Mod");
        fs::create_dir(&root)?;

        let first = FileSystemMetadataManager::new().read_with_warnings(&root, "my-mod")?;
        let second = FileSystemMetadataManager::new().read_with_warnings(&root, "my-mod")?;
        assert_eq!(first.metadata.logical_id, second.metadata.logical_id);
        assert_eq!(first.metadata.name, "My Character Mod");
        assert_eq!(first.metadata.source_kind, MetadataSourceKind::Inferred);
        Ok(())
    }

    #[test]
    fn ignores_traversing_preview_path() -> Result<(), Box<dyn std::error::Error>> {
        let parent = tempfile::tempdir()?;
        let root = parent.path().join("mod");
        fs::create_dir(&root)?;
        fs::write(parent.path().join("outside.png"), b"outside")?;
        fs::write(
            root.join("mod.json"),
            br#"{"id":"author.mod","name":"Mod","preview":"../outside.png"}"#,
        )?;

        let read = FileSystemMetadataManager::new().read_with_warnings(&root, "mod")?;
        assert!(read.metadata.preview_path.is_none());
        assert!(!read.warnings.is_empty());
        Ok(())
    }
}
