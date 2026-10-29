// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if cfg!(debug_assertions) {
        nexa_lib::run();

        return;
    }

    let mut arguments = std::env::args_os().skip(1);

    if arguments.next().as_deref() == Some(std::ffi::OsStr::new("--nexa-update-ui")) {
        if let Some(session) = arguments.next() {
            nexa_lib::run_update_helper(session.into());
        }

        return;
    }

    // Activation hooks must run before Tauri, single-instance handling or SQLite.
    velopack::VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .run();

    nexa_lib::run()
}
