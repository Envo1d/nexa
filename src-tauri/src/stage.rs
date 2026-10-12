//! Stage state and indexing; Shell/filesystem work never runs in write transactions.

mod file_actions;
mod previews;
mod queries;
mod scanning;
pub(crate) use scanning::activate_source_for;
pub use scanning::{
    __cmd__get_stage_scan_status, __cmd__start_library_scan, __cmd__start_stage_scan,
    __tauri_command_name_get_stage_scan_status, __tauri_command_name_start_library_scan,
    __tauri_command_name_start_stage_scan, get_stage_scan_status, launch_sources,
    start_library_scan, start_stage_scan,
};

pub(crate) use file_actions::checked_action_path;
pub use file_actions::{
    __cmd__stage_file_action, __tauri_command_name_stage_file_action, stage_file_action,
};
pub use previews::{
    __cmd__get_stage_preview, __tauri_command_name_get_stage_preview, get_stage_preview,
};
pub(crate) use queries::stage_file;
pub use queries::{
    __cmd__get_stage_files, __tauri_command_name_get_stage_files, StageFile, get_stage_files,
};

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use crate::{media_metadata::MediaKind, preview_cache::PreviewCache};

const BATCH: usize = 64;

const EXTRACTOR_VERSION: i64 = 1;

pub(crate) struct FileActionGuard<'a>(&'a AtomicBool);

impl Drop for FileActionGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub const CACHE_MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStatus {
    bytes: u64,
    max_bytes: u64,
}

#[tauri::command]
pub async fn get_cache_usage(state: tauri::State<'_, StageState>) -> Result<CacheStatus, String> {
    let cache = state.cache.clone();
    tauri::async_runtime::spawn_blocking(move || {
        cache
            .usage()
            .map(|usage| CacheStatus {
                bytes: usage.bytes,
                max_bytes: CACHE_MAX_BYTES,
            })
            .map_err(|error| {
                tracing::warn!(%error, "Cache usage measurement failed");
                "Could not measure thumbnail cache".to_string()
            })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub running: bool,
    pub scanned: u64,
    pub extracted: u64,
    pub errors: u64,
    pub message: String,
}

pub struct StageState {
    scanning: AtomicBool,
    status: Mutex<ScanStatus>,
    pub cache: Arc<PreviewCache>,
    preview_slots: Arc<tokio::sync::Semaphore>,
    generated_previews: Arc<AtomicU64>,
}

impl StageState {
    pub(crate) fn try_operation(&self) -> Option<FileActionGuard<'_>> {
        self.scanning
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| FileActionGuard(&self.scanning))
    }

    pub(crate) async fn operation(&self) -> FileActionGuard<'_> {
        loop {
            if let Some(guard) = self.try_operation() {
                return guard;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    pub fn new(cache_path: PathBuf) -> Self {
        Self {
            scanning: AtomicBool::new(false),
            status: Mutex::new(ScanStatus::default()),
            cache: Arc::new(PreviewCache::new(cache_path)),
            preview_slots: Arc::new(tokio::sync::Semaphore::new(2)),
            generated_previews: Arc::new(AtomicU64::new(0)),
        }
    }
}

pub(crate) fn now() -> i64 {
    timestamp(SystemTime::now()).unwrap_or(0)
}

fn timestamp(time: SystemTime) -> Option<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(value) => i64::try_from(value.as_millis()).ok(),
        Err(error) => i64::try_from(error.duration().as_millis())
            .ok()
            .map(|value| -value),
    }
}

fn kind(extension: &str) -> Option<MediaKind> {
    match extension {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "avif" | "heic"
        | "heif" => Some(MediaKind::Image),
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpeg" | "mpg" => {
            Some(MediaKind::Video)
        }
        _ => None,
    }
}

#[derive(Clone)]
struct Discovered {
    path: PathBuf,
    relative: String,
    extension: String,
    kind: MediaKind,
    size: i64,
    modified: Option<i64>,
    created: Option<i64>,
    revision: String,
}

fn inspect(path: PathBuf, root: &Path) -> Result<Option<Discovered>, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let Some(kind) = kind(&extension) else {
        return Ok(None);
    };

    let info = std::fs::symlink_metadata(&path).map_err(|_| "Cannot inspect media file")?;

    if !info.file_type().is_file() {
        return Err("Media is no longer a regular file".into());
    }

    let modified = info.modified().map_err(|_| "Cannot read file revision")?;

    let relative = path
        .strip_prefix(root)
        .map_err(|_| "File escaped source root")?
        .to_str()
        .ok_or("Invalid Unicode path")?
        .to_owned();

    let revision = format!("{}:{modified:?}", info.len());
    Ok(Some(Discovered {
        relative,
        extension,
        kind,
        size: i64::try_from(info.len()).map_err(|_| "File size exceeds database range")?,
        modified: timestamp(modified),
        created: info.created().ok().and_then(timestamp),
        revision,
        path,
    }))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use queries::browse;
