mod catalog;
mod dialogs;
mod media;
mod previews;
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
use gpui_component::{Root, Theme, ThemeMode};

fn main() {
    // SAFETY: this is the first action in main, before GPUI, logging, or any
    // application threads are started. This changes only this process's setting.
    // GPUI 0.2.2 otherwise places its opaque composition tree above child HWNDs.
    unsafe { std::env::set_var("GPUI_DISABLE_DIRECT_COMPOSITION", "1") };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,nexa_gpui_prototype=info".into()),
        )
        .init();

    let mut arguments = std::env::args_os().skip(1);
    let dll = match (arguments.next(), arguments.next(), arguments.next()) {
        (None, None, None) => None,
        (Some(flag), Some(path), None) if flag == "--mpv-dll" => Some(PathBuf::from(path)),
        _ => {
            eprintln!("Usage: nexa-gpui-prototype [--mpv-dll <absolute mpv DLL path>]");
            std::process::exit(2);
        }
    };

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "Starting isolated GPUI prototype"
    );
    tracing::info!(
        renderer = "hwnd_swapchain",
        "Using GPUI's native-child-compatible DirectX presentation path"
    );
    Application::new()
        .with_assets(ui::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            ui::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            let theme = Theme::global_mut(cx);
            theme.font_family = "Segoe UI".into();
            theme.background = gpui::rgb(0x1a1827).into();
            theme.foreground = gpui::rgb(0xeeedf6).into();
            theme.border = gpui::rgb(0x252434).into();
            theme.input = gpui::rgb(0x252434).into();
            theme.ring = gpui::rgb(0xa786ff).into();
            theme.muted_foreground = gpui::rgb(0xa39bb5).into();
            theme.popover = gpui::rgb(0x1a1827).into();
            theme.primary = gpui::rgb(0x7b43d9).into();
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
                    let view = cx.new(|cx| ui::Gallery::new(dll, window, cx));
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
