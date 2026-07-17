use std::{collections::BTreeSet, sync::Arc, time::SystemTime};

use tokio::sync::Mutex;

use crate::{
    core::conflicts::{ConflictAnalysisSubject, ModConflictDetector},
    database::{ConflictStore, Database},
    errors::AppError,
    models::ConflictReport,
};

pub struct ConflictService {
    store: ConflictStore,
    detector: ModConflictDetector,
    deployment_lock: Arc<Mutex<()>>,
}

impl ConflictService {
    pub fn new(database: &Database, deployment_lock: Arc<Mutex<()>>) -> Self {
        Self {
            store: ConflictStore::new(database.pool().clone()),
            detector: ModConflictDetector::for_efmi(),
            deployment_lock,
        }
    }

    pub async fn analyze_active(&self) -> Result<ConflictReport, AppError> {
        let _guard = self.deployment_lock.lock().await;
        let (profile_id, stored_subjects) = self.store.active_subjects().await?;
        let subjects = stored_subjects
            .into_iter()
            .map(|subject| {
                if subject.profile_id != profile_id {
                    return Err(AppError::DataIntegrity(
                        "冲突分析快照包含其他 Profile 的模组。".to_owned(),
                    ));
                }
                Ok(ConflictAnalysisSubject {
                    mod_name: subject.mod_name,
                    load_order: subject.load_order,
                    manifest: subject.manifest,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        let output = self.detector.analyze(&subjects).await?;
        let affected_mod_ids = output
            .conflicts
            .iter()
            .flat_map(|conflict| conflict.participants.iter())
            .map(|participant| participant.mod_id)
            .collect::<BTreeSet<_>>();
        let enabled_mods = u64::try_from(subjects.len())
            .map_err(|_| AppError::Conflict("已启用模组计数超过支持范围。".to_owned()))?;
        let affected_mods = u64::try_from(affected_mod_ids.len())
            .map_err(|_| AppError::Conflict("冲突模组计数超过支持范围。".to_owned()))?;
        tracing::info!(
            profile_id = %profile_id,
            enabled_mods,
            conflicts = output.conflicts.len(),
            affected_mods,
            analyzed_ini_files = output.analyzed_ini_files,
            "active profile conflict analysis completed"
        );
        Ok(ConflictReport {
            profile_id,
            generated_at: unix_timestamp_seconds()?,
            enabled_mods,
            analyzed_ini_files: output.analyzed_ini_files,
            affected_mods,
            conflicts: output.conflicts,
            load_order_verified: false,
            load_order_note: "列表显示 AEMM Profile 中保存的顺序；当前 EFMI 递归加载与同 Hash 覆盖的实际胜出规则尚未得到可靠验证，因此不会推断胜出模组。".to_owned(),
            warnings: output.warnings,
        })
    }
}

fn unix_timestamp_seconds() -> Result<i64, AppError> {
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| AppError::DataIntegrity("系统时间早于 Unix Epoch。".to_owned()))?;
    i64::try_from(duration.as_secs())
        .map_err(|_| AppError::DataIntegrity("系统时间超出支持范围。".to_owned()))
}
