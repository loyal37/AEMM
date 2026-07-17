use async_trait::async_trait;

use crate::{
    errors::AppError,
    models::{Conflict, ConflictKind, ConflictSeverity, DeploymentManifest},
};

mod efmi_ini;
mod path_analyzer;

pub use efmi_ini::{EFMI_INI_ANALYZER_ID, EfmiIniConflictAnalyzer};
pub use path_analyzer::{DEPLOYMENT_PATH_ANALYZER_ID, DeploymentPathConflictAnalyzer};

#[derive(Debug, Clone)]
pub struct ConflictAnalysisSubject {
    pub mod_name: String,
    pub load_order: u32,
    pub manifest: DeploymentManifest,
}

impl ConflictAnalysisSubject {
    pub fn mod_id(&self) -> uuid::Uuid {
        self.manifest.mod_id
    }
}

#[derive(Debug, Default)]
pub struct AnalyzerOutput {
    pub conflicts: Vec<Conflict>,
    pub analyzed_ini_files: u64,
    pub warnings: Vec<String>,
}

#[async_trait]
pub trait ConflictAnalyzer: Send + Sync {
    fn analyzer_id(&self) -> &'static str;

    async fn analyze(
        &self,
        subjects: &[ConflictAnalysisSubject],
    ) -> Result<AnalyzerOutput, AppError>;
}

pub struct ModConflictDetector {
    analyzers: Vec<Box<dyn ConflictAnalyzer>>,
}

impl ModConflictDetector {
    pub fn new(analyzers: Vec<Box<dyn ConflictAnalyzer>>) -> Self {
        Self { analyzers }
    }

    pub fn for_efmi() -> Self {
        Self::new(vec![
            Box::new(DeploymentPathConflictAnalyzer),
            Box::new(EfmiIniConflictAnalyzer),
        ])
    }

    pub async fn analyze(
        &self,
        subjects: &[ConflictAnalysisSubject],
    ) -> Result<AnalyzerOutput, AppError> {
        let mut output = AnalyzerOutput::default();
        for analyzer in &self.analyzers {
            let mut analyzer_output = analyzer.analyze(subjects).await.map_err(|error| {
                tracing::warn!(
                    analyzer_id = analyzer.analyzer_id(),
                    error = %error,
                    diagnostic = ?error,
                    "conflict analyzer failed"
                );
                error
            })?;
            output.conflicts.append(&mut analyzer_output.conflicts);
            output.analyzed_ini_files = output
                .analyzed_ini_files
                .checked_add(analyzer_output.analyzed_ini_files)
                .ok_or_else(|| AppError::Conflict("冲突分析文件计数超过支持范围。".to_owned()))?;
            output.warnings.append(&mut analyzer_output.warnings);
        }
        output.conflicts.sort_by(|left, right| {
            severity_rank(left.severity)
                .cmp(&severity_rank(right.severity))
                .then_with(|| kind_rank(left.kind).cmp(&kind_rank(right.kind)))
                .then_with(|| left.resource_key.cmp(&right.resource_key))
                .then_with(|| left.id.cmp(&right.id))
        });
        output.warnings.sort();
        output.warnings.dedup();
        Ok(output)
    }
}

pub(crate) fn conflict_id(
    analyzer_id: &str,
    kind: ConflictKind,
    resource_key: &str,
    mod_ids: impl IntoIterator<Item = uuid::Uuid>,
) -> String {
    let mut ids = mod_ids
        .into_iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    ids.sort();
    let mut hasher = blake3::Hasher::new();
    hasher.update(analyzer_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(format!("{kind:?}").as_bytes());
    hasher.update(&[0]);
    hasher.update(resource_key.as_bytes());
    for id in ids {
        hasher.update(&[0]);
        hasher.update(id.as_bytes());
    }
    format!("conflict-{}", &hasher.finalize().to_hex()[..20])
}

fn severity_rank(severity: ConflictSeverity) -> u8 {
    match severity {
        ConflictSeverity::Error => 0,
        ConflictSeverity::Warning => 1,
        ConflictSeverity::Information => 2,
    }
}

fn kind_rank(kind: ConflictKind) -> u8 {
    match kind {
        ConflictKind::DeploymentPath => 0,
        ConflictKind::EfmiNamespace => 1,
        ConflictKind::EfmiTextureOverride => 2,
        ConflictKind::EfmiShaderOverride => 3,
    }
}
