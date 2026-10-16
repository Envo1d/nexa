//! stage: previews responsibility.

use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

use serde::Serialize;
use sqlx::Row;
use tauri::Emitter;

use super::{CACHE_MAX_BYTES, CacheStatus, StageState, inspect};
use crate::{database::Database, media_paths::checked_action_path, preview_cache::PreviewOrigin};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheClearResult {
    usage: CacheStatus,
    failed_removals: u64,
}

#[tauri::command]
#[tracing::instrument(skip_all)]
pub async fn clear_thumbnail_cache(
    state: tauri::State<'_, StageState>,
) -> Result<CacheClearResult, String> {
    tracing::info!("Thumbnail cache cleanup requested");

    // Drain both bounded preview workers before maintenance; new previews resume afterward.
    let permit = state
        .preview_slots
        .clone()
        .acquire_many_owned(2)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Thumbnail cache cleanup could not acquire workers");
            "Could not start thumbnail cache cleanup".to_string()
        })?;

    let cache = state.cache.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;

        // PreviewCache owns maintenance failure logging and strict filename confinement.
        let report = cache
            .clear()
            .map_err(|_| "Could not clear thumbnail cache".to_string())?;

        tracing::info!(
            removed_entries = report.removed.entries,
            remaining_bytes = report.remaining.bytes,
            failed_removals = report.failed_removals,
            "Thumbnail cache cleanup finished"
        );

        Ok(CacheClearResult {
            usage: CacheStatus {
                bytes: report.remaining.bytes,
                max_bytes: CACHE_MAX_BYTES,
            },
            failed_removals: report.failed_removals,
        })
    })
    .await
    .map_err(|error| {
        tracing::error!(%error, "Thumbnail cache cleanup worker failed");
        "Thumbnail cache cleanup worker failed".to_string()
    })?
}

pub(super) fn checked_preview_path(
    root: &Path,
    relative: &str,
    expected: Option<&str>,
) -> Result<PathBuf, String> {
    let path = checked_action_path(root, relative)?;

    if let Some(expected) = expected {
        let current = inspect(path.clone(), root)?;

        if current.is_none_or(|current| current.revision != expected) {
            return Err("File changed; rescan its source".into());
        }
    }

    Ok(path)
}

pub(super) async fn preview_permit(
    state: &StageState,
) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
    tracing::debug!("Waiting for preview worker availability");

    state
        .preview_slots
        .clone()
        .acquire_owned()
        .await
        .map_err(|error| {
            tracing::error!(%error, "Preview worker pool unavailable");
            "Preview workers unavailable".to_string()
        })
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn get_stage_preview(
    app: tauri::AppHandle,
    id: String,
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, StageState>,
) -> Result<tauri::ipc::Response, String> {
    let id = id.parse::<i64>().map_err(|_| "Invalid media ID")?;

    // Maintenance temporarily owns both permits; contention is not a failed preview.
    let permit = preview_permit(&state).await?;

    let row = sqlx::query(concat!(
        "SELECT s.root_path,m.relative_path,m.file_revision FROM media m JOIN sources s ON ",
        "s.id=m.source_id WHERE m.id=? AND m.availability='present' AND (s.kind IN ('stage',",
        "'library') OR m.needs_distribution=1)",
    ))
    .bind(id)
    .fetch_optional(database.pool())
    .await
    .map_err(|e| e.to_string())?
    .ok_or("Media unavailable")?;

    let root = PathBuf::from(row.get::<String, _>("root_path"));

    let relative: String = row.get("relative_path");

    let cache = state.cache.clone();

    let generated = state.generated_previews.clone();

    let expected: Option<String> = row.get("file_revision");

    let bytes = tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;

        tracing::debug!(media_id = id, "Preview source validation started");

        let path = checked_preview_path(&root, &relative, expected.as_deref()).map_err(|error| {
            tracing::warn!(media_id = id, reason = %error, "Preview source rejected; rescan may be required");
            error
        })?;

        let preview = cache
            .load_or_generate(&path, 512)
            .map_err(|_| "Preview unavailable")?;
        // Periodic oldest-write eviction bounds disk growth without inventory per card.
        if preview.origin == PreviewOrigin::Generated
            && generated.fetch_add(1, Ordering::Relaxed).is_multiple_of(64)
        {
            match cache.prune(CACHE_MAX_BYTES) {
                Ok(report) => {
                    let bytes = report.remaining.bytes.saturating_add(report.removed.bytes);

                    if bytes >= CACHE_MAX_BYTES {
                        tracing::info!(
                            bytes,
                            max_bytes = CACHE_MAX_BYTES,
                            "Preview cache budget reached"
                        );

                        if let Err(error) = app.emit(
                            "cache-limit-reached",
                            CacheStatus {
                                bytes,
                                max_bytes: CACHE_MAX_BYTES,
                            },
                        ) {
                            tracing::warn!(%error, "Cache limit notification unavailable");
                        }
                    }
                }
                Err(_) => tracing::warn!("Preview cache budget maintenance unavailable"),
            }
        }

        let bytes = crate::preview_cache::encode_preview(&preview.image)
            .map_err(|_| "Cannot encode preview")?;
        Ok::<_, String>(bytes.to_vec())
    })
    .await
    .map_err(|error| {
        tracing::error!(media_id = id, %error, "Preview worker failed");
        error.to_string()
    })??;

    tracing::debug!(
        media_id = id,
        encoded_bytes = bytes.len(),
        "Preview request completed"
    );

    Ok(tauri::ipc::Response::new(bytes))
}
