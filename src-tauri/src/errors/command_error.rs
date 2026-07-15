use serde::Serialize;

use super::AppError;

pub type CommandResult<T> = Result<T, CommandError>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl From<AppError> for CommandError {
    fn from(error: AppError) -> Self {
        tracing::error!(error = %error, diagnostic = ?error, "command failed");

        let (code, message) = match error {
            AppError::ConfigValidation(message) => ("CONFIG_INVALID", message),
            AppError::UnsafePath(message) => ("UNSAFE_PATH", message),
            AppError::NotAvailable(message) => ("NOT_AVAILABLE", message),
            AppError::ConfigFormat(_) => (
                "CONFIG_FORMAT_ERROR",
                "配置文件格式无效，请检查本地日志。".to_owned(),
            ),
            AppError::FileSystem { .. } | AppError::PathResolution(_) => (
                "FILESYSTEM_ERROR",
                "文件系统操作失败，请检查路径权限和本地日志。".to_owned(),
            ),
            AppError::Database(_) | AppError::Migration(_) => (
                "DATABASE_ERROR",
                "数据库操作失败，请检查本地日志。".to_owned(),
            ),
            AppError::BackgroundTask(_) => (
                "BACKGROUND_TASK_ERROR",
                "后台任务意外终止，请检查本地日志。".to_owned(),
            ),
            AppError::Logging(_) => ("LOGGING_ERROR", "日志系统初始化失败。".to_owned()),
        };

        Self {
            code: code.to_owned(),
            message,
            details: None,
        }
    }
}
