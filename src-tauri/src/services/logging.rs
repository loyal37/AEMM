use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{errors::AppError, models::LogLevel};

pub struct LoggingGuard {
    _file_guard: WorkerGuard,
}

pub fn initialize_logging(log_directory: &Path, level: LogLevel) -> Result<LoggingGuard, AppError> {
    let file_appender = tracing_appender::rolling::daily(log_directory, "aemm.log");
    let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::new(level.as_filter()),
    };

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(file_writer);
    let console_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_target(false)
        .with_writer(std::io::stderr);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(console_layer)
        .try_init()
        .map_err(|error| AppError::Logging(error.to_string()))?;

    Ok(LoggingGuard {
        _file_guard: file_guard,
    })
}
