//! Application composition root and native lifecycle.

#[cfg(desktop)]
use crate::updater;
use crate::{
    config, database, disk_sync, legacy_import, library, logging, replacement, stage, taxonomy,
};

pub fn run() {
    use tauri::Manager;

    #[cfg(desktop)]
    use tauri_plugin_window_state::{AppHandleExt, StateFlags};

    let paths = config::AppPaths::resolve().expect("Could not resolve NexaNew storage");

    let log_guard = logging::init(&paths.logs_path).expect("Could not initialize NexaNew tracing");

    let log_guard = std::sync::Arc::new(std::sync::Mutex::new(Some(log_guard)));

    let setup_log_guard = log_guard.clone();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Starting NexaNew");

    let builder = tauri::Builder::default();

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        use tauri::Manager;

        let _span = tracing::info_span!("activate_existing_instance").entered();

        tracing::info!("Repeated launch detected");

        if let Some(window) = app.get_webview_window("main") {
            for (operation, result) in [
                ("show", window.show()),
                ("unminimize", window.unminimize()),
                ("focus", window.set_focus()),
            ] {
                if let Err(error) = result {
                    tracing::warn!(operation, %error, "Could not activate the main window");
                } else {
                    tracing::debug!(operation, "Window activation step completed");
                }
            }
        } else {
            tracing::warn!("Main window unavailable for repeated launch");
        }
    }));

    #[cfg(desktop)]
    let builder = builder.plugin(
        tauri_plugin_window_state::Builder::default()
            .with_state_flags(StateFlags::SIZE | StateFlags::POSITION)
            .with_filter(|label| label == "main")
            .skip_initial_state("main")
            .build(),
    );

    let result = builder
        .manage(logging::ShutdownLogs(log_guard.clone()))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .setup(move |app| {
            let _span = tracing::info_span!("application_setup").entered();

            let result = setup(app, paths);

            if let Err(error) = &result {
                tracing::error!(%error, "Application setup failed");
                // Tauri can panic on setup failure during its event loop. Release
                // uses panic=abort, so flush here while the worker is still alive.
                drop(setup_log_guard.lock().unwrap().take());
            }
            result
        })
        .invoke_handler(tauri::generate_handler![
            updater::check_for_updates,
            updater::install_update,
            config::get_config,
            config::save_config,
            config::open_logs,
            disk_sync::get_disk_sync_status,
            disk_sync::retry_disk_sync,
            disk_sync::get_duplicate_order,
            disk_sync::save_duplicate_order,
            legacy_import::find_legacy_database,
            legacy_import::import_legacy_database,
            taxonomy::get_taxonomy,
            taxonomy::rename_taxonomy,
            taxonomy::delete_taxonomy,
            taxonomy::get_redistribution_files,
            stage::start_stage_scan,
            stage::start_library_scan,
            library::get_library_files,
            replacement::get_replacement_source,
            replacement::get_replacement_candidates,
            replacement::replace_library_file,
            library::save_library_metadata,
            library::distribute_files,
            library::set_library_favorite,
            library::save_library_tags,
            stage::get_stage_scan_status,
            stage::get_stage_files,
            stage::stage_file_action,
            stage::get_cache_usage,
            stage::get_stage_preview
        ])
        .build(tauri::generate_context!());

    let result = result.map(|app| {
        app.run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                #[cfg(desktop)]
                {
                    let _span = tracing::info_span!("save_window_state", window = "main").entered();

                    tracing::info!("Saving window position and size");

                    match app.save_window_state(StateFlags::SIZE | StateFlags::POSITION) {
                        Ok(()) => tracing::info!("Window position and size saved"),
                        Err(error) => tracing::warn!(%error, "Window state could not be saved; previous geometry may be restored next launch"),
                    }
                }
                tauri::async_runtime::block_on(app.state::<database::Database>().close());
            }
        });
    });

    match &result {
        Ok(()) => tracing::info!("Application stopped"),
        Err(error) if log_guard.lock().unwrap().is_some() => {
            tracing::error!(%error, "Application failed")
        }
        Err(_) => {}
    }
    // Flush the worker explicitly before a possible panic in aborting release builds.
    drop(log_guard);
    result.expect("error while running tauri application");
}

fn setup(app: &mut tauri::App, paths: config::AppPaths) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;
    #[cfg(desktop)]
    use tauri_plugin_window_state::{StateFlags, WindowExt};

    #[cfg(desktop)]
    {
        let _span = tracing::info_span!("restore_window_state", window = "main").entered();

        tracing::info!("Restoring saved window position and size");

        if let Some(window) = app.get_webview_window("main") {
            match window.restore_state(StateFlags::SIZE | StateFlags::POSITION) {
                Ok(()) => tracing::info!(
                    "Window geometry initialized from saved state or configured defaults"
                ),
                Err(error) => {
                    tracing::warn!(%error, "Window state restoration failed; using available window geometry")
                }
            }
        } else {
            tracing::warn!("Main window unavailable for geometry restoration");
        }
    }

    let database_path = paths.database_path.clone();

    let cache_path = paths.cache_path.clone();

    let store = config::ConfigStore::load(paths).map_err(std::io::Error::other)?;

    let startup = store.snapshot().map_err(std::io::Error::other)?.settings;

    let database = tauri::async_runtime::block_on(database::Database::open(&database_path))?;
    app.manage(store);
    app.manage(database);
    app.manage(stage::StageState::new(cache_path));
    app.manage(legacy_import::ImportState::default());
    app.manage(disk_sync::SyncState::default());

    #[cfg(desktop)]
    {
        app.manage(updater::UpdateState::default());

        tracing::info!("Velopack updater initialized");
    }

    if startup.auto_scan {
        let mut roots = Vec::new();

        if let Some(root) = startup.library_path {
            roots.push((root, "library"));
        }

        if let Some(root) = startup.staging_path {
            roots.push((root, "stage"));
        }

        if !roots.is_empty() {
            stage::launch_sources(app.handle().clone(), roots).map_err(std::io::Error::other)?;
        }
    }
    disk_sync::start(app.handle().clone());

    tracing::info!("Application setup completed");
    Ok(())
}
