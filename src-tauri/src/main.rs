// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Activation hooks must run before Tauri, single-instance handling or SQLite.
    velopack::VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .run();

    nexa_lib::run()
}
