mod config;
pub mod database;
mod legacy_import;
mod logging;
pub mod media_metadata;
pub mod media_thumbnail;
pub mod preview_cache;
mod stage;
mod taxonomy;
#[cfg(windows)]
mod windows_com;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::Manager;
    let paths = config::AppPaths::resolve().expect("Could not resolve NexaNew storage");
    let log_guard = logging::init(&paths.logs_path).expect("Could not initialize NexaNew tracing");
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

    let result = builder
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .setup(move |app| {
            let _span = tracing::info_span!("application_setup").entered();
            let database_path = paths.database_path.clone();
            let cache_path = paths.cache_path.clone();
            let store = config::ConfigStore::load(paths).map_err(std::io::Error::other)?;
            let startup = store.snapshot().map_err(std::io::Error::other)?.settings;
            let database =
                tauri::async_runtime::block_on(database::Database::open(&database_path))?;
            app.manage(store);
            app.manage(database);
            app.manage(stage::StageState::new(cache_path));
            app.manage(legacy_import::ImportState::default());
            if startup.auto_scan
                && let Some(root) = startup.staging_path
            {
                stage::launch(app.handle().clone(), root).map_err(std::io::Error::other)?;
            }
            tracing::info!("Application setup completed");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::get_config,
            config::save_config,
            config::open_logs,
            legacy_import::find_legacy_database,
            legacy_import::import_legacy_database,
            taxonomy::get_taxonomy,
            taxonomy::rename_taxonomy,
            taxonomy::delete_taxonomy,
            taxonomy::get_redistribution_files,
            stage::start_stage_scan,
            stage::get_stage_scan_status,
            stage::get_stage_files,
            stage::get_stage_preview
        ])
        .build(tauri::generate_context!());
    let result = result.map(|app| {
        app.run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                tauri::async_runtime::block_on(app.state::<database::Database>().close());
            }
        });
    });
    match &result {
        Ok(()) => tracing::info!("Application stopped"),
        Err(error) => tracing::error!(%error, "Application failed"),
    }
    // Flush the worker explicitly before a possible panic in aborting release builds.
    drop(log_guard);
    result.expect("error while running tauri application");
}
