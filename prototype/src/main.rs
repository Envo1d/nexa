mod appearance;
mod catalog;
mod dialogs;
mod i18n;
mod logging;
mod previews;
mod taxonomy;
mod ui;

#[path = "../../src-tauri/src/naming.rs"]
mod naming;

#[path = "../../src-tauri/src/media_thumbnail.rs"]
pub mod media_thumbnail;
#[cfg(windows)]
#[path = "../../src-tauri/src/windows_com.rs"]
mod windows_com;

use std::path::PathBuf;

use gpui::{App, AppContext, Application, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_component::Root;

fn main() {
    logging::init();

    let mut arguments = std::env::args_os().skip(1);
    let preview_paths: Vec<PathBuf> = match arguments.next() {
        None => Vec::new(),
        Some(flag) if flag == "--preview" => arguments
            .take(catalog::MAX_OPEN_FILES)
            .map(PathBuf::from)
            .collect(),
        Some(_) => {
            eprintln!("Usage: nexa-gpui-prototype [--preview <local media files...>]");
            std::process::exit(2);
        }
    };

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "Starting isolated GPUI prototype"
    );
    Application::new()
        .with_assets(ui::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            ui::init(cx);
            appearance::apply(0, cx);
            i18n::apply(0);
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let result = cx.open_window(
                WindowOptions {
                    window_min_size: Some(size(px(960.0), px(600.0))),
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1680.0), px(1000.0)),
                        cx,
                    ))),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Nexa · GPUI prototype".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| {
                        let mut gallery = ui::Gallery::new(window, cx);
                        gallery.preview_local_files(preview_paths, window, cx);
                        gallery
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            );
            if let Err(error) = result {
                tracing::error!(%error, "Could not open prototype window");
                cx.quit();
            }
            cx.activate(true);
        });
    tracing::info!("GPUI prototype stopped");
}
