use serde::Serialize;
use sqlx::Row;

use crate::{config::ConfigStore, database::Database, disk_sync, replacement, stage::StageState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    total: usize,
    upper_id: String,
    scope: String,
    rejections: Vec<(String, String)>,
    generation: i64,
}

pub(super) async fn prepare(
    database: &Database,
    config: &ConfigStore,
) -> Result<Inventory, String> {
    disk_sync::recover(database.pool()).await?;
    replacement::recover(database.pool()).await?;

    let settings = config.snapshot()?.settings;

    let scope = serde_json::to_string(&(&settings.library_path, &settings.staging_path))
        .map_err(|error| error.to_string())?;

    let upper_id: i64 = sqlx::query_scalar("SELECT coalesce(max(id),0) FROM media")
        .fetch_one(database.pool())
        .await
        .map_err(|error| error.to_string())?;

    let mut cursor = 0_i64;

    let mut total = 0;

    loop {
        let rows = sqlx::query(concat!(
            "SELECT m.*,s.root_path,s.kind source_kind FROM media m JOIN sources s ON s.id=m.source_id ",
            "WHERE m.availability='present' AND m.id>? AND m.id<=? ORDER BY m.id LIMIT 500",
        ))
        .bind(cursor)
        .bind(upper_id)
        .fetch_all(database.pool())
        .await
        .map_err(|error| error.to_string())?;

        if rows.is_empty() {
            break;
        }

        for row in rows {
            cursor = row.get("id");

            if replacement::is_main(&row, &settings) || replacement::is_stage(&row, &settings) {
                total += 1;
            }
        }
    }

    let rejections = sqlx::query_as(
        "SELECT left_hash,right_hash FROM duplicate_rejections ORDER BY left_hash,right_hash",
    )
    .fetch_all(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    let generation = super::snapshots::begin(database.pool(), &scope, total).await?;

    Ok(Inventory {
        generation,
        total,
        upper_id: upper_id.to_string(),
        scope,
        rejections,
    })
}

#[tauri::command]
#[tracing::instrument(skip_all)]
pub async fn prepare_duplicate_scan(
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
    state: tauri::State<'_, StageState>,
) -> Result<Inventory, String> {
    let _guard = state
        .try_operation()
        .ok_or("Another media operation is running; retry when it finishes")?;

    tracing::info!("Duplicate scan inventory started");

    let result = prepare(&database, &config).await;

    match &result {
        Ok(inventory) => tracing::info!(
            total = inventory.total,
            "Duplicate scan inventory completed"
        ),
        Err(error) => tracing::error!(reason = %error, "Duplicate scan inventory failed"),
    }

    result
}
