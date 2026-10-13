use std::{ffi::OsString, path::Path};

use velopack::{
    Error, VelopackAsset,
    locator::{LocationContext, auto_locate_app_manifest},
};

pub(super) fn start(asset: &VelopackAsset, logs_path: &Path) -> Result<(), Error> {
    super::source::validate_asset(asset)?;

    let locator = auto_locate_app_manifest(LocationContext::FromCurrentExe)?;

    let packages_path = locator.get_packages_dir();

    let package_path = packages_path.join(&asset.FileName);

    // Match the SDK's wait-for-current-process launch, adding its documented
    // --log override to keep application-triggered update logs in NexaNew.
    let arguments = arguments(
        &package_path,
        &locator.get_root_dir(),
        &packages_path,
        &logs_path.join("velopack-update.log"),
        std::process::id(),
    );

    let updater_path = locator.get_update_path();

    velopack::process::run_process(&updater_path, arguments, updater_path.parent(), false, None)?;

    Ok(())
}

pub(super) fn arguments(
    package: &Path,
    root: &Path,
    packages: &Path,
    log: &Path,
    pid: u32,
) -> Vec<OsString> {
    vec![
        "apply".into(),
        "--package".into(),
        package.into(),
        "--waitPid".into(),
        pid.to_string().into(),
        "--root".into(),
        root.into(),
        "--packageDir".into(),
        packages.into(),
        "--log".into(),
        log.into(),
    ]
}
