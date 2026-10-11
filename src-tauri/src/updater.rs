use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde::Serialize;
use tauri::{Manager, ipc::Channel};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct UpdateState(Mutex<Option<PendingUpdate>>);

struct PendingUpdate {
    update: Update,
    shutdown_started: Arc<AtomicBool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    notes: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    downloaded: u64,
    total: Option<u64>,
}

#[tauri::command]
#[tracing::instrument(skip_all)]
pub async fn check_for_updates(
    app: tauri::AppHandle,
    state: tauri::State<'_, UpdateState>,
) -> Result<Option<UpdateInfo>, String> {
    let mut pending = state
        .0
        .try_lock()
        .map_err(|_| "An update operation is already running")?;
    tracing::info!("Checking for updates");
    let handle = app.clone();
    let shutdown_started = Arc::new(AtomicBool::new(false));
    let shutdown_flag = shutdown_started.clone();
    let result = async {
        app.updater_builder()
            .on_before_exit(move || {
                shutdown_flag.store(true, Ordering::Release);
                prepare_exit(&handle);
            })
            .timeout(Duration::from_secs(30))
            .build()?
            .check()
            .await
    }
    .await;
    match result {
        Ok(update) => {
            let info = update.as_ref().map(|update| UpdateInfo {
                version: update.version.clone(),
                notes: update.body.clone(),
            });
            tracing::info!(available = info.is_some(), "Update check completed");
            *pending = update.map(|update| PendingUpdate {
                update,
                shutdown_started,
            });
            Ok(info)
        }
        Err(error) => {
            tracing::warn!(%error, "Update check failed; manual retry is available");
            Err("Could not check for updates. Check your connection and published release, then retry.".into())
        }
    }
}

#[tauri::command]
#[tracing::instrument(skip_all)]
pub async fn install_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, UpdateState>,
    progress: Channel<DownloadProgress>,
) -> Result<(), String> {
    // Keep the pending update on failure so the user can explicitly retry.
    let pending = state
        .0
        .try_lock()
        .map_err(|_| "An update operation is already running")?;
    let pending_update = pending
        .as_ref()
        .ok_or("Check for an update before installing")?;
    let update = &pending_update.update;
    tracing::info!(version = %update.version, "Downloading signed update");
    let mut downloaded = 0_u64;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.send(DownloadProgress { downloaded, total });
            },
            || tracing::info!("Update download completed"),
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, "Update download or signature verification failed");
            "Could not download or verify the update. Installation was not started.".to_string()
        })?;

    let stage = app.state::<crate::stage::StageState>();
    let Some(_operation) = stage.try_operation() else {
        tracing::warn!("Update installation deferred because a media operation is running");
        return Err(
            "A media operation is running. Wait for it to finish, then retry the update.".into(),
        );
    };
    tracing::info!("Verified update ready; starting installer");
    let update = update.clone();
    let shutdown_started = pending_update.shutdown_started.clone();
    let environment = app.env();
    tauri::async_runtime::spawn_blocking(move || {
        let result = update.install(bytes);
        if let Err(error) = &result
            && shutdown_started.load(Ordering::Acquire)
        {
            // The plugin invokes shutdown before ShellExecuteW. If Windows
            // cannot launch the installer, SQLite/resources are already closed.
            // Restart through the saved process environment, without using any
            // Tauri APIs after cleanup or retrying installation automatically.
            tracing::error!(%error, "Installer launch failed after shutdown; restarting Nexa for recovery");
            tauri::process::restart(&environment);
        }
        result
    })
        .await
        .map_err(|error| {
            tracing::error!(%error, "Update installation worker failed");
            "Could not start the update installer".to_string()
        })?
        .map_err(|error| {
            tracing::error!(%error, "Update installer failed");
            "Could not start the update installer. You can retry.".to_string()
        })
}

// Windows updater exits the process directly. Normal RunEvent::Exit is not
// sufficient to close SQLite and save window geometry on this path.
pub fn prepare_exit(app: &tauri::AppHandle) {
    use tauri_plugin_window_state::{AppHandleExt, StateFlags};
    tracing::info!("Preparing application shutdown for update");
    if let Err(error) = app.save_window_state(StateFlags::SIZE | StateFlags::POSITION) {
        tracing::warn!(%error, "Could not save window geometry before update");
    }
    tauri::async_runtime::block_on(app.state::<crate::database::Database>().close());
    app.cleanup_before_exit();
    tracing::info!("Update shutdown preparation completed");
}
