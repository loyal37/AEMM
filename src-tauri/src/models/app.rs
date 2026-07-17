use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppSettings {
    pub schema_version: u32,
    pub language: String,
    pub theme: ThemePreference,
    #[serde(default)]
    pub onboarding_completed: bool,
    pub game: GameSettings,
    pub storage: StorageSettings,
    pub log_level: LogLevel,
}

impl AppSettings {
    pub fn defaults(repository_path: PathBuf, staging_path: PathBuf) -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            language: "zh-CN".to_owned(),
            theme: ThemePreference::Dark,
            onboarding_completed: false,
            game: GameSettings::default(),
            storage: StorageSettings {
                repository_path,
                staging_path,
            },
            log_level: LogLevel::Info,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameSettings {
    pub adapter_id: String,
    pub edition: Option<String>,
    pub installation_path: Option<PathBuf>,
    pub loader_root: Option<PathBuf>,
    pub launch_mode: LaunchMode,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            adapter_id: "endfield.local".to_owned(),
            edition: None,
            installation_path: None,
            loader_root: None,
            launch_mode: LaunchMode::EfmiLoader,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageSettings {
    pub repository_path: PathBuf,
    pub staging_path: PathBuf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ThemePreference {
    Dark,
    System,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LaunchMode {
    Game,
    EfmiLoader,
    ExternalLauncher,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub const fn as_filter(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrap {
    pub app_name: String,
    pub app_version: String,
    pub runtime_mode: &'static str,
    pub database_ready: bool,
    pub config_path: PathBuf,
    pub database_path: PathBuf,
    pub log_directory: PathBuf,
    pub settings: AppSettings,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::AppSettings;

    #[test]
    fn legacy_settings_default_onboarding_to_incomplete() -> Result<(), Box<dyn std::error::Error>>
    {
        let settings = AppSettings::defaults(
            PathBuf::from("C:/aemm/repository"),
            PathBuf::from("C:/aemm/staging"),
        );
        let mut value = serde_json::to_value(settings)?;
        let object = value
            .as_object_mut()
            .ok_or("serialized settings must be an object")?;
        object.remove("onboardingCompleted");

        let restored: AppSettings = serde_json::from_value(value)?;

        assert!(!restored.onboarding_completed);
        Ok(())
    }
}
