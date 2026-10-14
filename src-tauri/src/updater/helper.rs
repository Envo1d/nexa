use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use serde::Serialize;
use tauri::{WebviewUrl, WebviewWindowBuilder};
use velopack::{
    VelopackAsset,
    locator::{VelopackLocator, create_config_from_root_dir},
};

use crate::config::{AppLocale, AppTheme};

mod session;

#[cfg(test)]
mod tests;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    phase: &'static str,
    version: String,
    message: String,
    locale: AppLocale,
    theme: AppTheme,
}

struct HelperState {
    directory: PathBuf,
    snapshot: Mutex<Snapshot>,
    started: AtomicBool,
    finished: AtomicBool,
}

impl HelperState {
    fn status(&self, phase: &'static str, message: &str) {
        let mut snapshot = self
            .snapshot
            .lock()
            .unwrap_or_else(|error| error.into_inner());

        snapshot.phase = phase;
        snapshot.message = message.into();
    }
}

pub(super) fn start(
    asset: &VelopackAsset,
    locator: &VelopackLocator,
    logs: &Path,
    locale: AppLocale,
    theme: AppTheme,
) -> Result<(), String> {
    let root = logs.join("update-sessions");

    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    cleanup_previous_sessions(&root);

    let temporary = tempfile::Builder::new()
        .prefix("helper-")
        .tempdir_in(&root)
        .map_err(|error| error.to_string())?;

    let directory = temporary.path();

    let mut installation = create_config_from_root_dir(locator.get_root_dir());

    installation.PackagesDir = locator.get_packages_dir();

    let request = session::Request {
        asset: asset.clone(),
        installation,
        logs: logs.into(),
        parent_pid: std::process::id(),
        locale,
        theme,
    };

    session::write(directory, "request.json", &request)?;

    // The helper executable and WebView profile must survive replacement of current/.
    let helper = directory.join("NexaUpdate.exe");

    fs::copy(
        std::env::current_exe().map_err(|error| error.to_string())?,
        &helper,
    )
    .map_err(|error| error.to_string())?;

    hidden_command(&helper)
        .arg("--nexa-update-ui")
        .arg(directory)
        .current_dir(directory)
        .spawn()
        .map_err(|error| error.to_string())?;

    let directory = temporary.keep();

    let launched = session::wait_for_launch(&directory, Duration::from_secs(45));

    if let Err(error) = launched {
        let _ = session::write(&directory, "cancel.json", &true);

        return Err(error);
    }

    if let Err(error) = session::write(&directory, "commit.json", &true) {
        let _ = session::write(&directory, "cancel.json", &true);

        return Err(error);
    }

    tracing::info!(version = %asset.Version, ?locale, ?theme, "Independent update window accepted handoff");

    Ok(())
}

fn cleanup_previous_sessions(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();

        let eligible = name.to_string_lossy().starts_with("helper-")
            && entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink());

        if eligible && entry.path().join("finished.json").is_file() {
            // Windows refuses to remove an executing helper. Completed sessions
            // are reclaimed on the next update, confined to this owned directory.
            if let Err(error) = fs::remove_dir_all(entry.path()) {
                tracing::debug!(%error, "Previous update session retained");
                // A locked WebView file can cause partial deletion. Retain the
                // completion marker so the next update can retry cleanup.
                let _ = session::write(&entry.path(), "finished.json", &true);
            }
        }
    }
}

fn hidden_command(path: &Path) -> Command {
    let mut command = Command::new(path);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    command
}

#[tauri::command]
fn update_snapshot(state: tauri::State<'_, Arc<HelperState>>) -> Snapshot {
    state
        .snapshot
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

#[tauri::command]
fn update_ui_ready(app: tauri::AppHandle, state: tauri::State<'_, Arc<HelperState>>) {
    if state.started.swap(true, Ordering::SeqCst) {
        return;
    }

    let state = state.inner().clone();

    thread::spawn(move || {
        if let Err(error) = apply(&app, &state) {
            if state.directory.join("commit.json").exists() {
                tracing::error!(%error, "Independent update failed");
            } else {
                // The parent command owns failed-handoff error reporting.
                tracing::warn!(%error, "Independent update handoff rejected");
            }
            let _ = session::write(
                &state.directory,
                "launched.json",
                &Err::<(), _>(error.clone()),
            );
            state.status("error", &error);
            state.finished.store(true, Ordering::SeqCst);
        }
    });
}

#[tauri::command]
fn close_update_window(app: tauri::AppHandle, state: tauri::State<'_, Arc<HelperState>>) {
    if state.finished.load(Ordering::SeqCst) {
        app.exit(0);
    }
}

fn apply(app: &tauri::AppHandle, state: &HelperState) -> Result<(), String> {
    let request: session::Request = session::read(&state.directory, "request.json")?;

    if state.directory.join("cancel.json").exists() {
        return Err("Update cancelled. Nexa remains open; retry from Settings.".into());
    }

    state.status("verifying", "Checking the downloaded package…");
    super::source::validate_asset(&request.asset).map_err(|error| error.to_string())?;

    let package = request
        .installation
        .PackagesDir
        .join(&request.asset.FileName);

    super::source::verify_package(&package, &request.asset).map_err(|_| {
        "The downloaded package failed verification. Retry from Nexa Settings.".to_string()
    })?;

    let arguments = super::launch::arguments(
        &package,
        &request.installation.RootAppDir,
        &request.installation.PackagesDir,
        &request.logs.join("velopack-update.log"),
        request.parent_pid,
    );

    let mut updater = hidden_command(&request.installation.UpdateExePath)
        .args(arguments)
        .current_dir(&state.directory)
        .spawn()
        .map_err(|_| "Could not start the installer. Nexa remains open; retry from Settings.")?;

    // The worker is already waiting for the main PID. Require explicit consent
    // from that process before allowing it to close and release the file locks.
    let handoff = session::write(&state.directory, "launched.json", &Ok::<(), String>(()))
        .and_then(|()| session::await_commit(&state.directory, Duration::from_secs(60)));

    if let Err(error) = handoff {
        let _ = updater.kill();
        let _ = updater.wait();

        return Err(error);
    }

    tracing::info!(version = %request.asset.Version, "Applying update in independent helper");
    state.status("installing", "Closing Nexa and installing the new version…");

    let status = updater
        .wait()
        .map_err(|_| "Could not determine the installation result.")?;

    if !status.success() {
        return Err(
            "Installation failed. See NexaNew logs for details, then reopen Nexa or run Setup."
                .into(),
        );
    }

    let manifest = request.installation.load_manifest().map_err(|_| {
        "Installation finished, but the installed version could not be verified. Run Nexa Setup."
    })?;

    if manifest.id != request.asset.PackageId
        || manifest.version.to_string() != request.asset.Version
    {
        return Err("The installed version does not match the update. Run Nexa Setup.".into());
    }

    state.status("restarting", "Update installed. Opening Nexa…");
    tracing::info!(version = %request.asset.Version, "Update installed; restarting Nexa");

    hidden_command(
        &request
            .installation
            .CurrentBinaryDir
            .join(&manifest.main_exe),
    )
    .current_dir(&request.installation.RootAppDir)
    .spawn()
    .map_err(|_| "Update installed, but Nexa could not start. Open Nexa from its shortcut.")?;

    thread::sleep(Duration::from_millis(700));
    app.exit(0);

    Ok(())
}

pub(crate) fn run(directory: PathBuf) {
    let Ok(request) = session::read::<session::Request>(&directory, "request.json") else {
        return;
    };

    let log_guard = match crate::logging::init(&request.logs) {
        Ok(guard) => guard,
        Err(_) => {
            let _ = session::write(
                &directory,
                "launched.json",
                &Err::<(), _>("Could not initialize updater logs. Nexa remains open."),
            );
            let _ = session::write(&directory, "finished.json", &true);

            return;
        }
    };

    let log_guard = crate::logging::ShutdownLogs(Arc::new(Mutex::new(Some(log_guard))));

    let state = Arc::new(HelperState {
        directory: directory.clone(),
        snapshot: Mutex::new(Snapshot {
            phase: "preparing",
            version: request.asset.Version,
            message: "Preparing your update…".into(),
            locale: request.locale,
            theme: request.theme,
        }),
        started: AtomicBool::new(false),
        finished: AtomicBool::new(false),
    });

    let mut context = tauri::generate_context!();

    context.config_mut().app.windows.clear();
    context.config_mut().identifier = "com.envo1d.nexanew.update".into();

    let window_state = state.clone();

    let watchdog = state.clone();

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(45));

        if !watchdog.started.swap(true, Ordering::SeqCst) {
            tracing::warn!("Update interface readiness timed out; main application remains open");
            watchdog.status("error", "The update interface did not load. Close this window and retry from Nexa Settings.");
            watchdog.finished.store(true, Ordering::SeqCst);
        }
    });

    let finished_directory = directory.clone();

    let result = tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            update_snapshot,
            update_ui_ready,
            close_update_window
        ])
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "updater", WebviewUrl::App("update.html".into()))
                .title(if request.locale == AppLocale::Russian {
                    "Обновление Nexa"
                } else {
                    "Nexa Update"
                })
                .inner_size(420.0, 440.0)
                .resizable(false)
                .decorations(false)
                .center()
                .data_directory(directory.join("webview"))
                .build()?;

            tracing::info!("Independent update window created");

            Ok(())
        })
        .on_window_event(move |_, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && !window_state.finished.load(Ordering::SeqCst)
            {
                api.prevent_close();
            }
        })
        .build(context);

    match result {
        Ok(app) => app.run(move |_, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let _ = session::write(&finished_directory, "finished.json", &true);
                tracing::info!("Independent update window closed");
                log_guard.flush();
            }
        }),
        Err(error) => {
            tracing::warn!(%error, "Could not create independent update window; parent owns error reporting");
            let _ = session::write(
                &finished_directory,
                "launched.json",
                &Err::<(), _>("Could not create the update window. Nexa remains open."),
            );
            let _ = session::write(&finished_directory, "finished.json", &true);
            log_guard.flush();
        }
    }
}
