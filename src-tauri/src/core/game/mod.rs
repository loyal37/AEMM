use std::path::Path;

use async_trait::async_trait;

use crate::{
    errors::AppError,
    models::{GameInstallation, GameValidation, LaunchSpec},
};

#[async_trait]
pub trait GameAdapter: Send + Sync {
    fn adapter_id(&self) -> &'static str;

    async fn discover(&self) -> Result<Vec<GameInstallation>, AppError>;

    async fn validate(&self, candidate: &Path) -> Result<GameValidation, AppError>;

    async fn launch_spec(&self, installation: &GameInstallation) -> Result<LaunchSpec, AppError>;
}
