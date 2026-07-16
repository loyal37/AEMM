use std::{path::PathBuf, sync::Arc};

use aemm_lib::{
    core::{
        deployment::{EfmiCopyDeploymentStrategy, ModDeploymentStrategy},
        mods::{
            ExtractionPolicy, FileSystemModScanner, InstallProgressReporter, ModScanner,
            detect_mod_root, stage_source,
        },
    },
    models::{DeploymentContext, ModImportSourceKind},
};
use uuid::Uuid;
use walkdir::WalkDir;

#[tokio::test]
#[ignore = "requires a user-supplied AEMM_COMPAT_ARCHIVE path"]
async fn validates_external_efmi_package() -> Result<(), Box<dyn std::error::Error>> {
    let archive = std::env::var_os("AEMM_COMPAT_ARCHIVE")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("AEMM_COMPAT_ARCHIVE is not set"))?;
    if !archive.is_absolute() || !archive.is_file() {
        return Err(std::io::Error::other(
            "AEMM_COMPAT_ARCHIVE must reference an existing absolute file",
        )
        .into());
    }

    let staging = tempfile::tempdir()?;
    let payload = staging.path().join("payload");
    let operation_id = Uuid::new_v4();
    let reporter: InstallProgressReporter = Arc::new(|_| {});
    let staged = stage_source(
        &archive,
        &payload,
        operation_id,
        ExtractionPolicy::default(),
        &reporter,
    )?;
    assert_eq!(staged.source_kind, ModImportSourceKind::Zip);
    let detected = detect_mod_root(&staged.staged_root)?;
    let has_ini = WalkDir::new(&detected.path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .any(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
        });
    assert!(has_ini, "external package did not contain an EFMI INI file");
    let canonical_detected = std::fs::canonicalize(&detected.path)?;
    let canonical_staging = std::fs::canonicalize(staging.path())?;
    assert!(canonical_detected.starts_with(canonical_staging));

    let loader_root = staging.path().join("EFMI");
    std::fs::create_dir_all(loader_root.join("Mods"))?;
    std::fs::write(
        loader_root.join("d3dx.ini"),
        "[Include]\ninclude_recursive = Mods\nexclude_recursive = DISABLED*\n",
    )?;
    let strategy = EfmiCopyDeploymentStrategy::open(loader_root).await?;
    let scanned = FileSystemModScanner::new()
        .scan_candidate(&detected.path)
        .await?;
    let repository_root = detected
        .path
        .parent()
        .ok_or_else(|| std::io::Error::other("detected mod root has no parent"))?
        .to_path_buf();
    let context = DeploymentContext {
        profile_id: Uuid::new_v4(),
        mod_id: Uuid::new_v4(),
        repository_root,
        mod_root: detected.path,
        destination_root: strategy.mods_root().to_path_buf(),
        source_content_fingerprint: scanned.content_fingerprint,
        files: scanned.files,
    };
    let plan = strategy.plan_deploy(&context).await?;
    let manifest = strategy.deploy(&context, plan).await?;
    strategy.verify(&manifest).await?;
    let receipt = strategy.begin_revoke(&manifest).await?;
    strategy.finalize_revoke(&receipt).await?;
    Ok(())
}
