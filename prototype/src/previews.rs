//! Prepare bounded Shell previews on a fresh thread, independent of GPUI's MTA pool.

use std::{collections::HashMap, path::PathBuf, sync::Arc, thread};

use gpui::RenderImage;

use crate::catalog::{self, Entry};

pub(super) struct Prepared {
    pub entries: Vec<Entry>,
    pub previews: HashMap<usize, Arc<RenderImage>>,
}

/// Call from a background task: joining the dedicated thread must not block UI.
pub(super) fn prepare(paths: Vec<PathBuf>) -> Result<Prepared, String> {
    let worker = thread::Builder::new()
        .name("nexa-preview-sta".into())
        .spawn(move || {
            let entries: Vec<_> = paths
                .into_iter()
                .take(catalog::MAX_OPEN_FILES)
                .filter_map(catalog::entry)
                .collect();
            let mut previews = HashMap::new();

            // The existing provider owns balanced STA initialization and error logs.
            // A fresh thread avoids RPC_E_CHANGED_MODE from GPUI's MTA workers.
            for (index, entry) in entries.iter().enumerate() {
                if let Ok(image) = crate::media_thumbnail::get_thumbnail(&entry.path, 512) {
                    let mut image = image::DynamicImage::ImageRgb8(image).into_rgba8();

                    // Shell output is opaque; GPUI expects premultiplied BGRA.
                    for pixel in image.pixels_mut() {
                        pixel.0.swap(0, 2);
                    }

                    previews.insert(
                        index,
                        Arc::new(RenderImage::new(vec![image::Frame::new(image)])),
                    );
                }
            }

            Prepared { entries, previews }
        })
        .map_err(|_| "Could not start the native preview worker.".to_owned())?;

    worker
        .join()
        .map_err(|_| "The native preview worker stopped unexpectedly.".to_owned())
}

#[cfg(test)]
mod tests;
