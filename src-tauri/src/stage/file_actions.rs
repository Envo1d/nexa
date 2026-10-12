//! stage: file actions responsibility.

use std::path::{Path, PathBuf};

use serde::Serialize;
use sqlx::Row;

use super::{StageState, inspect};
use crate::{config::ConfigStore, database::Database};

pub(crate) fn checked_action_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    use std::path::Component;

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

#[cfg(windows)]
fn recycle_file(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::UI::Shell::{
            FO_DELETE, FOF_ALLOWUNDO, FOF_NO_CONNECTED_ELEMENTS, FOF_NOCONFIRMATION,
            FOF_WANTNUKEWARNING, SHFILEOPSTRUCTW, SHFileOperationW,
        },
        core::PCWSTR,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();

    if wide[..wide.len() - 2].contains(&0) {
        return Err("Invalid file path".into());
    }

    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(wide.as_ptr()),
        fFlags: (FOF_ALLOWUNDO.0
            | FOF_NOCONFIRMATION.0
            | FOF_WANTNUKEWARNING.0
            | FOF_NO_CONNECTED_ELEMENTS.0) as u16,
        ..Default::default()
    };
    // The fully qualified, double-NUL path lives through the synchronous Shell call.
    let result = unsafe { SHFileOperationW(&mut operation) };

    if result != 0 || operation.fAnyOperationsAborted.as_bool() {
        return Err("Windows could not delete the file, or deletion was cancelled".into());
    }

    if path.try_exists().map_err(|_| "Cannot verify deletion")? {
        return Err("File still exists after deletion".into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn recycle_file(_path: &Path) -> Result<(), String> {
    Err("File recycling is currently supported on Windows only".into())
}

#[derive(Serialize)]
pub struct FileActionReport {
    succeeded: Vec<String>,
    failed: Vec<String>,
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(action, count = ids.len()), err)]
pub async fn stage_file_action(
    app: tauri::AppHandle,
    ids: Vec<String>,
    action: String,
    library: Option<bool>,
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, StageState>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<FileActionReport, String> {
    if !matches!(action.as_str(), "open" | "reveal" | "delete" | "share")
        || ids.is_empty()
        || ids.len() > 500
        || (action == "share" && ids.len() != 1)
    {
        return Err("Invalid file action or selection (maximum 500 files)".into());
    }

    let _guard = state.operation().await;
    crate::disk_sync::recover(database.pool()).await?;

    let settings = config.snapshot()?.settings;

    let library = library.unwrap_or(false);

    let configured_root = if library {
        settings.library_path
    } else {
        settings.staging_path
    };

    let mut report = FileActionReport {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };

    tracing::info!(library, "Media file action started");

    let mut seen = std::collections::HashSet::new();

    for raw_id in ids {
        let id = raw_id.parse::<i64>().map_err(|_| "Invalid media ID")?;

        if !seen.insert(id) {
            continue;
        }

        let result = perform_action(
            &app,
            database.pool(),
            &configured_root,
            library,
            &action,
            id,
        )
        .await;

        match result {
            Ok(()) => report.succeeded.push(raw_id),
            Err(error) => {
                tracing::warn!(media_id = id, reason = %error, "Media file action failed");
                report.failed.push(format!("File {raw_id}: {error}"));
            }
        }
    }

    if action == "delete" && !report.succeeded.is_empty() {
        crate::disk_sync::request(&app);
    }

    tracing::info!(
        succeeded = report.succeeded.len(),
        failed = report.failed.len(),
        "Media file action completed"
    );
    Ok(report)
}

async fn perform_action(
    app: &tauri::AppHandle,
    pool: &sqlx::SqlitePool,
    configured_root: &Option<PathBuf>,
    library: bool,
    action: &str,
    id: i64,
) -> Result<(), String> {
    let row = sqlx::query(concat!(
        "SELECT s.root_path,s.kind,m.relative_path,m.file_revision,m.needs_distribution,",
        "m.library_assignment FROM media m JOIN sources s ON s.id=m.source_id WHERE m.id=?",
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|_| "Cannot read file record")?
    .ok_or("File record is unavailable")?;

    let root = PathBuf::from(row.get::<String, _>("root_path"));
    let assignment = row.get::<Option<String>, _>("library_assignment");
    let source_kind = row.get::<String, _>("kind");
    let needs_distribution = row.get::<i64, _>("needs_distribution") != 0;
    let matches_root = configured_root.as_ref() == Some(&root);

    let eligible = if library {
        let assigned = configured_root.is_some()
            && assignment.as_deref() == configured_root.as_ref().and_then(|path| path.to_str());
        (assigned || source_kind == "library" && matches_root) && !needs_distribution
    } else {
        needs_distribution || source_kind == "stage" && matches_root && assignment.is_none()
    };

    if !eligible {
        return Err("File is outside the current media selection".into());
    }

    let relative: String = row.get("relative_path");
    let revision: Option<String> = row.get("file_revision");
    let worker_action = action.to_owned();
    let worker_app = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_opener::OpenerExt;

        let path = checked_action_path(&root, &relative)?;

        match worker_action.as_str() {
            "delete" => {
                if let Some(expected) = revision {
                    let current = inspect(path.clone(), &root)?;
                    if current.is_none_or(|current| current.revision != expected) {
                        return Err("File changed; rescan its source before deleting".into());
                    }
                }
                recycle_file(&path)
            }
            "share" => crate::share::show(worker_app, &path),
            "open" => worker_app
                .opener()
                .open_path(
                    path.to_str().ok_or("File path is not valid Unicode")?,
                    None::<&str>,
                )
                .map_err(|_| "Windows could not open the file or folder".into()),
            _ => worker_app
                .opener()
                .reveal_item_in_dir(path)
                .map_err(|_| "Windows could not open the file or folder".into()),
        }
    })
    .await
    .map_err(|_| "File action worker failed")??;

    if action == "delete" {
        sqlx::query("DELETE FROM media WHERE id=?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(|_| "File deleted, but database cleanup failed; rescan its source")?;
    }

    Ok(())
}
