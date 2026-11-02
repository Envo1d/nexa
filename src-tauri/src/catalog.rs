//! Explicit catalog snapshots; original media and preferences are never restored.
mod portable;
mod snapshots;

use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{config::ConfigStore, database::Database, stage::StageState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    digest: String,
    created_at: u64,
    media_count: usize,
    roots: Vec<String>,
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub async fn inspect_catalog_backup(path: PathBuf) -> Result<BackupPreview, String> {
    tracing::info!("Inspecting catalog backup");
    let (backup, digest) = snapshots::read(path).await?;
    backup.validate()?;

    Ok(BackupPreview {
        digest,
        created_at: backup.created_at,
        media_count: backup.tables["media"].len(),
        roots: backup.tables["sources"]
            .iter()
            .filter_map(|row| row["root_path"].as_str().map(str::to_owned))
            .collect(),
    })
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(portable), err)]
pub async fn export_catalog(
    path: PathBuf,
    portable: bool,
    database: tauri::State<'_, Database>,
    media: tauri::State<'_, StageState>,
) -> Result<(), String> {
    let _guard = media
        .try_operation()
        .ok_or("Wait for the current library operation to finish.")?;

    tracing::info!("Catalog export started");
    let snapshot = snapshots::capture(database.pool()).await?;
    let bytes = if portable {
        serde_json::to_vec_pretty(&portable::export(&snapshot)?)
    } else {
        serde_json::to_vec_pretty(&snapshot)
    }
    .map_err(|error| error.to_string())?;

    snapshots::write(path, bytes).await?;
    tracing::info!("Catalog export completed");
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub async fn restore_catalog(
    path: PathBuf,
    digest: String,
    database: tauri::State<'_, Database>,
    media: tauri::State<'_, StageState>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<String, String> {
    let _guard = media
        .try_operation()
        .ok_or("Wait for the current library operation to finish.")?;

    tracing::info!("Catalog restore started");
    let settings = config.snapshot()?.settings;

    if settings.sync_structure || settings.auto_scan {
        return Err(
            "Disable and save disk synchronization and automatic scanning before restoring.".into(),
        );
    }

    let (backup, observed) = snapshots::read(path).await?;

    if digest != observed {
        return Err("The backup changed. Select it again before restoring.".into());
    }

    backup.validate()?;
    snapshots::check_roots(&backup, &settings)?;

    let current = snapshots::capture(database.pool()).await?;
    let parent = config
        .paths
        .database_path
        .parent()
        .ok_or("Invalid database directory")?;
    let safety_root = parent.join("catalog-backups");
    tokio::fs::create_dir_all(&safety_root)
        .await
        .map_err(|error| error.to_string())?;

    let bytes = serde_json::to_vec_pretty(&current).map_err(|error| error.to_string())?;
    let suffix = digest_hex(&bytes);
    let safety = safety_root.join(format!(
        "before-restore-{}-{}.json",
        current.created_at,
        &suffix[..16]
    ));
    snapshots::write(safety.clone(), bytes).await?;

    tracing::info!("Safety catalog backup saved; applying restore");
    snapshots::restore(database.pool(), &backup, &current).await?;
    tracing::info!(
        media_count = backup.tables["media"].len(),
        "Catalog restore completed"
    );
    Ok(safety.to_string_lossy().into_owned())
}

fn safe_relative(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\0')
        && Path::new(value)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

fn digest_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests;
