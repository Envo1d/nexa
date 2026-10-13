use std::sync::mpsc;

use serde::Serialize;
use tauri::{Manager, ipc::Channel};
use tokio::sync::Mutex;
use velopack::{UpdateCheck, UpdateManager, UpdateOptions};

mod helper;
mod launch;
mod source;

pub(crate) use helper::run as run_helper;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct UpdateState(Mutex<Option<PendingUpdate>>);

#[derive(Clone)]
struct PendingUpdate {
    manager: UpdateManager,
    update: velopack::UpdateInfo,
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
    percent: u8,
}

fn check_installed_update() -> Result<Option<PendingUpdate>, String> {
    if !cfg!(windows) {
        return Err("Package updates are supported on Windows only.".into());
    }

    let manager = UpdateManager::new(
        source::SignedSource,
        Some(UpdateOptions {
            MaximumDeltasBeforeFallback: -1,
            ..Default::default()
        }),
        None,
    )
    .map_err(|error| {
        tracing::warn!(%error, "Velopack installation unavailable");
        "Install the new Nexa Setup from GitHub Releases once to enable package updates. Development and old NSIS installations cannot use this updater.".to_string()
    })?;

    match manager.check_for_updates().map_err(|error| {
        tracing::warn!(%error, "Signed update feed check failed; retry is available");
        "Could not check or verify updates. Check your connection and published release, then retry.".to_string()
    })? {
        UpdateCheck::UpdateAvailable(update) => Ok(Some(PendingUpdate {
            manager,
            update: *update,
        })),
        UpdateCheck::NoUpdateAvailable => Ok(None),
        UpdateCheck::RemoteIsEmpty => {
            tracing::warn!("Update feed is empty; no version comparison was possible");
            Err("No Windows update packages have been published yet. Try again later.".into())
        }
    }
}

#[tauri::command]
#[tracing::instrument(skip_all)]
pub async fn check_for_updates(
    state: tauri::State<'_, UpdateState>,
) -> Result<Option<UpdateInfo>, String> {
    let mut pending = state
        .0
        .try_lock()
        .map_err(|_| "An update operation is already running")?;

    tracing::info!("Checking for signed Velopack updates");
    *pending = None;

    let result = tauri::async_runtime::spawn_blocking(check_installed_update)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Update check worker failed");
            "Could not finish checking for updates. Retry the check.".to_string()
        })??;

    let info = result.as_ref().map(|pending| {
        let release = &pending.update.TargetFullRelease;

        UpdateInfo {
            version: release.Version.clone(),
            notes: (!release.NotesMarkdown.is_empty()).then(|| release.NotesMarkdown.clone()),
        }
    });

    *pending = result;
    tracing::info!(available = info.is_some(), "Update check completed");

    Ok(info)
}

#[tauri::command]
#[tracing::instrument(skip_all)]
pub async fn install_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, UpdateState>,
    progress: Channel<DownloadProgress>,
) -> Result<(), String> {
    let pending = state
        .0
        .try_lock()
        .map_err(|_| "An update operation is already running")?;

    let pending_update = pending
        .as_ref()
        .ok_or("Check for an update before installing")?
        .clone();

    tracing::info!(version = %pending_update.update.TargetFullRelease.Version, "Downloading Velopack update");

    let download = pending_update.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let (sender, receiver) = mpsc::channel::<i16>();

        std::thread::scope(|scope| {
            scope.spawn(move || {
                for percent in receiver {
                    let _ = progress.send(DownloadProgress {
                        percent: percent.clamp(0, 100) as u8,
                    });
                }
            });

            download
                .manager
                .download_updates(&download.update, Some(sender))?;

            // The SDK skips checksum validation when the target is cached.
            source::verify_cached_package(&download.update.TargetFullRelease)
        })
    })
    .await
    .map_err(|error| {
        tracing::error!(%error, "Update download worker failed");
        "Could not finish downloading the update. You can retry.".to_string()
    })?
    .map_err(|error| {
        tracing::error!(%error, "Update download or verification failed");
        "Could not download or verify the update. Installation was not started.".to_string()
    })?;

    tracing::info!("Verified update download completed");

    let stage = app.state::<crate::stage::StageState>();

    let Some(_operation) = stage.try_operation() else {
        tracing::warn!("Update deferred because a media operation is running");

        return Err(
            "A media operation is running. Wait for it to finish, then retry the update.".into(),
        );
    };

    // Launch before cleanup so a launch failure leaves the database usable.
    // Velopack waits for our PID to exit before replacing application files.
    let logs_path = app
        .state::<crate::config::ConfigStore>()
        .snapshot()?
        .paths
        .logs_path;

    tauri::async_runtime::spawn_blocking(move || {
        launch::start(&pending_update.update.TargetFullRelease, &logs_path)
    })
    .await
    .map_err(|error| {
        tracing::error!(%error, "Update launch worker failed");
        "Could not start the updater. You can retry.".to_string()
    })?
    .map_err(|error| {
        tracing::error!(%error, "Independent updater handoff failed; application remains open");
        "Could not start the updater. Nexa remains open; you can retry.".to_string()
    })?;

    tracing::info!("Independent updater ready; preparing shutdown");
    prepare_exit(&app).await;
    // Keep both operation guards until the process actually terminates.
    std::process::exit(0);
}

async fn prepare_exit(app: &tauri::AppHandle) {
    use tauri_plugin_window_state::{AppHandleExt, StateFlags};

    if let Err(error) = app.save_window_state(StateFlags::SIZE | StateFlags::POSITION) {
        tracing::warn!(%error, "Could not save window geometry before update");
    }

    app.state::<crate::database::Database>().close().await;
    app.cleanup_before_exit();

    tracing::info!("Update shutdown preparation completed");
    app.state::<crate::logging::ShutdownLogs>().flush();
}
