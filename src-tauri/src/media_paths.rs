//! Confined existing-file paths shared by media operations.

use std::path::{Component, Path, PathBuf};

pub(crate) fn checked_action_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if !root.is_absolute() {
        return Err("Source folder must be absolute".into());
    }

    let relative = Path::new(relative);

    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Invalid stored file path".into());
    }

    let path = root.join(relative);

    let metadata = std::fs::symlink_metadata(&path).map_err(|_| "File is unavailable on disk")?;

    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Only regular files are supported".into());
    }

    let canonical_root = root
        .canonicalize()
        .map_err(|_| "Source folder is unavailable")?;

    let canonical = path
        .canonicalize()
        .map_err(|_| "File is unavailable on disk")?;

    if !canonical.starts_with(canonical_root) {
        return Err("File resolves outside its source folder".into());
    }
    Ok(path)
}
