use std::{ffi::OsString, path::Path};

use velopack::{
    Error, VelopackAsset,
    locator::{LocationContext, auto_locate_app_manifest},
};

pub(super) fn start(asset: &VelopackAsset, logs_path: &Path) -> Result<(), Error> {
    super::source::validate_asset(asset)?;

    let locator = auto_locate_app_manifest(LocationContext::FromCurrentExe)?;

    super::helper::start(asset, &locator, logs_path).map_err(Error::Other)
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
        "--silent".into(),
        "--norestart".into(),
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
