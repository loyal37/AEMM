use std::{collections::BTreeMap, path::Path};

use async_trait::async_trait;

use crate::{
    core::conflicts::{AnalyzerOutput, ConflictAnalysisSubject, ConflictAnalyzer, conflict_id},
    core::deployment::EFMI_DIRECT_STRATEGY_ID,
    errors::AppError,
    models::{Conflict, ConflictEvidence, ConflictKind, ConflictParticipant, ConflictSeverity},
    utils::validate_relative_path,
};

pub const DEPLOYMENT_PATH_ANALYZER_ID: &str = "deployment.path.v1";

#[derive(Debug, Clone)]
struct PathOccurrence {
    subject_index: usize,
    source_path: std::path::PathBuf,
    target_path: std::path::PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub struct DeploymentPathConflictAnalyzer;

#[async_trait]
impl ConflictAnalyzer for DeploymentPathConflictAnalyzer {
    fn analyzer_id(&self) -> &'static str {
        DEPLOYMENT_PATH_ANALYZER_ID
    }

    async fn analyze(
        &self,
        subjects: &[ConflictAnalysisSubject],
    ) -> Result<AnalyzerOutput, AppError> {
        let mut targets: BTreeMap<String, Vec<PathOccurrence>> = BTreeMap::new();
        for (subject_index, subject) in subjects.iter().enumerate() {
            let direct = subject.manifest.strategy_id == EFMI_DIRECT_STRATEGY_ID;
            if direct {
                if !subject.manifest.destination_directory.is_absolute()
                    || subject.manifest.destination_directory.parent()
                        != Some(subject.manifest.destination_root.as_path())
                {
                    return Err(AppError::UnsafePath(
                        "EFMI 原地模组清单不是 Mods 的直属目录。".to_owned(),
                    ));
                }
            } else {
                validate_relative_path(&subject.manifest.destination_directory)?;
            }
            for entry in &subject.manifest.entries {
                validate_relative_path(&entry.destination_relative)?;
                let target_path = subject
                    .manifest
                    .destination_directory
                    .join(&entry.destination_relative);
                let key = normalized_absolute_key(&subject.manifest.destination_root, &target_path);
                targets.entry(key).or_default().push(PathOccurrence {
                    subject_index,
                    source_path: entry.source_relative.clone(),
                    target_path,
                });
            }
        }

        let mut conflicts = Vec::new();
        for occurrences in targets.into_values() {
            let mut by_subject: BTreeMap<usize, Vec<PathOccurrence>> = BTreeMap::new();
            for occurrence in occurrences {
                by_subject
                    .entry(occurrence.subject_index)
                    .or_default()
                    .push(occurrence);
            }
            if by_subject.len() < 2 {
                continue;
            }

            let display_target = by_subject
                .values()
                .next()
                .and_then(|items| items.first())
                .map(|item| storage_path(&item.target_path))
                .ok_or_else(|| {
                    AppError::DataIntegrity("部署目标冲突组缺少文件证据。".to_owned())
                })?;
            let participants = by_subject
                .into_iter()
                .map(|(index, items)| {
                    let subject = subjects.get(index).ok_or_else(|| {
                        AppError::DataIntegrity("部署目标冲突索引无效。".to_owned())
                    })?;
                    Ok(ConflictParticipant {
                        mod_id: subject.mod_id(),
                        mod_name: subject.mod_name.clone(),
                        load_order: subject.load_order,
                        evidence: items
                            .into_iter()
                            .map(|item| ConflictEvidence {
                                source_path: item.source_path,
                                section: None,
                                detail: format!(
                                    "部署到同一目标 {}",
                                    storage_path(&item.target_path)
                                ),
                            })
                            .collect(),
                    })
                })
                .collect::<Result<Vec<_>, AppError>>()?;
            let id = conflict_id(
                DEPLOYMENT_PATH_ANALYZER_ID,
                ConflictKind::DeploymentPath,
                &display_target,
                participants.iter().map(|participant| participant.mod_id),
            );
            conflicts.push(Conflict {
                id,
                analyzer_id: DEPLOYMENT_PATH_ANALYZER_ID.to_owned(),
                kind: ConflictKind::DeploymentPath,
                severity: ConflictSeverity::Error,
                resource_key: display_target,
                summary: "多个已启用模组写入同一实际部署文件。".to_owned(),
                participants,
                winning_mod_id: None,
            });
        }

        Ok(AnalyzerOutput {
            conflicts,
            ..AnalyzerOutput::default()
        })
    }
}

fn normalized_absolute_key(root: &Path, relative: &Path) -> String {
    format!(
        "{}|{}",
        storage_path(root).to_lowercase(),
        storage_path(relative).to_lowercase()
    )
}

fn storage_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use uuid::Uuid;

    use crate::{
        core::conflicts::{ConflictAnalysisSubject, ConflictAnalyzer},
        models::{ConflictKind, DeploymentEntry, DeploymentManifest},
    };

    use super::DeploymentPathConflictAnalyzer;

    #[tokio::test]
    async fn reports_only_actual_shared_deployment_targets()
    -> Result<(), Box<dyn std::error::Error>> {
        let first = subject("First", 0, "Shared", "Textures/body.dds");
        let second = subject("Second", 1, "Shared", "Textures/body.dds");
        let distinct = subject("Distinct", 2, "Other", "Textures/body.dds");
        let output = DeploymentPathConflictAnalyzer
            .analyze(&[first, second, distinct])
            .await?;

        assert_eq!(output.conflicts.len(), 1);
        assert_eq!(output.conflicts[0].kind, ConflictKind::DeploymentPath);
        assert_eq!(output.conflicts[0].participants.len(), 2);
        assert!(output.conflicts[0].winning_mod_id.is_none());
        Ok(())
    }

    fn subject(
        name: &str,
        load_order: u32,
        directory: &str,
        target: &str,
    ) -> ConflictAnalysisSubject {
        let mod_id = Uuid::new_v4();
        ConflictAnalysisSubject {
            mod_name: name.to_owned(),
            load_order,
            manifest: DeploymentManifest {
                schema_version: 1,
                id: Uuid::new_v4(),
                profile_id: Uuid::new_v4(),
                mod_id,
                strategy_id: "fixture".to_owned(),
                destination_root: PathBuf::from(r"C:\EFMI\Mods"),
                destination_directory: PathBuf::from(directory),
                source_content_fingerprint: "fixture".to_owned(),
                entries: vec![DeploymentEntry {
                    source_relative: PathBuf::from(target),
                    destination_relative: PathBuf::from(target),
                    size_bytes: 1,
                    content_hash: "a".repeat(64),
                }],
                created_at: 1,
            },
        }
    }
}
