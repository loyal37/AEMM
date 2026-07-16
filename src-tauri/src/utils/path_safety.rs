use std::path::{Component, Path, PathBuf};

use crate::errors::AppError;

pub fn validate_relative_path(path: &Path) -> Result<(), AppError> {
    if path.as_os_str().is_empty() {
        return Err(AppError::UnsafePath("路径不能为空。".to_owned()));
    }

    let mut has_normal_component = false;
    for component in path.components() {
        match component {
            Component::Normal(segment) => {
                has_normal_component = true;
                validate_windows_segment(segment.to_string_lossy().as_ref())?
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::UnsafePath(
                    "路径必须是安全的相对路径，不能包含父级、根目录或设备前缀。".to_owned(),
                ));
            }
        }
    }

    if !has_normal_component {
        return Err(AppError::UnsafePath(
            "路径必须包含至少一个文件或目录名。".to_owned(),
        ));
    }

    Ok(())
}

pub fn join_lexically_contained(root: &Path, relative: &Path) -> Result<PathBuf, AppError> {
    validate_relative_path(relative)?;
    Ok(root.join(relative))
}

pub fn ensure_existing_child(root: &Path, candidate: &Path) -> Result<PathBuf, AppError> {
    let canonical_root = root
        .canonicalize()
        .map_err(|source| AppError::file_system(root, source))?;
    let canonical_candidate = candidate
        .canonicalize()
        .map_err(|source| AppError::file_system(candidate, source))?;

    if canonical_candidate == canonical_root || !canonical_candidate.starts_with(&canonical_root) {
        return Err(AppError::UnsafePath(
            "目标不是允许管理目录中的子路径。".to_owned(),
        ));
    }

    Ok(canonical_candidate)
}

fn validate_windows_segment(segment: &str) -> Result<(), AppError> {
    if segment.is_empty()
        || segment.ends_with(['.', ' '])
        || segment.chars().any(|character| {
            matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*') || character <= '\u{001f}'
        })
    {
        return Err(AppError::UnsafePath(
            "路径包含 Windows 不允许的文件名字符。".to_owned(),
        ));
    }

    let stem = match segment.split('.').next() {
        Some(value) => value.to_ascii_uppercase(),
        None => {
            return Err(AppError::UnsafePath("路径包含无效的文件名。".to_owned()));
        }
    };
    let is_reserved = matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );

    if is_reserved {
        return Err(AppError::UnsafePath(
            "路径包含 Windows 保留设备名。".to_owned(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ensure_existing_child, join_lexically_contained, validate_relative_path};

    #[test]
    fn rejects_parent_traversal_and_reserved_names() {
        assert!(validate_relative_path(Path::new("../../Windows/System32")).is_err());
        assert!(validate_relative_path(Path::new(".")).is_err());
        assert!(validate_relative_path(Path::new("safe/CON.ini")).is_err());
        assert!(validate_relative_path(Path::new("safe/mod.ini")).is_ok());
    }

    #[test]
    fn joins_only_lexically_safe_relative_paths() -> Result<(), Box<dyn std::error::Error>> {
        let joined = join_lexically_contained(Path::new("C:/repository"), Path::new("author/mod"))?;
        assert_eq!(joined, Path::new("C:/repository/author/mod"));
        Ok(())
    }

    #[test]
    fn canonical_child_check_rejects_the_root_itself() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        assert!(ensure_existing_child(root.path(), root.path()).is_err());
        Ok(())
    }
}
