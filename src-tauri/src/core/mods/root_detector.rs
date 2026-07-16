use std::{
    fs,
    path::{Path, PathBuf},
};

use walkdir::WalkDir;

use crate::errors::AppError;

use super::repository::path_is_link_or_reparse_point;

const MAX_MANIFEST_SEARCH_DEPTH: usize = 5;
const MAX_WRAPPER_DEPTH: usize = 4;

#[derive(Debug)]
pub struct DetectedModRoot {
    pub path: PathBuf,
    pub warnings: Vec<String>,
}

pub fn detect_mod_root(staged_root: &Path) -> Result<DetectedModRoot, AppError> {
    if !staged_root.is_absolute() || path_is_link_or_reparse_point(staged_root)? {
        return Err(AppError::UnsafePath(
            "待分析的模组根目录必须是非链接的绝对目录。".to_owned(),
        ));
    }
    let staged_root = fs::canonicalize(staged_root)
        .map_err(|source| AppError::file_system(staged_root, source))?;
    if !staged_root.is_dir() {
        return Err(AppError::ModInstall(
            "解压后的模组内容不是目录。".to_owned(),
        ));
    }

    let manifest_roots = find_manifest_roots(&staged_root)?;
    if manifest_roots.len() > 1 {
        return Err(AppError::ModInstall(format!(
            "压缩包包含 {} 个独立 mod.json，无法安全判断唯一模组根目录；请分别导入。",
            manifest_roots.len()
        )));
    }
    if let Some(root) = manifest_roots.into_iter().next() {
        let warnings = (root != staged_root)
            .then(|| "已根据唯一的 mod.json 自动定位真正的模组根目录。".to_owned())
            .into_iter()
            .collect();
        ensure_contains_file(&root)?;
        return Ok(DetectedModRoot {
            path: root,
            warnings,
        });
    }

    let mut current = staged_root.clone();
    let mut unwrapped = 0_usize;
    for _ in 0..MAX_WRAPPER_DEPTH {
        let entries = meaningful_direct_entries(&current)?;
        let files = entries
            .iter()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .count();
        let directories = entries
            .iter()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .collect::<Vec<_>>();
        if files == 0 && directories.len() == 1 {
            let child = directories[0].path();
            if path_is_link_or_reparse_point(&child)? {
                return Err(AppError::UnsafePath(
                    "模组包装目录不能是链接或重解析点。".to_owned(),
                ));
            }
            current = child;
            unwrapped += 1;
        } else {
            break;
        }
    }
    ensure_contains_file(&current)?;

    let mut warnings = Vec::new();
    if unwrapped > 0 {
        warnings.push(format!("已自动移除 {unwrapped} 层仅用于打包的外层目录。"));
    }
    if current == staged_root && has_multiple_ini_branches(&current)? {
        return Err(AppError::ModInstall(
            "压缩包顶层包含多个疑似独立 EFMI 模组目录，无法安全自动合并；请分别导入。".to_owned(),
        ));
    }
    if current == staged_root {
        warnings.push(
            "未找到作者 mod.json；AEMM 将把当前包根目录视为一个模组并生成内部元数据。".to_owned(),
        );
    }
    Ok(DetectedModRoot {
        path: current,
        warnings,
    })
}

fn find_manifest_roots(staged_root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut roots = Vec::new();
    for entry in WalkDir::new(staged_root)
        .follow_links(false)
        .max_depth(MAX_MANIFEST_SEARCH_DEPTH)
        .sort_by_file_name()
    {
        let entry = entry
            .map_err(|error| AppError::ModInstall(format!("无法分析模组文件结构：{error}")))?;
        if path_is_link_or_reparse_point(entry.path())? {
            return Err(AppError::UnsafePath(format!(
                "待安装内容包含链接或重解析点：{}。",
                entry.path().display()
            )));
        }
        if entry.file_type().is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("mod.json")
        {
            let parent = entry
                .path()
                .parent()
                .ok_or_else(|| AppError::ModInstall("mod.json 缺少父目录。".to_owned()))?;
            if !roots.iter().any(|existing| existing == parent) {
                roots.push(parent.to_path_buf());
            }
        }
    }
    Ok(roots)
}

fn meaningful_direct_entries(path: &Path) -> Result<Vec<fs::DirEntry>, AppError> {
    let mut entries = fs::read_dir(path)
        .map_err(|source| AppError::file_system(path, source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| AppError::file_system(path, source))?;
    entries.retain(|entry| !is_packaging_junk(entry.file_name().to_string_lossy().as_ref()));
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());
    Ok(entries)
}

fn is_packaging_junk(name: &str) -> bool {
    name.eq_ignore_ascii_case("__MACOSX")
        || name.eq_ignore_ascii_case(".DS_Store")
        || name.eq_ignore_ascii_case("Thumbs.db")
        || name.eq_ignore_ascii_case("desktop.ini")
}

fn ensure_contains_file(path: &Path) -> Result<(), AppError> {
    for entry in WalkDir::new(path).follow_links(false) {
        let entry = entry
            .map_err(|error| AppError::ModInstall(format!("无法分析模组文件结构：{error}")))?;
        if entry.depth() > 0 && entry.file_type().is_file() {
            return Ok(());
        }
    }
    Err(AppError::ModInstall(
        "导入内容不包含任何普通文件。".to_owned(),
    ))
}

fn has_multiple_ini_branches(path: &Path) -> Result<bool, AppError> {
    let branches = meaningful_direct_entries(path)?
        .into_iter()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| branch_contains_ini(&entry.path()).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(branches.len() > 1)
}

fn branch_contains_ini(path: &Path) -> Result<Option<PathBuf>, AppError> {
    for entry in WalkDir::new(path).follow_links(false).max_depth(3) {
        let entry = entry
            .map_err(|error| AppError::ModInstall(format!("无法分析 EFMI 文件结构：{error}")))?;
        if entry.file_type().is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("ini"))
        {
            return Ok(Some(path.to_path_buf()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::detect_mod_root;

    #[test]
    fn unwraps_single_packaging_directory() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let payload = directory.path().join("payload");
        fs::create_dir_all(payload.join("Wrapper/Actual"))?;
        fs::write(payload.join("Wrapper/Actual/mod.ini"), b"content")?;
        let detected = detect_mod_root(&payload)?;
        assert_eq!(
            detected.path,
            fs::canonicalize(payload.join("Wrapper/Actual"))?
        );
        assert!(!detected.warnings.is_empty());
        Ok(())
    }

    #[test]
    fn uses_unique_nested_manifest_root() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let payload = directory.path().join("payload");
        fs::create_dir_all(payload.join("docs"))?;
        fs::create_dir_all(payload.join("mod"))?;
        fs::write(payload.join("docs/readme.txt"), b"readme")?;
        fs::write(payload.join("mod/mod.json"), br#"{"id":"a.b","name":"B"}"#)?;
        let detected = detect_mod_root(&payload)?;
        assert_eq!(detected.path, fs::canonicalize(payload.join("mod"))?);
        Ok(())
    }

    #[test]
    fn rejects_multiple_manifest_roots() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let payload = directory.path().join("payload");
        fs::create_dir_all(payload.join("one"))?;
        fs::create_dir_all(payload.join("two"))?;
        fs::write(payload.join("one/mod.json"), b"{}")?;
        fs::write(payload.join("two/mod.json"), b"{}")?;
        assert!(detect_mod_root(&payload).is_err());
        Ok(())
    }
}
