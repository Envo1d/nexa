//! Catalog diagnostics and explicit repairs; original media is never moved or deleted.
mod queries;
mod relinking;
mod verification;

use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use serde::Serialize;
use tauri::Emitter;

use crate::{config::ConfigStore, database::Database, disk_sync, stage};

#[derive(Clone, Serialize)]
pub(crate) struct PreviewFailure {
    id: i64,
    revision: Option<String>,
    path: String,
    reason: Option<String>,
    attempt: u64,
}

#[derive(Default)]
pub(crate) struct HealthState {
    next: AtomicU64,
    previews: Mutex<BTreeMap<i64, PreviewFailure>>,
    unavailable_roots: Mutex<Vec<String>>,
}

impl HealthState {
    pub(crate) fn begin_preview(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) fn finish_preview(
        &self,
        id: i64,
        revision: Option<String>,
        path: String,
        attempt: u64,
        reason: Option<String>,
    ) {
        let Ok(mut entries) = self.previews.lock() else {
            return;
        };

        if entries
            .get(&id)
            .is_some_and(|current| current.attempt > attempt)
        {
            return;
        }

        entries.insert(
            id,
            PreviewFailure {
                id,
                revision,
                path,
                attempt,
                reason,
            },
        );

        while entries.len() > 1024 {
            if let Some(oldest) = entries
                .values()
                .min_by_key(|entry| entry.attempt)
                .map(|entry| entry.id)
            {
                entries.remove(&oldest);
            }
        }
    }

    fn failures(&self) -> Result<String, String> {
        let entries = self
            .previews
            .lock()
            .map_err(|_| "Preview diagnostics unavailable")?;
        let roots = self
            .unavailable_roots
            .lock()
            .map_err(|_| "Source diagnostics unavailable")?;
        serde_json::to_string(&serde_json::json!({
            "previews": entries
                .values()
                .filter(|entry| entry.reason.is_some())
                .collect::<Vec<_>>(),
            "roots": *roots,
        }))
        .map_err(|error| error.to_string())
    }
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn get_library_health(
    category: String,
    offset: i64,
    database: tauri::State<'_, Database>,
    health: tauri::State<'_, HealthState>,
    sync: tauri::State<'_, disk_sync::SyncState>,
) -> Result<queries::HealthPage, String> {
    let sync_error = disk_sync::get_disk_sync_status(sync)?.error;
    queries::browse(
        database.pool(),
        &category,
        offset,
        &health.failures()?,
        sync_error,
    )
    .await
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn check_library_health(
    app: tauri::AppHandle,
    database: tauri::State<'_, Database>,
    media: tauri::State<'_, stage::StageState>,
    health: tauri::State<'_, HealthState>,
) -> Result<verification::Verification, String> {
    let _guard = media
        .try_operation()
        .ok_or("Wait for the current library operation to finish.")?;
    tracing::info!("Library health file verification started");
    let result = verification::verify(database.pool()).await?;
    *health
        .unavailable_roots
        .lock()
        .map_err(|_| "Source diagnostics unavailable")? = result.roots.iter().cloned().collect();
    tracing::info!(
        checked = result.checked,
        missing = result.missing,
        unavailable = result.unavailable,
        "Library health file verification completed"
    );
    let _ = app.emit("library-health-changed", ());
    Ok(result)
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn get_health_media(
    id: String,
    database: tauri::State<'_, Database>,
) -> Result<queries::HealthMedia, String> {
    queries::media(database.pool(), parse_id(&id)?).await
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn retry_health_metadata(
    app: tauri::AppHandle,
    id: String,
    database: tauri::State<'_, Database>,
    media: tauri::State<'_, stage::StageState>,
) -> Result<(), String> {
    let id = parse_id(&id)?;
    let _guard = media
        .try_operation()
        .ok_or("Wait for the current library operation to finish.")?;
    tracing::info!(media_id = id, "Technical metadata retry started");
    crate::disk_sync::recover(database.pool()).await?;
    let result = stage::refresh_technical_metadata(database.pool(), id).await;
    let _ = app.emit("library-health-changed", ());
    result?;
    tracing::info!(media_id = id, "Technical metadata retry completed");
    crate::disk_sync::request(&app);
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn inspect_health_relink(
    id: String,
    path: std::path::PathBuf,
    database: tauri::State<'_, Database>,
    media: tauri::State<'_, stage::StageState>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<relinking::Proposal, String> {
    let _guard = media
        .try_operation()
        .ok_or("Wait for the current library operation to finish.")?;
    tracing::info!("Missing file relink inspection started");
    relinking::inspect(
        database.pool(),
        parse_id(&id)?,
        path,
        &config.snapshot()?.settings,
    )
    .await
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn confirm_health_relink(
    app: tauri::AppHandle,
    id: String,
    path: std::path::PathBuf,
    token: String,
    database: tauri::State<'_, Database>,
    media: tauri::State<'_, stage::StageState>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<(), String> {
    let id = parse_id(&id)?;
    let _guard = media
        .try_operation()
        .ok_or("Wait for the current library operation to finish.")?;
    tracing::info!(media_id = id, "Missing file relink started");
    relinking::confirm(
        database.pool(),
        id,
        path,
        &token,
        &config.snapshot()?.settings,
    )
    .await?;
    tracing::info!(
        media_id = id,
        "Missing file relink completed; catalog identity preserved"
    );
    disk_sync::request(&app);
    let _ = app.emit("library-health-changed", ());
    Ok(())
}

fn parse_id(value: &str) -> Result<i64, String> {
    value.parse::<i64>().map_err(|_| "Invalid media ID".into())
}

#[cfg(test)]
mod tests;
