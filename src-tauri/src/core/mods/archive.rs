use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use sevenz_rust2::{Password, SevenZReader};
use uuid::Uuid;
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::{
    errors::AppError,
    models::{ModImportSourceKind, ModInstallProgress, ModInstallProgressStage},
    utils::validate_relative_path,
};

use super::repository::path_is_link_or_reparse_point;

const ZIP_LOCAL_SIGNATURE: &[u8] = b"PK\x03\x04";
const ZIP_EMPTY_SIGNATURE: &[u8] = b"PK\x05\x06";
const ZIP_SPANNED_SIGNATURE: &[u8] = b"PK\x07\x08";
const SEVEN_Z_SIGNATURE: &[u8] = b"7z\xbc\xaf\x27\x1c";
const RAR4_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x00";
const RAR5_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x01\x00";
const COPY_BUFFER_BYTES: usize = 128 * 1024;
const PROGRESS_BYTE_STEP: u64 = 4 * 1024 * 1024;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
const UNIX_FILE_TYPE_MASK: u32 = 0o170000;
const UNIX_SYMLINK_TYPE: u32 = 0o120000;

pub type InstallProgressReporter = Arc<dyn Fn(ModInstallProgress) + Send + Sync>;

#[derive(Debug, Clone, Copy)]
pub struct ExtractionPolicy {
    pub max_entries: u64,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
    pub max_compression_ratio: u64,
    pub max_path_length: usize,
    pub max_path_components: usize,
}

impl Default for ExtractionPolicy {
    fn default() -> Self {
        Self {
            max_entries: 20_000,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
            max_file_bytes: 512 * 1024 * 1024,
            max_compression_ratio: 1_000,
            max_path_length: 1_024,
            max_path_components: 64,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StagedSource {
    pub source_kind: ModImportSourceKind,
    pub source_name: String,
    pub staged_root: PathBuf,
    pub entry_count: u64,
    pub total_bytes: u64,
    pub warnings: Vec<String>,
}

pub fn stage_source(
    source: &Path,
    payload_root: &Path,
    operation_id: Uuid,
    policy: ExtractionPolicy,
    progress: &InstallProgressReporter,
) -> Result<StagedSource, AppError> {
    if !source.is_absolute() || source.as_os_str().is_empty() {
        return Err(AppError::ModInstall(
            "导入源必须是非空的绝对路径。".to_owned(),
        ));
    }
    if path_is_link_or_reparse_point(source)? {
        return Err(AppError::UnsafePath(
            "导入源不能是符号链接、目录联接或其他重解析点。".to_owned(),
        ));
    }
    let source = fs::canonicalize(source).map_err(|error| AppError::file_system(source, error))?;
    emit(
        progress,
        operation_id,
        ModInstallProgressStage::Inspecting,
        "正在识别导入源并检查安全限制…",
        0,
        None,
        0,
        None,
    );
    fs::create_dir(payload_root).map_err(|source| AppError::file_system(payload_root, source))?;

    if source.is_dir() {
        return stage_directory(&source, payload_root, operation_id, policy, progress);
    }
    let metadata = fs::metadata(&source).map_err(|error| AppError::file_system(&source, error))?;
    if !metadata.is_file() {
        return Err(AppError::ModInstall(
            "导入源既不是普通文件也不是目录。".to_owned(),
        ));
    }

    let source_kind = detect_archive_kind(&source)?;
    match source_kind {
        ModImportSourceKind::Zip => {
            extract_zip(&source, payload_root, operation_id, policy, progress)
        }
        ModImportSourceKind::SevenZip => {
            extract_seven_zip(&source, payload_root, operation_id, policy, progress)
        }
        ModImportSourceKind::Rar => {
            extract_rar(&source, payload_root, operation_id, policy, progress)
        }
        ModImportSourceKind::Directory => Err(AppError::DataIntegrity(
            "文件导入源被错误识别为目录。".to_owned(),
        )),
    }
}

fn detect_archive_kind(path: &Path) -> Result<ModImportSourceKind, AppError> {
    let mut file = File::open(path).map_err(|source| AppError::file_system(path, source))?;
    let mut signature = [0_u8; 8];
    let read = file
        .read(&mut signature)
        .map_err(|source| AppError::file_system(path, source))?;
    let signature = &signature[..read];
    if signature.starts_with(ZIP_LOCAL_SIGNATURE)
        || signature.starts_with(ZIP_EMPTY_SIGNATURE)
        || signature.starts_with(ZIP_SPANNED_SIGNATURE)
    {
        Ok(ModImportSourceKind::Zip)
    } else if signature.starts_with(SEVEN_Z_SIGNATURE) {
        Ok(ModImportSourceKind::SevenZip)
    } else if signature.starts_with(RAR4_SIGNATURE) || signature.starts_with(RAR5_SIGNATURE) {
        Ok(ModImportSourceKind::Rar)
    } else {
        Err(AppError::Archive(
            "无法识别压缩包格式；仅支持 ZIP、7z、RAR 或文件夹。".to_owned(),
        ))
    }
}

fn stage_directory(
    source: &Path,
    payload_root: &Path,
    operation_id: Uuid,
    policy: ExtractionPolicy,
    progress: &InstallProgressReporter,
) -> Result<StagedSource, AppError> {
    let source_name = source_display_name(source);
    let staged_name = safe_source_directory_name(&source_name);
    let staged_root = payload_root.join(staged_name);
    fs::create_dir(&staged_root).map_err(|source| AppError::file_system(&staged_root, source))?;

    let mut entries = Vec::new();
    let mut paths = ValidatedPaths::default();
    let mut total_bytes = 0_u64;
    for item in WalkDir::new(source).follow_links(false).sort_by_file_name() {
        let item =
            item.map_err(|error| AppError::ModInstall(format!("无法遍历导入文件夹：{error}")))?;
        if item.depth() == 0 {
            continue;
        }
        if path_is_link_or_reparse_point(item.path())? {
            return Err(AppError::UnsafePath(format!(
                "导入文件夹包含链接或重解析点：{}。",
                item.path().display()
            )));
        }
        let relative = item
            .path()
            .strip_prefix(source)
            .map_err(|_| AppError::UnsafePath("导入文件无法转换为安全相对路径。".to_owned()))?;
        let relative = validate_archive_path(relative.to_string_lossy().as_ref(), policy)?;
        let metadata = item
            .metadata()
            .map_err(|source| AppError::file_system(item.path(), source.into()))?;
        let is_directory = metadata.is_dir();
        if !is_directory && !metadata.is_file() {
            return Err(AppError::UnsafePath(format!(
                "导入文件夹包含非普通文件：{}。",
                item.path().display()
            )));
        }
        let size = if is_directory { 0 } else { metadata.len() };
        check_entry_quota(&mut total_bytes, size, &entries, policy)?;
        paths.insert(&relative, is_directory)?;
        entries.push(SourceEntry {
            source: item.path().to_path_buf(),
            relative,
            is_directory,
            size,
        });
    }
    if entries.iter().all(|entry| entry.is_directory) {
        return Err(AppError::ModInstall(
            "导入文件夹不包含任何文件。".to_owned(),
        ));
    }

    emit(
        progress,
        operation_id,
        ModInstallProgressStage::Extracting,
        "正在安全复制模组文件…",
        0,
        Some(u64::try_from(entries.len()).unwrap_or(u64::MAX)),
        0,
        Some(total_bytes),
    );
    let mut processed_items = 0_u64;
    let mut processed_bytes = 0_u64;
    for entry in &entries {
        let target = staged_root.join(&entry.relative);
        if entry.is_directory {
            create_directory_new_or_existing(&target)?;
        } else {
            let mut input = File::open(&entry.source)
                .map_err(|source| AppError::file_system(&entry.source, source))?;
            let mut output = create_file_new(&target)?;
            copy_exact_bounded(&mut input, &mut output, entry.size, &target)?;
            processed_bytes = processed_bytes
                .checked_add(entry.size)
                .ok_or_else(|| AppError::DataIntegrity("复制进度超过支持范围。".to_owned()))?;
        }
        processed_items += 1;
        emit(
            progress,
            operation_id,
            ModInstallProgressStage::Extracting,
            "正在安全复制模组文件…",
            processed_items,
            Some(u64::try_from(entries.len()).unwrap_or(u64::MAX)),
            processed_bytes,
            Some(total_bytes),
        );
    }
    Ok(StagedSource {
        source_kind: ModImportSourceKind::Directory,
        source_name,
        staged_root,
        entry_count: u64::try_from(entries.len()).unwrap_or(u64::MAX),
        total_bytes,
        warnings: Vec::new(),
    })
}

fn extract_zip(
    source: &Path,
    payload_root: &Path,
    operation_id: Uuid,
    policy: ExtractionPolicy,
    progress: &InstallProgressReporter,
) -> Result<StagedSource, AppError> {
    let source_name = source_display_name(source);
    let mut archive =
        ZipArchive::new(File::open(source).map_err(|error| AppError::file_system(source, error))?)
            .map_err(|error| AppError::Archive(format!("无法读取 ZIP 压缩包：{error}")))?;
    if archive
        .has_overlapping_files()
        .map_err(|error| AppError::Archive(format!("无法检查 ZIP 重叠条目：{error}")))?
    {
        return Err(AppError::Archive(
            "ZIP 包包含重叠文件数据，已按恶意或损坏压缩包拒绝。".to_owned(),
        ));
    }

    let mut paths = ValidatedPaths::default();
    let mut total_bytes = 0_u64;
    let mut total_compressed = 0_u64;
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| AppError::Archive(format!("无法读取 ZIP 条目：{error}")))?;
        if entry.encrypted() {
            return Err(AppError::Archive("暂不支持加密 ZIP 模组包。".to_owned()));
        }
        if entry.is_symlink() {
            return Err(AppError::UnsafePath(format!(
                "ZIP 条目 {} 是符号链接，已拒绝。",
                entry.name()
            )));
        }
        if !entry.is_file() && !entry.is_dir() {
            return Err(AppError::Archive(format!(
                "ZIP 条目 {} 不是普通文件或目录。",
                entry.name()
            )));
        }
        let relative = validate_archive_path(entry.name(), policy)?;
        let is_directory = entry.is_dir();
        let size = if is_directory { 0 } else { entry.size() };
        check_entry_quota(&mut total_bytes, size, &entries, policy)?;
        total_compressed = total_compressed
            .checked_add(entry.compressed_size())
            .ok_or_else(|| AppError::Archive("ZIP 压缩大小超过支持范围。".to_owned()))?;
        paths.insert(&relative, is_directory)?;
        entries.push(ArchiveEntry {
            relative,
            is_directory,
            size,
        });
    }
    validate_archive_totals(&entries, total_bytes, total_compressed, policy)?;
    extract_zip_entries(
        &mut archive,
        payload_root,
        operation_id,
        &entries,
        total_bytes,
        progress,
    )?;
    Ok(StagedSource {
        source_kind: ModImportSourceKind::Zip,
        source_name: source_name.clone(),
        staged_root: payload_root.to_path_buf(),
        entry_count: u64::try_from(entries.len()).unwrap_or(u64::MAX),
        total_bytes,
        warnings: extension_warning(source, "zip", &source_name),
    })
}

fn extract_zip_entries(
    archive: &mut ZipArchive<File>,
    payload_root: &Path,
    operation_id: Uuid,
    entries: &[ArchiveEntry],
    total_bytes: u64,
    progress: &InstallProgressReporter,
) -> Result<(), AppError> {
    emit_extract_start(progress, operation_id, entries, total_bytes);
    let mut processed_bytes = 0_u64;
    for (index, expected) in entries.iter().enumerate() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| AppError::Archive(format!("无法解压 ZIP 条目：{error}")))?;
        let target = payload_root.join(&expected.relative);
        if expected.is_directory {
            create_directory_new_or_existing(&target)?;
        } else {
            let mut output = create_file_new(&target)?;
            copy_exact_bounded(&mut entry, &mut output, expected.size, &target)?;
            processed_bytes = processed_bytes
                .checked_add(expected.size)
                .ok_or_else(|| AppError::DataIntegrity("ZIP 解压进度超过支持范围。".to_owned()))?;
        }
        emit_extract_progress(
            progress,
            operation_id,
            index,
            entries.len(),
            processed_bytes,
            total_bytes,
        );
    }
    Ok(())
}

fn extract_seven_zip(
    source: &Path,
    payload_root: &Path,
    operation_id: Uuid,
    policy: ExtractionPolicy,
    progress: &InstallProgressReporter,
) -> Result<StagedSource, AppError> {
    let source_name = source_display_name(source);
    let reader = SevenZReader::open(source, Password::empty())
        .map_err(|error| AppError::Archive(format!("无法读取 7z 压缩包：{error}")))?;
    let archive = reader.archive();
    let mut paths = ValidatedPaths::default();
    let mut total_bytes = 0_u64;
    let mut entries = Vec::with_capacity(archive.files.len());
    for entry in &archive.files {
        if entry.is_anti_item {
            return Err(AppError::Archive(format!(
                "7z 条目 {} 是删除标记条目，已拒绝。",
                entry.name
            )));
        }
        if archive_attributes_are_link(entry.windows_attributes) {
            return Err(AppError::UnsafePath(format!(
                "7z 条目 {} 是链接或重解析点，已拒绝。",
                entry.name
            )));
        }
        let relative = validate_archive_path(&entry.name, policy)?;
        let size = if entry.is_directory { 0 } else { entry.size };
        check_entry_quota(&mut total_bytes, size, &entries, policy)?;
        paths.insert(&relative, entry.is_directory)?;
        entries.push(ArchiveEntry {
            relative,
            is_directory: entry.is_directory,
            size,
        });
    }
    let total_compressed = archive.pack_sizes.iter().try_fold(0_u64, |sum, value| {
        sum.checked_add(*value)
            .ok_or_else(|| AppError::Archive("7z 压缩大小超过支持范围。".to_owned()))
    })?;
    validate_archive_totals(&entries, total_bytes, total_compressed, policy)?;
    drop(reader);
    emit_extract_start(progress, operation_id, &entries, total_bytes);

    let mut callback_error = None;
    let mut processed_items = 0_u64;
    let mut processed_bytes = 0_u64;
    let result = sevenz_rust2::decompress_file_with_extract_fn(
        source,
        payload_root,
        |entry, input, _untrusted_destination| {
            let extracted = (|| -> Result<(), AppError> {
                let relative = validate_archive_path(&entry.name, policy)?;
                let expected = entries
                    .get(usize::try_from(processed_items).unwrap_or(usize::MAX))
                    .ok_or_else(|| AppError::Archive("7z 实际条目数超过预检清单。".to_owned()))?;
                if entry.is_anti_item
                    || archive_attributes_are_link(entry.windows_attributes)
                    || relative != expected.relative
                    || entry.is_directory != expected.is_directory
                    || (!entry.is_directory && entry.size != expected.size)
                {
                    return Err(AppError::Archive(
                        "7z 条目在预检与解压阶段发生变化。".to_owned(),
                    ));
                }
                let target = payload_root.join(relative);
                if entry.is_directory {
                    create_directory_new_or_existing(&target)?;
                } else {
                    let mut output = create_file_new(&target)?;
                    copy_exact_bounded(input, &mut output, entry.size, &target)?;
                    processed_bytes = processed_bytes.checked_add(entry.size).ok_or_else(|| {
                        AppError::DataIntegrity("7z 解压进度超过支持范围。".to_owned())
                    })?;
                }
                processed_items += 1;
                emit(
                    progress,
                    operation_id,
                    ModInstallProgressStage::Extracting,
                    "正在安全解压模组文件…",
                    processed_items,
                    Some(u64::try_from(entries.len()).unwrap_or(u64::MAX)),
                    processed_bytes,
                    Some(total_bytes),
                );
                Ok(())
            })();
            match extracted {
                Ok(()) => Ok(true),
                Err(error) => {
                    let message = error.to_string();
                    callback_error = Some(error);
                    Err(sevenz_rust2::Error::other(message))
                }
            }
        },
    );
    if let Some(error) = callback_error {
        return Err(error);
    }
    result.map_err(|error| AppError::Archive(format!("无法解压 7z 压缩包：{error}")))?;
    if processed_items != u64::try_from(entries.len()).unwrap_or(u64::MAX) {
        return Err(AppError::Archive(
            "7z 实际解压条目数与清单不一致。".to_owned(),
        ));
    }
    Ok(StagedSource {
        source_kind: ModImportSourceKind::SevenZip,
        source_name: source_name.clone(),
        staged_root: payload_root.to_path_buf(),
        entry_count: processed_items,
        total_bytes,
        warnings: extension_warning(source, "7z", &source_name),
    })
}

fn extract_rar(
    source: &Path,
    payload_root: &Path,
    operation_id: Uuid,
    policy: ExtractionPolicy,
    progress: &InstallProgressReporter,
) -> Result<StagedSource, AppError> {
    let source_name = source_display_name(source);
    if unrar::Archive::new(source).is_multipart() {
        return Err(AppError::Archive(
            "暂不支持多卷 RAR 模组包，请先合并或重新打包为单个压缩包。".to_owned(),
        ));
    }
    let listing = unrar::Archive::new(source)
        .open_for_listing()
        .map_err(|error| AppError::Archive(format!("无法读取 RAR 压缩包：{error}")))?;
    let mut entries = Vec::new();
    let mut paths = ValidatedPaths::default();
    let mut total_bytes = 0_u64;
    for item in listing {
        let entry =
            item.map_err(|error| AppError::Archive(format!("无法读取 RAR 条目：{error}")))?;
        if entry.is_encrypted() {
            return Err(AppError::Archive("暂不支持加密 RAR 模组包。".to_owned()));
        }
        if entry.is_split() {
            return Err(AppError::Archive(
                "暂不支持跨卷拆分的 RAR 条目。".to_owned(),
            ));
        }
        if archive_attributes_are_link(entry.file_attr) {
            return Err(AppError::UnsafePath(format!(
                "RAR 条目 {} 是链接或重解析点，已拒绝。",
                entry.filename.display()
            )));
        }
        let relative = validate_archive_path(entry.filename.to_string_lossy().as_ref(), policy)?;
        let is_directory = entry.is_directory();
        let size = if is_directory { 0 } else { entry.unpacked_size };
        check_entry_quota(&mut total_bytes, size, &entries, policy)?;
        paths.insert(&relative, is_directory)?;
        entries.push(ArchiveEntry {
            relative,
            is_directory,
            size,
        });
    }
    let compressed_size = fs::metadata(source)
        .map_err(|error| AppError::file_system(source, error))?
        .len();
    validate_archive_totals(&entries, total_bytes, compressed_size, policy)?;
    emit_extract_start(progress, operation_id, &entries, total_bytes);

    let mut archive = unrar::Archive::new(source)
        .open_for_processing()
        .map_err(|error| AppError::Archive(format!("无法打开 RAR 压缩包：{error}")))?;
    let mut processed_items = 0_usize;
    let mut processed_bytes = 0_u64;
    loop {
        let Some(current) = archive
            .read_header()
            .map_err(|error| AppError::Archive(format!("无法读取 RAR 文件头：{error}")))?
        else {
            break;
        };
        let expected = entries
            .get(processed_items)
            .ok_or_else(|| AppError::Archive("RAR 实际条目数超过预检清单。".to_owned()))?;
        if current.entry().is_encrypted()
            || current.entry().is_split()
            || archive_attributes_are_link(current.entry().file_attr)
            || current.entry().is_directory() != expected.is_directory
            || (!expected.is_directory && current.entry().unpacked_size != expected.size)
        {
            return Err(AppError::Archive(
                "RAR 条目在预检与解压阶段发生变化或包含不支持的属性。".to_owned(),
            ));
        }
        let relative =
            validate_archive_path(current.entry().filename.to_string_lossy().as_ref(), policy)?;
        if relative != expected.relative {
            return Err(AppError::Archive(
                "RAR 条目在预检与解压阶段发生变化。".to_owned(),
            ));
        }
        let target = payload_root.join(&relative);
        if expected.is_directory {
            create_directory_new_or_existing(&target)?;
            archive = current
                .skip()
                .map_err(|error| AppError::Archive(format!("无法跳过 RAR 目录条目：{error}")))?;
        } else {
            let (contents, next) = current
                .read()
                .map_err(|error| AppError::Archive(format!("无法读取 RAR 文件内容：{error}")))?;
            if u64::try_from(contents.len()).unwrap_or(u64::MAX) != expected.size {
                return Err(AppError::Archive(format!(
                    "RAR 条目 {} 的实际大小与清单不一致。",
                    expected.relative.display()
                )));
            }
            let mut output = create_file_new(&target)?;
            output
                .write_all(&contents)
                .map_err(|source| AppError::file_system(&target, source))?;
            output
                .sync_all()
                .map_err(|source| AppError::file_system(&target, source))?;
            processed_bytes = processed_bytes
                .checked_add(expected.size)
                .ok_or_else(|| AppError::DataIntegrity("RAR 解压进度超过支持范围。".to_owned()))?;
            archive = next;
        }
        processed_items += 1;
        emit_extract_progress(
            progress,
            operation_id,
            processed_items - 1,
            entries.len(),
            processed_bytes,
            total_bytes,
        );
    }
    if processed_items != entries.len() {
        return Err(AppError::Archive(
            "RAR 实际解压条目数与预检清单不一致。".to_owned(),
        ));
    }
    Ok(StagedSource {
        source_kind: ModImportSourceKind::Rar,
        source_name: source_name.clone(),
        staged_root: payload_root.to_path_buf(),
        entry_count: u64::try_from(entries.len()).unwrap_or(u64::MAX),
        total_bytes,
        warnings: extension_warning(source, "rar", &source_name),
    })
}

#[derive(Debug)]
struct SourceEntry {
    source: PathBuf,
    relative: PathBuf,
    is_directory: bool,
    size: u64,
}

#[derive(Debug)]
struct ArchiveEntry {
    relative: PathBuf,
    is_directory: bool,
    size: u64,
}

trait EntryLike {
    fn size(&self) -> u64;
}

impl EntryLike for SourceEntry {
    fn size(&self) -> u64 {
        self.size
    }
}

impl EntryLike for ArchiveEntry {
    fn size(&self) -> u64 {
        self.size
    }
}

#[derive(Default)]
struct ValidatedPaths {
    entries: HashMap<String, bool>,
}

impl ValidatedPaths {
    fn insert(&mut self, relative: &Path, is_directory: bool) -> Result<(), AppError> {
        let key = path_key(relative);
        if self.entries.contains_key(&key) {
            return Err(AppError::UnsafePath(format!(
                "压缩包包含大小写不敏感的重复路径：{}。",
                relative.display()
            )));
        }
        let mut ancestor = relative.parent();
        while let Some(path) = ancestor {
            if path.as_os_str().is_empty() {
                break;
            }
            if self
                .entries
                .get(&path_key(path))
                .is_some_and(|value| !value)
            {
                return Err(AppError::UnsafePath(format!(
                    "路径 {} 的父路径被另一个文件占用。",
                    relative.display()
                )));
            }
            ancestor = path.parent();
        }
        if !is_directory {
            let prefix = format!("{key}/");
            if self
                .entries
                .keys()
                .any(|existing| existing.starts_with(&prefix))
            {
                return Err(AppError::UnsafePath(format!(
                    "文件路径 {} 与已有子条目冲突。",
                    relative.display()
                )));
            }
        }
        self.entries.insert(key, is_directory);
        Ok(())
    }
}

fn validate_archive_path(raw: &str, policy: ExtractionPolicy) -> Result<PathBuf, AppError> {
    let normalized = raw.replace('\\', "/");
    let normalized = normalized.trim_end_matches('/');
    if normalized.is_empty()
        || normalized.len() > policy.max_path_length
        || normalized.starts_with('/')
        || normalized.starts_with("//")
    {
        return Err(AppError::UnsafePath(format!(
            "压缩包条目路径无效或过长：{raw}。"
        )));
    }
    let path = PathBuf::from(normalized);
    validate_relative_path(&path)?;
    let components = path.components().collect::<Vec<_>>();
    if components.len() > policy.max_path_components
        || components
            .iter()
            .any(|component| matches!(component, Component::CurDir))
    {
        return Err(AppError::UnsafePath(format!(
            "压缩包条目路径层级过深或包含当前目录组件：{raw}。"
        )));
    }
    if components.iter().any(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .count()
            > 255
    }) {
        return Err(AppError::UnsafePath(format!(
            "压缩包条目包含过长的文件名片段：{raw}。"
        )));
    }
    Ok(path)
}

fn check_entry_quota<T: EntryLike>(
    total_bytes: &mut u64,
    size: u64,
    entries: &[T],
    policy: ExtractionPolicy,
) -> Result<(), AppError> {
    let next_count = u64::try_from(entries.len())
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    if next_count > policy.max_entries {
        return Err(AppError::Archive(format!(
            "压缩包条目数超过安全上限 {}。",
            policy.max_entries
        )));
    }
    if size > policy.max_file_bytes {
        return Err(AppError::Archive(format!(
            "单个文件大小超过安全上限 {} 字节。",
            policy.max_file_bytes
        )));
    }
    *total_bytes = total_bytes
        .checked_add(size)
        .ok_or_else(|| AppError::Archive("解压后总大小超过支持范围。".to_owned()))?;
    if *total_bytes > policy.max_total_bytes {
        return Err(AppError::Archive(format!(
            "解压后总大小超过安全上限 {} 字节。",
            policy.max_total_bytes
        )));
    }
    Ok(())
}

fn validate_archive_totals<T: EntryLike>(
    entries: &[T],
    total_bytes: u64,
    total_compressed: u64,
    policy: ExtractionPolicy,
) -> Result<(), AppError> {
    if entries.is_empty() || entries.iter().all(|entry| entry.size() == 0) {
        return Err(AppError::Archive("压缩包不包含任何模组文件。".to_owned()));
    }
    if total_bytes > 0
        && (total_compressed == 0
            || total_bytes / total_compressed.max(1) > policy.max_compression_ratio)
    {
        return Err(AppError::Archive(format!(
            "压缩包展开比例超过安全上限 {}:1。",
            policy.max_compression_ratio
        )));
    }
    Ok(())
}

fn copy_exact_bounded(
    input: &mut dyn Read,
    output: &mut File,
    expected_size: u64,
    target: &Path,
) -> Result<(), AppError> {
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    let mut written = 0_u64;
    loop {
        let remaining = expected_size.saturating_sub(written);
        if remaining == 0 {
            let mut extra = [0_u8; 1];
            let extra_read = input
                .read(&mut extra)
                .map_err(|source| AppError::file_system(target, source))?;
            if extra_read != 0 {
                return Err(AppError::Archive(format!(
                    "条目 {} 的实际内容超过声明大小。",
                    target.display()
                )));
            }
            break;
        }
        let limit = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let read = input
            .read(&mut buffer[..limit])
            .map_err(|source| AppError::file_system(target, source))?;
        if read == 0 {
            return Err(AppError::Archive(format!(
                "条目 {} 的实际内容短于声明大小。",
                target.display()
            )));
        }
        output
            .write_all(&buffer[..read])
            .map_err(|source| AppError::file_system(target, source))?;
        written = written
            .checked_add(u64::try_from(read).unwrap_or(u64::MAX))
            .ok_or_else(|| AppError::Archive("解压文件大小超过支持范围。".to_owned()))?;
    }
    output
        .sync_all()
        .map_err(|source| AppError::file_system(target, source))
}

fn create_directory_new_or_existing(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        let metadata =
            fs::symlink_metadata(path).map_err(|source| AppError::file_system(path, source))?;
        if !metadata.is_dir() || path_is_link_or_reparse_point(path)? {
            return Err(AppError::UnsafePath(format!(
                "目录路径 {} 已被不安全条目占用。",
                path.display()
            )));
        }
        return Ok(());
    }
    fs::create_dir_all(path).map_err(|source| AppError::file_system(path, source))
}

fn create_file_new(path: &Path) -> Result<File, AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::UnsafePath(format!("文件路径 {} 没有父目录。", path.display())))?;
    create_directory_new_or_existing(parent)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| AppError::file_system(path, source))
}

fn archive_attributes_are_link(attributes: u32) -> bool {
    attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || ((attributes >> 16) & UNIX_FILE_TYPE_MASK) == UNIX_SYMLINK_TYPE
}

fn source_display_name(source: &Path) -> String {
    source
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Imported Mod".to_owned())
}

fn safe_source_directory_name(value: &str) -> String {
    let mut result = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    result = result.trim().trim_end_matches(['.', ' ']).to_owned();
    if result.is_empty() {
        result = "Imported Mod".to_owned();
    }
    if result.encode_utf16().count() > 120 {
        result = result.chars().take(100).collect();
    }
    if validate_relative_path(Path::new(&result)).is_err() {
        result = format!(
            "Imported Mod-{}",
            &blake3::hash(value.as_bytes()).to_hex()[..8]
        );
    }
    result
}

fn extension_warning(source: &Path, expected: &str, source_name: &str) -> Vec<String> {
    let matches = source
        .extension()
        .is_some_and(|value| value.to_string_lossy().eq_ignore_ascii_case(expected));
    if matches {
        Vec::new()
    } else {
        vec![format!(
            "文件 {source_name} 的扩展名与检测到的 {expected} 格式不一致；AEMM 已按文件签名处理。"
        )]
    }
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn emit_extract_start(
    progress: &InstallProgressReporter,
    operation_id: Uuid,
    entries: &[ArchiveEntry],
    total_bytes: u64,
) {
    emit(
        progress,
        operation_id,
        ModInstallProgressStage::Extracting,
        "正在安全解压模组文件…",
        0,
        Some(u64::try_from(entries.len()).unwrap_or(u64::MAX)),
        0,
        Some(total_bytes),
    );
}

fn emit_extract_progress(
    progress: &InstallProgressReporter,
    operation_id: Uuid,
    index: usize,
    total_items: usize,
    processed_bytes: u64,
    total_bytes: u64,
) {
    let processed_items = u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1);
    let should_emit = processed_items == u64::try_from(total_items).unwrap_or(u64::MAX)
        || processed_bytes == total_bytes
        || processed_bytes % PROGRESS_BYTE_STEP < COPY_BUFFER_BYTES as u64;
    if should_emit {
        emit(
            progress,
            operation_id,
            ModInstallProgressStage::Extracting,
            "正在安全解压模组文件…",
            processed_items,
            Some(u64::try_from(total_items).unwrap_or(u64::MAX)),
            processed_bytes,
            Some(total_bytes),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn emit(
    progress: &InstallProgressReporter,
    operation_id: Uuid,
    stage: ModInstallProgressStage,
    message: impl Into<String>,
    processed_items: u64,
    total_items: Option<u64>,
    processed_bytes: u64,
    total_bytes: Option<u64>,
) {
    progress(ModInstallProgress {
        operation_id,
        stage,
        message: message.into(),
        processed_items,
        total_items,
        processed_bytes,
        total_bytes,
    });
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use sevenz_rust2::{SevenZArchiveEntry, SevenZWriter};
    use uuid::Uuid;
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::{ExtractionPolicy, InstallProgressReporter, stage_source};

    fn no_progress() -> InstallProgressReporter {
        std::sync::Arc::new(|_| {})
    }

    #[test]
    fn extracts_safe_zip_without_overwrite() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let archive_path = temp.path().join("example.zip");
        let file = fs::File::create(&archive_path)?;
        let mut writer = ZipWriter::new(file);
        writer.start_file("Example/mod.json", SimpleFileOptions::default())?;
        writer.write_all(br#"{"id":"author.example","name":"Example"}"#)?;
        writer.start_file("Example/content.ini", SimpleFileOptions::default())?;
        writer.write_all(b"content")?;
        writer.finish()?;
        let payload = temp.path().join("payload");

        let staged = stage_source(
            &archive_path,
            &payload,
            Uuid::new_v4(),
            ExtractionPolicy::default(),
            &no_progress(),
        )?;
        assert_eq!(staged.entry_count, 2);
        assert!(payload.join("Example/content.ini").is_file());
        Ok(())
    }

    #[test]
    fn rejects_zip_slip_before_writing_outside_payload() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let archive_path = temp.path().join("traversal.zip");
        let file = fs::File::create(&archive_path)?;
        let mut writer = ZipWriter::new(file);
        writer.start_file(
            "../../Windows/System32/evil.ini",
            SimpleFileOptions::default(),
        )?;
        writer.write_all(b"evil")?;
        writer.finish()?;
        let payload = temp.path().join("payload");

        assert!(
            stage_source(
                &archive_path,
                &payload,
                Uuid::new_v4(),
                ExtractionPolicy::default(),
                &no_progress(),
            )
            .is_err()
        );
        assert!(!temp.path().join("Windows/System32/evil.ini").exists());
        Ok(())
    }

    #[test]
    fn rejects_case_insensitive_archive_collisions() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let archive_path = temp.path().join("collision.zip");
        let file = fs::File::create(&archive_path)?;
        let mut writer = ZipWriter::new(file);
        writer.start_file("Mod/File.ini", SimpleFileOptions::default())?;
        writer.write_all(b"one")?;
        writer.start_file("mod/file.INI", SimpleFileOptions::default())?;
        writer.write_all(b"two")?;
        writer.finish()?;

        assert!(
            stage_source(
                &archive_path,
                &temp.path().join("payload"),
                Uuid::new_v4(),
                ExtractionPolicy::default(),
                &no_progress(),
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn rejects_zip_symlink_entries() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let archive_path = temp.path().join("symlink.zip");
        let file = fs::File::create(&archive_path)?;
        let mut writer = ZipWriter::new(file);
        writer.add_symlink(
            "Example/link.ini",
            "../../outside.ini",
            SimpleFileOptions::default(),
        )?;
        writer.finish()?;

        assert!(
            stage_source(
                &archive_path,
                &temp.path().join("payload"),
                Uuid::new_v4(),
                ExtractionPolicy::default(),
                &no_progress(),
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn extracts_safe_seven_zip_through_validated_callback() -> Result<(), Box<dyn std::error::Error>>
    {
        let temp = tempfile::tempdir()?;
        let archive_path = temp.path().join("example.7z");
        let mut writer = SevenZWriter::new(fs::File::create(&archive_path)?)?;
        writer.push_archive_entry(
            SevenZArchiveEntry::new_file("Example/mod.json"),
            Some(&br#"{"id":"author.example","name":"Example"}"#[..]),
        )?;
        writer.push_archive_entry(
            SevenZArchiveEntry::new_file("Example/content.ini"),
            Some(&b"content"[..]),
        )?;
        writer.finish()?;
        let payload = temp.path().join("payload");

        let staged = stage_source(
            &archive_path,
            &payload,
            Uuid::new_v4(),
            ExtractionPolicy::default(),
            &no_progress(),
        )?;
        assert_eq!(staged.entry_count, 2);
        assert_eq!(fs::read(payload.join("Example/content.ini"))?, b"content");
        Ok(())
    }

    #[test]
    fn rejects_seven_zip_parent_traversal() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let archive_path = temp.path().join("traversal.7z");
        let mut writer = SevenZWriter::new(fs::File::create(&archive_path)?)?;
        writer.push_archive_entry(
            SevenZArchiveEntry::new_file("../../outside.ini"),
            Some(&b"evil"[..]),
        )?;
        writer.finish()?;

        assert!(
            stage_source(
                &archive_path,
                &temp.path().join("payload"),
                Uuid::new_v4(),
                ExtractionPolicy::default(),
                &no_progress(),
            )
            .is_err()
        );
        assert!(!temp.path().join("outside.ini").exists());
        Ok(())
    }

    #[test]
    fn extracts_single_file_rar_without_using_native_destination_paths()
    -> Result<(), Box<dyn std::error::Error>> {
        // Derived from unrar 0.5.8's MIT/Apache-2.0 `data/version.rar` test vector.
        const VERSION_RAR: &str = "UmFyIRoHAM+QcwAADQAAAAAAAAAPDHQggCcAFQAAAAsAAAADRfN9xqSKB0cdMwcApIEAAFZFUlNJT04MAI/sikXMI8hICINi/l/dXFOI8HLEPXsAQAcA";
        let temp = tempfile::tempdir()?;
        let archive_path = temp.path().join("version.rar");
        fs::write(&archive_path, STANDARD.decode(VERSION_RAR)?)?;
        let payload = temp.path().join("payload");

        let staged = stage_source(
            &archive_path,
            &payload,
            Uuid::new_v4(),
            ExtractionPolicy::default(),
            &no_progress(),
        )?;
        assert_eq!(staged.entry_count, 1);
        assert!(payload.join("VERSION").is_file());
        Ok(())
    }

    #[test]
    fn enforces_unpacked_size_quota() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let archive_path = temp.path().join("large.zip");
        let file = fs::File::create(&archive_path)?;
        let mut writer = ZipWriter::new(file);
        writer.start_file("large.bin", SimpleFileOptions::default())?;
        writer.write_all(&[0_u8; 64])?;
        writer.finish()?;
        let policy = ExtractionPolicy {
            max_total_bytes: 32,
            max_file_bytes: 32,
            ..ExtractionPolicy::default()
        };
        assert!(
            stage_source(
                &archive_path,
                &temp.path().join("payload"),
                Uuid::new_v4(),
                policy,
                &no_progress(),
            )
            .is_err()
        );
        Ok(())
    }
}
