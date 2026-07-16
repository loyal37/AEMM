use std::{collections::HashMap, fs, path::Path, path::PathBuf};

use crate::{
    errors::AppError,
    models::{EfmiValidation, LaunchSpec},
};

const MAX_INI_BYTES: u64 = 1024 * 1024;
const LOADER_EXECUTABLE: &str = "3DMigotoLoader.exe";

#[derive(Debug, Default)]
pub struct EfmiAdapter;

impl EfmiAdapter {
    pub const fn new() -> Self {
        Self
    }

    pub async fn validate(
        &self,
        candidate: &Path,
        game_executable: &Path,
    ) -> Result<EfmiValidation, AppError> {
        let candidate = candidate.to_path_buf();
        let game_executable = game_executable.to_path_buf();
        tokio::task::spawn_blocking(move || validate_sync(&candidate, &game_executable))
            .await
            .map_err(AppError::from)
    }

    pub fn launch_spec(&self, validation: &EfmiValidation) -> Result<LaunchSpec, AppError> {
        if !validation.valid || !validation.launch_ready {
            return Err(AppError::NotAvailable(
                "EFMI 加载器未通过启动校验，请检查加载器路径与 d3dx.ini 的 launch 设置。"
                    .to_owned(),
            ));
        }

        let executable = validation
            .executable
            .clone()
            .ok_or_else(|| AppError::NotAvailable("EFMI 启动程序路径不可用。".to_owned()))?;
        let working_directory = validation
            .root
            .clone()
            .ok_or_else(|| AppError::NotAvailable("EFMI 根目录不可用。".to_owned()))?;

        Ok(LaunchSpec {
            executable,
            working_directory,
            arguments: Vec::new(),
        })
    }
}

fn validate_sync(candidate: &Path, game_executable: &Path) -> EfmiValidation {
    let mut evidence = Vec::new();
    let mut issues = Vec::new();

    if !candidate.is_absolute() {
        issues.push("EFMI 目录必须是绝对路径。".to_owned());
        return invalid_validation(evidence, issues);
    }

    let root = match fs::canonicalize(candidate) {
        Ok(path) if path.is_dir() => path,
        _ => {
            issues.push("EFMI 目录不存在、无法访问或不是目录。".to_owned());
            return invalid_validation(evidence, issues);
        }
    };
    let game_executable = match fs::canonicalize(game_executable) {
        Ok(path) if path.is_file() => path,
        _ => {
            issues.push("已配置的游戏可执行文件已失效。".to_owned());
            return invalid_validation(evidence, issues);
        }
    };

    let loader_executable = match canonical_direct_file(&root, LOADER_EXECUTABLE) {
        Some(path) => {
            evidence.push(format!("找到 {LOADER_EXECUTABLE}。"));
            path
        }
        None => {
            issues.push(format!(
                "未找到安全且位于 EFMI 根目录内的 {LOADER_EXECUTABLE}。"
            ));
            return invalid_validation(evidence, issues);
        }
    };

    if canonical_direct_file(&root, "d3d11.dll").is_none() {
        issues.push("未找到 EFMI 所需的 d3d11.dll。".to_owned());
        return invalid_validation(evidence, issues);
    }
    evidence.push("找到 d3d11.dll。".to_owned());

    let mods_directory = root.join("Mods");
    if !canonical_contained_directory(&root, &mods_directory) {
        issues.push("未找到安全且位于 EFMI 根目录内的 Mods 目录。".to_owned());
        return invalid_validation(evidence, issues);
    }
    evidence.push("找到 EFMI Mods 目录。".to_owned());

    let ini_path = root.join("d3dx.ini");
    if canonical_direct_file(&root, "d3dx.ini").is_none() {
        issues.push("未找到安全且位于 EFMI 根目录内的 d3dx.ini。".to_owned());
        return invalid_validation(evidence, issues);
    }
    let ini_metadata = match fs::metadata(&ini_path) {
        Ok(metadata) if metadata.len() <= MAX_INI_BYTES => metadata,
        Ok(_) => {
            issues.push("d3dx.ini 大小异常。".to_owned());
            return invalid_validation(evidence, issues);
        }
        Err(_) => {
            issues.push("无法读取 d3dx.ini。".to_owned());
            return invalid_validation(evidence, issues);
        }
    };
    let _ = ini_metadata;
    let contents = match fs::read_to_string(&ini_path) {
        Ok(contents) => contents,
        Err(_) => {
            issues.push("d3dx.ini 不是可识别的 UTF-8 文本。".to_owned());
            return invalid_validation(evidence, issues);
        }
    };
    let loader_settings = parse_section(&contents, "Loader");

    if !loader_settings
        .get("target")
        .is_some_and(|value| value.eq_ignore_ascii_case("Endfield.exe"))
    {
        issues.push("d3dx.ini 的 Loader.target 不是 Endfield.exe。".to_owned());
        return invalid_validation(evidence, issues);
    }
    evidence.push("d3dx.ini 的加载目标为 Endfield.exe。".to_owned());

    if !loader_settings
        .get("module")
        .is_some_and(|value| value.eq_ignore_ascii_case("d3d11.dll"))
    {
        issues.push("d3dx.ini 的 Loader.module 不是 d3d11.dll。".to_owned());
        return invalid_validation(evidence, issues);
    }
    evidence.push("d3dx.ini 使用 d3d11.dll 模块。".to_owned());

    let configured_game_executable = loader_settings.get("launch").map(|value| {
        let value = PathBuf::from(value);
        if value.is_absolute() {
            value
        } else {
            root.join(value)
        }
    });
    let launch_ready = configured_game_executable
        .as_deref()
        .and_then(|path| fs::canonicalize(path).ok())
        .is_some_and(|path| path == game_executable);

    if launch_ready {
        evidence.push("d3dx.ini 的 launch 路径与当前游戏可执行文件一致。".to_owned());
    } else {
        issues.push(
            "d3dx.ini 的 launch 路径缺失、已失效或与当前游戏目录不一致；在修正前不能通过 EFMI 启动。"
                .to_owned(),
        );
    }

    EfmiValidation {
        valid: true,
        launch_ready,
        root: Some(root),
        executable: Some(loader_executable),
        configured_game_executable,
        evidence,
        issues,
    }
}

fn parse_section(contents: &str, requested_section: &str) -> HashMap<String, String> {
    let mut current_section = String::new();
    let mut values = HashMap::new();

    for line in contents.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current_section = section.trim().to_owned();
            continue;
        }
        if !current_section.eq_ignore_ascii_case(requested_section) {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        values.insert(
            key.trim().to_ascii_lowercase(),
            value.trim().trim_matches('"').to_owned(),
        );
    }

    values
}

fn canonical_direct_file(root: &Path, file_name: &str) -> Option<PathBuf> {
    let canonical = fs::canonicalize(root.join(file_name)).ok()?;
    if canonical.is_file() && canonical.parent() == Some(root) {
        Some(canonical)
    } else {
        None
    }
}

fn canonical_contained_directory(root: &Path, candidate: &Path) -> bool {
    let Ok(canonical) = fs::canonicalize(candidate) else {
        return false;
    };
    canonical.is_dir() && canonical != root && canonical.starts_with(root)
}

fn invalid_validation(evidence: Vec<String>, issues: Vec<String>) -> EfmiValidation {
    EfmiValidation {
        valid: false,
        launch_ready: false,
        root: None,
        executable: None,
        configured_game_executable: None,
        evidence,
        issues,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use crate::core::game::EfmiAdapter;

    fn create_fixture(
        root: &Path,
        game_executable: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(root.join("Mods"))?;
        fs::write(root.join("3DMigotoLoader.exe"), b"fixture")?;
        fs::write(root.join("d3d11.dll"), b"fixture")?;
        fs::write(
            root.join("d3dx.ini"),
            format!(
                "[Loader]\ntarget = Endfield.exe\nlaunch = {}\nmodule = d3d11.dll\n",
                game_executable.display()
            ),
        )?;
        Ok(())
    }

    #[tokio::test]
    async fn accepts_launch_ready_efmi_layout() -> Result<(), Box<dyn std::error::Error>> {
        let game = tempfile::tempdir()?;
        let game_executable = game.path().join("Endfield.exe");
        fs::write(&game_executable, b"fixture")?;
        let loader = tempfile::tempdir()?;
        create_fixture(loader.path(), &game_executable)?;

        let validation = EfmiAdapter::new()
            .validate(loader.path(), &game_executable)
            .await?;
        assert!(validation.valid);
        assert!(validation.launch_ready);
        Ok(())
    }

    #[tokio::test]
    async fn reports_stale_launch_path_without_rejecting_loader()
    -> Result<(), Box<dyn std::error::Error>> {
        let game = tempfile::tempdir()?;
        let game_executable = game.path().join("Endfield.exe");
        fs::write(&game_executable, b"fixture")?;
        let other_game = game.path().join("OldEndfield.exe");
        fs::write(&other_game, b"fixture")?;
        let loader = tempfile::tempdir()?;
        create_fixture(loader.path(), &other_game)?;

        let validation = EfmiAdapter::new()
            .validate(loader.path(), &game_executable)
            .await?;
        assert!(validation.valid);
        assert!(!validation.launch_ready);
        assert!(!validation.issues.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_incorrect_target() -> Result<(), Box<dyn std::error::Error>> {
        let game = tempfile::tempdir()?;
        let game_executable = game.path().join("Endfield.exe");
        fs::write(&game_executable, b"fixture")?;
        let loader = tempfile::tempdir()?;
        create_fixture(loader.path(), &game_executable)?;
        fs::write(
            loader.path().join("d3dx.ini"),
            "[Loader]\ntarget = OtherGame.exe\nmodule = d3d11.dll\n",
        )?;

        let validation = EfmiAdapter::new()
            .validate(loader.path(), &game_executable)
            .await?;
        assert!(!validation.valid);
        Ok(())
    }
}
