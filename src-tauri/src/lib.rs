mod bootstrap;
mod config;
pub mod database;
mod disk_sync;
mod legacy_import;
mod library;
mod logging;
pub mod media_metadata;
mod media_paths;
pub mod media_thumbnail;
mod metadata_presets;
mod naming;
pub mod preview_cache;
mod replacement;
mod share;
mod stage;
mod taxonomy;

#[cfg(desktop)]
mod updater;

#[cfg(windows)]
mod windows_com;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    bootstrap::run();
}

#[cfg(desktop)]
pub fn run_update_helper(session: std::path::PathBuf) {
    updater::run_helper(session);
}
