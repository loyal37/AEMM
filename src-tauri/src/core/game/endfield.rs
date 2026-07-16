use std::{
    cmp::Reverse,
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

use async_trait::async_trait;

use crate::{
    core::game::GameAdapter,
    errors::AppError,
    models::{
        DetectedGameInstallation, GameDiscoverySource, GameEdition, GameInstallation,
        GameValidation, GameVersionInfo, LaunchSpec,
    },
};

const ADAPTER_ID: &str = "endfield.local";
const GAME_DIRECTORY_NAME: &str = "EndField Game";
const GAME_EXECUTABLE_NAME: &str = "Endfield.exe";
const APP_INFO_RELATIVE_PATH: &str = "Endfield_Data/app.info";
const MAX_APP_INFO_BYTES: u64 = 4 * 1024;

#[derive(Debug, Default)]
pub struct EndfieldAdapter;

impl EndfieldAdapter {
    pub const fn new() -> Self {
        Self
    }

    fn discover_sync() -> Vec<DetectedGameInstallation> {
        let mut candidates = launcher_registry_candidates();
        candidates.extend(known_install_root_candidates());

        let mut seen = HashSet::new();
        let mut detected = Vec::new();
        for candidate in candidates {
            let key = normalized_path_key(&candidate.path);
            if !seen.insert(key) {
                continue;
            }

            let validation = validate_sync(&candidate.path);
            if validation.valid {
                detected.push(DetectedGameInstallation {
                    source: candidate.source,
                    validation,
                });
            }
        }

        detected.sort_by_key(|candidate| Reverse(candidate.validation.confidence));
        detected
    }
}

#[async_trait]
impl GameAdapter for EndfieldAdapter {
    fn adapter_id(&self) -> &'static str {
        ADAPTER_ID
    }

    async fn discover(&self) -> Result<Vec<DetectedGameInstallation>, AppError> {
        tokio::task::spawn_blocking(Self::discover_sync)
            .await
            .map_err(AppError::from)
    }

    async fn validate(&self, candidate: &Path) -> Result<GameValidation, AppError> {
        let candidate = candidate.to_path_buf();
        tokio::task::spawn_blocking(move || validate_sync(&candidate))
            .await
            .map_err(AppError::from)
    }

    async fn launch_spec(&self, installation: &GameInstallation) -> Result<LaunchSpec, AppError> {
        let validation = self.validate(&installation.installation_root).await?;
        let verified = validation.installation.ok_or_else(|| {
            AppError::NotAvailable("游戏目录已失效，请重新检测或选择游戏目录。".to_owned())
        })?;

        Ok(LaunchSpec {
            executable: verified.executable,
            working_directory: verified.installation_root,
            arguments: Vec::new(),
        })
    }
}

fn validate_sync(candidate: &Path) -> GameValidation {
    let mut evidence = Vec::new();
    let mut issues = Vec::new();

    if !candidate.is_absolute() {
        issues.push("游戏目录必须是绝对路径。".to_owned());
        return invalid_validation(evidence, issues);
    }

    let root = match fs::canonicalize(candidate) {
        Ok(path) if path.is_dir() => path,
        Ok(_) => {
            issues.push("选择的路径不是目录。".to_owned());
            return invalid_validation(evidence, issues);
        }
        Err(_) => {
            issues.push("选择的游戏目录不存在或无法访问。".to_owned());
            return invalid_validation(evidence, issues);
        }
    };

    let executable = match canonical_direct_file(&root, GAME_EXECUTABLE_NAME) {
        Some(path) => {
            evidence.push("找到目录直属的 Endfield.exe。".to_owned());
            path
        }
        None => {
            issues.push("未找到安全且位于游戏目录内的 Endfield.exe。".to_owned());
            return invalid_validation(evidence, issues);
        }
    };

    let app_info = root.join(APP_INFO_RELATIVE_PATH);
    if !is_existing_path_contained(&root, &app_info, false) {
        issues.push("未找到有效的 Endfield_Data/app.info 身份文件。".to_owned());
        return invalid_validation(evidence, issues);
    }

    let app_info_metadata = match fs::metadata(&app_info) {
        Ok(metadata) => metadata,
        Err(_) => {
            issues.push("无法读取 Endfield_Data/app.info。".to_owned());
            return invalid_validation(evidence, issues);
        }
    };
    if app_info_metadata.len() > MAX_APP_INFO_BYTES {
        issues.push("Endfield_Data/app.info 大小异常。".to_owned());
        return invalid_validation(evidence, issues);
    }

    let app_identity = match fs::read_to_string(&app_info) {
        Ok(contents) => contents,
        Err(_) => {
            issues.push("Endfield_Data/app.info 不是可识别的文本文件。".to_owned());
            return invalid_validation(evidence, issues);
        }
    };
    let identity_lines = app_identity
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if identity_lines.as_slice() != ["Hypergryph", "Endfield"] {
        issues.push("app.info 的发行商或产品标识与终末地不匹配。".to_owned());
        return invalid_validation(evidence, issues);
    }
    evidence.push("app.info 已确认发行商 Hypergryph 与产品 Endfield。".to_owned());

    let mut confidence = 70;
    for (file_name, description) in [
        ("UnityPlayer.dll", "找到 UnityPlayer.dll。"),
        ("GameAssembly.dll", "找到 GameAssembly.dll。"),
    ] {
        if canonical_direct_file(&root, file_name).is_some() {
            confidence += 15;
            evidence.push(description.to_owned());
        } else {
            issues.push(format!("缺少终末地运行文件 {file_name}，目录验证失败。"));
            return invalid_validation(evidence, issues);
        }
    }

    let edition = infer_edition(&root);
    if edition == GameEdition::Unknown {
        issues.push("安装身份有效，但尚不能可靠判断国服或国际服。".to_owned());
    } else {
        evidence.push("路径特征与已验证的鹰角启动器国服布局一致。".to_owned());
    }

    GameValidation {
        valid: true,
        confidence,
        evidence,
        issues,
        installation: Some(GameInstallation {
            adapter_id: ADAPTER_ID.to_owned(),
            edition,
            installation_root: root,
            executable,
            loader_root: None,
            version: GameVersionInfo {
                value: None,
                source: None,
                note: "当前安装未发现可验证的游戏版本源；EXE 版本属于 Unity 引擎，未作为游戏版本展示。"
                    .to_owned(),
            },
        }),
    }
}

fn invalid_validation(evidence: Vec<String>, issues: Vec<String>) -> GameValidation {
    GameValidation {
        valid: false,
        confidence: 0,
        evidence,
        issues,
        installation: None,
    }
}

fn canonical_direct_file(root: &Path, file_name: &str) -> Option<PathBuf> {
    let path = root.join(file_name);
    let canonical = fs::canonicalize(path).ok()?;
    if !canonical.is_file() || canonical.parent() != Some(root) {
        return None;
    }
    Some(canonical)
}

fn is_existing_path_contained(root: &Path, candidate: &Path, directory: bool) -> bool {
    let Ok(canonical) = fs::canonicalize(candidate) else {
        return false;
    };
    canonical.starts_with(root)
        && canonical != root
        && if directory {
            canonical.is_dir()
        } else {
            canonical.is_file()
        }
}

fn infer_edition(root: &Path) -> GameEdition {
    let path = normalized_path_key(root);
    if path.contains("hypergryph launcher") || path.contains("gryphlink") {
        GameEdition::China
    } else {
        GameEdition::Unknown
    }
}

#[derive(Debug)]
struct DiscoveryCandidate {
    path: PathBuf,
    source: GameDiscoverySource,
}

fn known_install_root_candidates() -> Vec<DiscoveryCandidate> {
    let mut candidates = Vec::new();
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(program_files) = env::var_os(variable) {
            add_launcher_layouts(
                Path::new(&program_files),
                GameDiscoverySource::KnownInstallRoot,
                &mut candidates,
            );
        }
    }

    #[cfg(windows)]
    for drive in b'A'..=b'Z' {
        let drive_root = PathBuf::from(format!("{}:\\", char::from(drive)));
        if drive_root.exists() {
            add_launcher_layouts(
                &drive_root,
                GameDiscoverySource::KnownInstallRoot,
                &mut candidates,
            );
        }
    }

    candidates
}

fn add_launcher_layouts(
    parent: &Path,
    source: GameDiscoverySource,
    candidates: &mut Vec<DiscoveryCandidate>,
) {
    for launcher_directory in ["Hypergryph Launcher", "GRYPHLINK"] {
        candidates.push(DiscoveryCandidate {
            path: parent
                .join(launcher_directory)
                .join("games")
                .join(GAME_DIRECTORY_NAME),
            source,
        });
    }
}

#[cfg(windows)]
fn launcher_registry_candidates() -> Vec<DiscoveryCandidate> {
    use winreg::{
        RegKey,
        enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        },
    };

    const UNINSTALL_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
    let mut candidates = Vec::new();
    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            let root = RegKey::predef(hive);
            let Ok(uninstall) = root.open_subkey_with_flags(UNINSTALL_KEY, KEY_READ | view) else {
                continue;
            };

            for subkey_name in uninstall.enum_keys().filter_map(Result::ok) {
                let Ok(subkey) = uninstall.open_subkey_with_flags(&subkey_name, KEY_READ) else {
                    continue;
                };
                let Ok(display_name) = subkey.get_value::<String, _>("DisplayName") else {
                    continue;
                };
                let normalized_name = display_name.to_lowercase();
                if !normalized_name.contains("鹰角启动器")
                    && !normalized_name.contains("hypergryph launcher")
                    && !normalized_name.contains("gryphlink")
                {
                    continue;
                }
                let Ok(install_location) = subkey.get_value::<String, _>("InstallLocation") else {
                    continue;
                };
                if install_location.trim().is_empty() {
                    continue;
                }
                candidates.push(DiscoveryCandidate {
                    path: PathBuf::from(install_location)
                        .join("games")
                        .join(GAME_DIRECTORY_NAME),
                    source: GameDiscoverySource::LauncherRegistry,
                });
            }
        }
    }
    candidates
}

#[cfg(not(windows))]
fn launcher_registry_candidates() -> Vec<DiscoveryCandidate> {
    Vec::new()
}

fn normalized_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::core::game::{EndfieldAdapter, GameAdapter};

    fn create_valid_fixture(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(root.join("Endfield_Data"))?;
        fs::write(root.join("Endfield.exe"), b"fixture")?;
        fs::write(
            root.join("Endfield_Data/app.info"),
            b"Hypergryph\nEndfield\n",
        )?;
        fs::write(root.join("UnityPlayer.dll"), b"fixture")?;
        fs::write(root.join("GameAssembly.dll"), b"fixture")?;
        Ok(())
    }

    use std::path::Path;

    #[tokio::test]
    async fn validates_verified_endfield_layout() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        create_valid_fixture(directory.path())?;

        let validation = EndfieldAdapter::new().validate(directory.path()).await?;
        assert!(validation.valid);
        assert_eq!(validation.confidence, 100);
        assert!(validation.installation.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_executable_only_false_positive() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        fs::write(directory.path().join("Endfield.exe"), b"fixture")?;

        let validation = EndfieldAdapter::new().validate(directory.path()).await?;
        assert!(!validation.valid);
        assert!(validation.installation.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_spoofed_app_identity() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        create_valid_fixture(directory.path())?;
        fs::write(
            directory.path().join("Endfield_Data/app.info"),
            b"Other\nEndfield\n",
        )?;

        let validation = EndfieldAdapter::new().validate(directory.path()).await?;
        assert!(!validation.valid);
        Ok(())
    }
}
