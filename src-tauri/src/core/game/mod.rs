use std::path::Path;

use async_trait::async_trait;

use crate::{
    errors::AppError,
    models::{DetectedGameInstallation, GameInstallation, GameValidation, LaunchSpec},
};

mod efmi;
mod endfield;

pub use efmi::EfmiAdapter;
pub use endfield::EndfieldAdapter;

#[async_trait]
pub trait GameAdapter: Send + Sync {
    fn adapter_id(&self) -> &'static str;

    async fn discover(&self) -> Result<Vec<DetectedGameInstallation>, AppError>;

    async fn validate(&self, candidate: &Path) -> Result<GameValidation, AppError>;

    async fn launch_spec(&self, installation: &GameInstallation) -> Result<LaunchSpec, AppError>;
}
