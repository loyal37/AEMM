use async_trait::async_trait;

use crate::{
    errors::AppError,
    models::{Conflict, DeploymentPlan},
};

#[async_trait]
pub trait ConflictAnalyzer: Send + Sync {
    fn analyzer_id(&self) -> &'static str;

    async fn analyze(&self, ordered_plans: &[DeploymentPlan]) -> Result<Vec<Conflict>, AppError>;
}

pub struct ModConflictDetector {
    analyzers: Vec<Box<dyn ConflictAnalyzer>>,
}

impl ModConflictDetector {
    pub fn new(analyzers: Vec<Box<dyn ConflictAnalyzer>>) -> Self {
        Self { analyzers }
    }

    pub async fn analyze(
        &self,
        ordered_plans: &[DeploymentPlan],
    ) -> Result<Vec<Conflict>, AppError> {
        let mut conflicts = Vec::new();
        for analyzer in &self.analyzers {
            conflicts.extend(analyzer.analyze(ordered_plans).await?);
        }
        Ok(conflicts)
    }
}
