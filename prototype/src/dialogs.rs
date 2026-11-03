//! Sequence native modal pickers outside fullscreen without holding UI borrows.

use std::{
    future::Future,
    path::PathBuf,
    time::{Duration, Instant},
};

use gpui::{App, PathPromptOptions, Window};

pub(super) fn open(
    window: &Window,
    cx: &mut App,
    options: PathPromptOptions,
    operation: &'static str,
) -> impl Future<Output = Result<Option<Vec<PathBuf>>, String>> + 'static {
    let handle = window.window_handle();
    let restore_fullscreen = window.is_fullscreen();
    let mut cx = cx.to_async();

    if restore_fullscreen {
        tracing::info!(operation, "Leaving fullscreen before opening native dialog");
        window.toggle_fullscreen();
    }

    async move {
        // GPUI's Windows fullscreen operation is queued on its foreground executor.
        // Yield first so that operation and its synchronous resize messages finish.
        let deadline = Instant::now() + Duration::from_secs(3);
        let ready = loop {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            match handle.update(&mut cx, |_, window, _| window.is_fullscreen()) {
                Ok(false) => break Ok(()),
                Ok(true) if Instant::now() < deadline => continue,
                Ok(true) => {
                    break Err("Could not leave fullscreen before opening the dialog.".to_owned());
                }
                Err(_) => break Err("The window closed before the dialog could open.".to_owned()),
            }
        };

        let result = match ready {
            Ok(()) => {
                tracing::info!(operation, "Opening native file dialog");
                match cx.update(|cx| cx.prompt_for_paths(options)) {
                    Ok(prompt) => match prompt.await {
                        Ok(Ok(paths)) => {
                            tracing::info!(
                                operation,
                                selected = paths.as_ref().map_or(0, Vec::len),
                                "Native file dialog returned"
                            );
                            Ok(paths)
                        }
                        Ok(Err(_)) | Err(_) => Err("The native file dialog failed.".to_owned()),
                    },
                    Err(_) => {
                        Err("The application closed before the dialog could open.".to_owned())
                    }
                }
            }
            Err(error) => Err(error),
        };

        if restore_fullscreen {
            let restored = handle.update(&mut cx, |_, window, _| {
                if !window.is_fullscreen() {
                    window.toggle_fullscreen();
                }
            });
            if restored.is_ok() {
                let deadline = Instant::now() + Duration::from_secs(3);
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    match handle.update(&mut cx, |_, window, _| window.is_fullscreen()) {
                        Ok(true) => {
                            tracing::info!(operation, "Fullscreen restored after native dialog");
                            break;
                        }
                        Ok(false) if Instant::now() < deadline => continue,
                        Ok(false) => {
                            // Keep a successful selection even when restoration fails.
                            tracing::warn!(
                                operation,
                                "Fullscreen restoration timed out; retaining dialog result"
                            );
                            break;
                        }
                        Err(_) => break,
                    }
                }
            } else {
                tracing::debug!(
                    operation,
                    "Skipped fullscreen restoration for closed window"
                );
            }
        }

        result
    }
}
