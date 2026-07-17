use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("configuration is invalid: {0}")]
    ConfigValidation(String),

    #[error("configuration data could not be parsed")]
    ConfigFormat(#[source] serde_json::Error),

    #[error("filesystem operation failed for {path}")]
    FileSystem {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("required path could not be resolved: {0}")]
    PathResolution(String),

    #[error("database operation failed")]
    Database(#[from] sqlx::Error),

    #[error("database migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("background task failed")]
    BackgroundTask(#[from] tokio::task::JoinError),

    #[error("logging initialization failed: {0}")]
    Logging(String),

    #[error("unsafe path rejected: {0}")]
    UnsafePath(String),

    #[error("the requested operation is not available: {0}")]
    NotAvailable(String),

    #[error("game installation validation failed: {0}")]
    GameValidation(String),

    #[error("EFMI loader validation failed: {0}")]
    LoaderValidation(String),

    #[error("failed to start process {path}")]
    ProcessLaunch {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("mod repository scan failed: {0}")]
    ModScan(String),

    #[error("mod metadata is invalid: {0}")]
    ModMetadata(String),

    #[error("archive processing failed: {0}")]
    Archive(String),

    #[error("mod installation failed: {0}")]
    ModInstall(String),

    #[error("mod deployment failed: {0}")]
    Deployment(String),

    #[error("mod conflict analysis failed: {0}")]
    Conflict(String),

    #[error("stored application data is inconsistent: {0}")]
    DataIntegrity(String),
}

impl AppError {
    pub fn file_system(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::FileSystem {
            path: path.into(),
            source,
        }
    }
}
