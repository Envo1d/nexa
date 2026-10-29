//! Native window shell for the session-only frontend demo.

use tauri::Manager;

use crate::stage;

pub(super) fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        mode = "demo",
        persistence = false,
        "Starting native demo without application storage or background services"
    );

    let result = tauri::Builder::default()
        .plugin(
            tauri::plugin::Builder::<tauri::Wry>::new("demo")
                .js_init_script("Object.defineProperty(window, '__NEXA_DEMO__', { value: true });")
                .build(),
        )
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            app.manage(stage::ViewerFullscreenState::default());
            tracing::info!("Native demo setup completed");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![stage::set_viewer_fullscreen])
        .run(tauri::generate_context!());

    match &result {
        Ok(()) => tracing::info!("Native demo stopped"),
        Err(error) => tracing::error!(%error, "Native demo failed"),
    }

    result.expect("error while running native demo");
}
