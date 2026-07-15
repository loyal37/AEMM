use async_trait::async_trait;
use uuid::Uuid;

use crate::{errors::AppError, models::Profile};

#[derive(Debug, Clone)]
pub struct ProfileSwitchPlan {
    pub source_profile_id: Uuid,
    pub target_profile_id: Uuid,
    pub enable_mods: Vec<Uuid>,
    pub disable_mods: Vec<Uuid>,
    pub warnings: Vec<String>,
}

#[async_trait]
pub trait ProfileManager: Send + Sync {
    async fn list(&self) -> Result<Vec<Profile>, AppError>;
    async fn create(&self, name: String) -> Result<Profile, AppError>;
    async fn prepare_switch(
        &self,
        current_profile_id: Uuid,
        target_profile_id: Uuid,
    ) -> Result<ProfileSwitchPlan, AppError>;
    async fn apply_switch(&self, plan: ProfileSwitchPlan) -> Result<Profile, AppError>;
}
