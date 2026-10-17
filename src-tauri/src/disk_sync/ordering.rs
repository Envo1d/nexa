//! disk_sync: ordering responsibility.

use std::path::Path;

use serde::Serialize;
use sqlx::SqlitePool;

use super::planning::{Target, grouped_targets};
use super::{recover, request};
use crate::{
    config::{AppConfig, ConfigStore},
    database::Database,
    stage::StageState,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderFile {
    pub(super) id: String,
    pub(super) filename: String,
    pub(super) kind: String,
    pub(super) revision: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderGroup {
    pub(super) key: String,
    pub(super) files: Vec<OrderFile>,
}

pub(super) async fn find_order_group(
    pool: &SqlitePool,
    root: &str,
    settings: &AppConfig,
    id: i64,
) -> Result<(String, Vec<Target>), String> {
    grouped_targets(pool, root, settings)
        .await?
        .into_iter()
        .find(|(_, group)| group.iter().any(|file| file.id == id))
        .ok_or_else(|| "File no longer belongs to a managed Library group".into())
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub async fn get_duplicate_order(
    id: String,
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, StageState>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<OrderGroup, String> {
    let id = id.parse::<i64>().map_err(|_| "Invalid media ID")?;

    let _guard = state.operation().await;
    recover(database.pool()).await?;

    let settings = config.snapshot()?.settings;

    let root = settings
        .library_path
        .as_ref()
        .and_then(|p| p.to_str())
        .ok_or("Library is not configured")?;

    let (key, group) = find_order_group(database.pool(), root, &settings, id).await?;

    tracing::debug!(files = group.len(), "Duplicate order group loaded");

    let files = group
        .into_iter()
        .map(|target| {
            Ok(OrderFile {
                id: target.id.to_string(),
                filename: Path::new(&target.current_relative)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or("Invalid media filename")?
                    .to_owned(),
                kind: target.kind,
                revision: target.revision,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(OrderGroup { key, files })
}

pub(super) async fn persist_order(
    pool: &SqlitePool,
    root: &str,
    settings: &AppConfig,
    key: &str,
    expected_ids: &[String],
    ids: &[String],
) -> Result<(), String> {
    let anchor = expected_ids
        .first()
        .ok_or("Empty order group")?
        .parse::<i64>()
        .map_err(|_| "Invalid media ID")?;

    let (current_key, group) = find_order_group(pool, root, settings, anchor).await?;

    let current: Vec<String> = group.iter().map(|file| file.id.to_string()).collect();

    let members: std::collections::HashSet<&String> = ids.iter().collect();

    if current_key != key || current != expected_ids {
        return Err("The group changed. Reload it before saving the order.".into());
    }

    if ids.len() != current.len()
        || members.len() != ids.len()
        || !current.iter().all(|id| members.contains(id))
    {
        return Err("Order must contain every group member exactly once".into());
    }

    let mut transaction = pool.begin().await.map_err(|e| e.to_string())?;

    for (position, id) in ids.iter().enumerate() {
        sqlx::query(concat!(
            "UPDATE media SET naming_order_key=?,naming_order=?,",
            "updated_at=CAST(unixepoch('subsec')*1000 AS INTEGER) WHERE id=?",
        ))
        .bind(key)
        .bind(position as i64)
        .bind(id)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    }
    transaction.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(files = ids.len()), err)]
pub async fn save_duplicate_order(
    app: tauri::AppHandle,
    key: String,
    expected_ids: Vec<String>,
    ids: Vec<String>,
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, StageState>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<(), String> {
    tracing::info!("Duplicate order save started");

    let _guard = state.operation().await;
    recover(database.pool()).await?;

    let settings = config.snapshot()?.settings;

    let root = settings
        .library_path
        .as_ref()
        .and_then(|p| p.to_str())
        .ok_or("Library is not configured")?;
    persist_order(database.pool(), root, &settings, &key, &expected_ids, &ids).await?;
    if expected_ids != ids {
        request(&app);
    }

    tracing::info!(order_changed = expected_ids != ids, "Duplicate order saved");
    Ok(())
}
