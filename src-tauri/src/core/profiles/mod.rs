use uuid::Uuid;

use crate::{errors::AppError, models::DeploymentManifest};

pub const MAX_PROFILE_NAME_CHARACTERS: usize = 64;

#[derive(Debug, Clone)]
pub struct ProfileSwitchPlan {
    pub source_profile_id: Uuid,
    pub target_profile_id: Uuid,
    pub source_manifests: Vec<DeploymentManifest>,
    pub target_mod_ids: Vec<Uuid>,
    pub warnings: Vec<String>,
}

pub fn validate_profile_name(name: String) -> Result<String, AppError> {
    let normalized = name.trim();
    if normalized.is_empty() {
        return Err(AppError::Profile("Profile 名称不能为空。".to_owned()));
    }
    if normalized.chars().count() > MAX_PROFILE_NAME_CHARACTERS {
        return Err(AppError::Profile(format!(
            "Profile 名称不能超过 {MAX_PROFILE_NAME_CHARACTERS} 个字符。"
        )));
    }
    if normalized.chars().any(char::is_control) {
        return Err(AppError::Profile(
            "Profile 名称不能包含控制字符。".to_owned(),
        ));
    }
    Ok(normalized.to_owned())
}

#[cfg(test)]
mod tests {
    use super::validate_profile_name;

    #[test]
    fn normalizes_and_validates_profile_names() {
        assert_eq!(
            validate_profile_name("  截图配置  ".to_owned())
                .ok()
                .as_deref(),
            Some("截图配置")
        );
        assert!(validate_profile_name(" \n ".to_owned()).is_err());
        assert!(validate_profile_name("a".repeat(65)).is_err());
        assert!(validate_profile_name("bad\0name".to_owned()).is_err());
    }
}
