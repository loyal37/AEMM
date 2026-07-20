use async_trait::async_trait;

use crate::{
    errors::AppError,
    models::{DeploymentContext, DeploymentManifest, DeploymentPlan, DeploymentRevokeReceipt},
};

mod efmi_copy;
mod efmi_direct;

pub(crate) use efmi_copy::verify_deployment_marker;
pub use efmi_copy::{EFMI_COPY_STRATEGY_ID, EfmiCopyDeploymentStrategy};
pub use efmi_direct::{EFMI_DIRECT_STRATEGY_ID, EfmiDirectDeploymentStrategy};

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

    async fn begin_revoke(
        &self,
        manifest: &DeploymentManifest,
    ) -> Result<DeploymentRevokeReceipt, AppError>;

    async fn finalize_revoke(&self, receipt: &DeploymentRevokeReceipt) -> Result<(), AppError>;

    async fn rollback_revoke(&self, receipt: &DeploymentRevokeReceipt) -> Result<(), AppError>;

    async fn rollback_deploy(&self, manifest: &DeploymentManifest) -> Result<(), AppError>;

    async fn verify(&self, manifest: &DeploymentManifest) -> Result<(), AppError>;
}
