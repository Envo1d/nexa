//! stage: file actions responsibility.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use serde::Serialize;
use sqlx::SqlitePool;

use super::{StageState, inspect};
use crate::{config::ConfigStore, database::Database, media_paths::checked_action_path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileAction {
    Open,
    Reveal,
    Delete,
    Share,
}

pub(super) fn validate_selection(
    action: &str,
    ids: &[String],
) -> Result<(FileAction, Vec<(String, i64)>), String> {
    let action = match action {
        "open" => FileAction::Open,
        "reveal" => FileAction::Reveal,
        "delete" => FileAction::Delete,
        "share" => FileAction::Share,
        _ => return Err("Invalid file action or selection (maximum 500 files)".into()),
    };

    if ids.is_empty() || ids.len() > 500 || (action == FileAction::Share && ids.len() != 1) {
        return Err("Invalid file action or selection (maximum 500 files)".into());
    }

    let mut seen = HashSet::new();

    let mut selection = Vec::with_capacity(ids.len());

    // Validate every ID before recovery or any Shell/filesystem side effects.
    for raw_id in ids {
        let id = raw_id
            .parse::<i64>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or("Invalid media ID")?;

        if seen.insert(id) {
            selection.push((raw_id.clone(), id));
        }
    }

    Ok((action, selection))
}

#[derive(sqlx::FromRow)]
pub(super) struct ActionFile {
    pub(super) root_path: String,
    kind: String,
    pub(super) relative_path: String,
    pub(super) file_revision: Option<String>,
    needs_distribution: i64,
    library_assignment: Option<String>,
}

pub(super) async fn action_file(
    pool: &SqlitePool,
    configured_root: Option<&Path>,
    library: bool,
    id: i64,
) -> Result<ActionFile, String> {
    let file: ActionFile = sqlx::query_as(concat!(
        "SELECT s.root_path,s.kind,m.relative_path,m.file_revision,m.needs_distribution,",
        "m.library_assignment FROM media m JOIN sources s ON s.id=m.source_id ",
        "WHERE m.id=? AND m.availability='present'",
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|_| "Cannot read file record")?
    .ok_or("File record is unavailable")?;

    let matches_root = configured_root == Some(Path::new(&file.root_path));

    let needs_distribution = file.needs_distribution != 0;

    let eligible = if library {
        let assigned = configured_root.is_some()
            && file.library_assignment.as_deref() == configured_root.and_then(Path::to_str);

        let physical_library = file.kind == "library" && matches_root;

        (assigned || physical_library) && !needs_distribution
    } else {
        let physical_stage = file.kind == "stage" && matches_root;

        needs_distribution || physical_stage && file.library_assignment.is_none()
    };

    if !eligible {
        return Err("File is outside the current media selection".into());
    }

    Ok(file)
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
#[allow(clippy::too_many_arguments)] // Tauri injects the three managed dependencies.
pub async fn stage_file_action(
    app: tauri::AppHandle,
    ids: Vec<String>,
    action: String,
    library: Option<bool>,
    expected_revisions: Option<HashMap<String, String>>,
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, StageState>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<FileActionReport, String> {
    let (action, selection) = validate_selection(&action, &ids)?;

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

    for (raw_id, id) in selection {
        let result = perform_action(
            &app,
            database.pool(),
            configured_root.as_deref(),
            library,
            action,
            id,
            expected_revisions
                .as_ref()
                .and_then(|values| values.get(&raw_id))
                .cloned(),
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

    if library && action == FileAction::Delete && !report.succeeded.is_empty() {
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
    pool: &SqlitePool,
    configured_root: Option<&Path>,
    library: bool,
    action: FileAction,
    id: i64,
    expected_revision: Option<String>,
) -> Result<(), String> {
    let file = action_file(pool, configured_root, library, id).await?;

    let root = PathBuf::from(file.root_path);

    let worker_app = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_opener::OpenerExt;

        let path = checked_action_path(&root, &file.relative_path)?;

        if let Some(expected) = expected_revision {
            let current = inspect(path.clone(), &root)?;

            if current.is_none_or(|current| current.revision != expected) {
                return Err("File changed since comparison; restart duplicate search".into());
            }
        }

        match action {
            FileAction::Delete => {
                if let Some(expected) = file.file_revision {
                    let current = inspect(path.clone(), &root)?;
                    if current.is_none_or(|current| current.revision != expected) {
                        return Err("File changed; rescan its source before deleting".into());
                    }
                }
                recycle_file(&path)
            }
            FileAction::Share => crate::share::show(worker_app, &path),
            FileAction::Open => worker_app
                .opener()
                .open_path(
                    path.to_str().ok_or("File path is not valid Unicode")?,
                    None::<&str>,
                )
                .map_err(|_| "Windows could not open the file or folder".into()),
            FileAction::Reveal => worker_app
                .opener()
                .reveal_item_in_dir(path)
                .map_err(|_| "Windows could not open the file or folder".into()),
        }
    })
    .await
    .map_err(|_| "File action worker failed")??;

    if action == FileAction::Delete {
        sqlx::query("DELETE FROM media WHERE id=?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(|_| "File deleted, but database cleanup failed; rescan its source")?;
    }

    Ok(())
}
