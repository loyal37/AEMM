use async_trait::async_trait;

use crate::{
    errors::AppError,
    models::{DeploymentContext, DeploymentManifest, DeploymentPlan},
};

#[async_trait]
pub trait ModDeploymentStrategy: Send + Sync {
    fn strategy_id(&self) -> &'static str;

    async fn plan_deploy(&self, context: &DeploymentContext) -> Result<DeploymentPlan, AppError>;

    async fn deploy(
        &self,
        context: &DeploymentContext,
        plan: DeploymentPlan,
    ) -> Result<DeploymentManifest, AppError>;

    async fn plan_revoke(&self, manifest: &DeploymentManifest) -> Result<DeploymentPlan, AppError>;

    async fn revoke(&self, manifest: &DeploymentManifest) -> Result<(), AppError>;

    async fn verify(&self, manifest: &DeploymentManifest) -> Result<(), AppError>;
}
