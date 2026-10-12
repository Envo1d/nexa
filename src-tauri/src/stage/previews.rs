//! stage: previews responsibility.

use std::{path::Path, sync::atomic::Ordering};

use sqlx::Row;
use tauri::Emitter;

use super::{CACHE_MAX_BYTES, CacheStatus, StageState, inspect};
use crate::{database::Database, preview_cache::PreviewOrigin};

#[tauri::command]
pub async fn get_stage_preview(
    app: tauri::AppHandle,
    id: String,
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, StageState>,
) -> Result<tauri::ipc::Response, String> {
    let id = id.parse::<i64>().map_err(|_| "Invalid media ID")?;

    let permit = state
        .preview_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| "Preview workers busy")?;

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

    let path =
        Path::new(&row.get::<String, _>("root_path")).join(row.get::<String, _>("relative_path"));

    let cache = state.cache.clone();

    let generated = state.generated_previews.clone();

    let expected: Option<String> = row.get("file_revision");

    let bytes = tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;

        if let Some(expected) = expected {
            let current = inspect(path.clone(), path.parent().ok_or("Invalid media path")?)?;

            if current.is_none_or(|current| current.revision != expected) {
                return Err("File changed; rescan its source".into());
            }
        }

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
    .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}
