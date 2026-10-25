//! Bounded content discovery and persistent, explicit visual-review decisions.
mod cache;
mod fingerprints;
mod inventory;
mod queries;
mod review;
mod snapshots;

pub use snapshots::{
    __cmd__load_duplicate_search, __cmd__save_duplicate_search,
    __tauri_command_name_load_duplicate_search, __tauri_command_name_save_duplicate_search,
    load_duplicate_search, save_duplicate_search,
};

pub use inventory::{
    __cmd__prepare_duplicate_scan, __tauri_command_name_prepare_duplicate_scan,
    prepare_duplicate_scan,
};
pub use review::{
    __cmd__set_duplicate_rejection, __tauri_command_name_set_duplicate_rejection,
    set_duplicate_rejection,
};

use std::path::PathBuf;

use serde::Serialize;
use sqlx::{AssertSqlSafe, Row};

use crate::{
    config::ConfigStore,
    database::Database,
    disk_sync, replacement,
    stage::{self, StageState},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    #[serde(flatten)]
    technical: stage::StageFile,
    main: bool,
    artist: String,
    copyrights: Vec<String>,
    characters: Vec<String>,
    tags: Vec<String>,
    video_title: Option<String>,
    sha256: String,
    visual: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    files: Vec<File>,
    after: String,
    more: bool,
    skipped: usize,
    visual_unavailable: usize,
    scope: String,
    processed: usize,
    reused: usize,
    hashed: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    completed: usize,
}

#[tauri::command]
#[tracing::instrument(skip_all)]
pub async fn fingerprint_duplicate_batch(
    after: String,
    scope: Option<String>,
    upper_id: Option<String>,
    on_progress: tauri::ipc::Channel<Progress>,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
    state: tauri::State<'_, StageState>,
) -> Result<Page, String> {
    let after = after.parse::<i64>().map_err(|_| "Invalid cursor")?;

    if after < 0 {
        return Err("Invalid cursor".into());
    }

    let _guard = state
        .try_operation()
        .ok_or("Another media operation is running; retry when it finishes")?;

    tracing::info!(after, "Duplicate fingerprint batch started");

    let upper_id = upper_id
        .map(|value| value.parse::<i64>().map_err(|_| "Invalid scan boundary"))
        .transpose()?;

    let result = batch(
        after,
        scope,
        &database,
        &config,
        upper_id,
        Some(on_progress),
    )
    .await;

    match &result {
        Ok(page) => tracing::info!(
            files = page.files.len(),
            skipped = page.skipped,
            visual_unavailable = page.visual_unavailable,
            more = page.more,
            reused = page.reused,
            hashed = page.hashed,
            "Duplicate fingerprint batch completed"
        ),
        Err(error) => tracing::error!(reason = %error, "Duplicate fingerprint batch failed"),
    }

    result
}

async fn batch(
    after: i64,
    scope: Option<String>,
    database: &Database,
    config: &ConfigStore,
    upper_id: Option<i64>,
    on_progress: Option<tauri::ipc::Channel<Progress>>,
) -> Result<Page, String> {
    let settings = config.snapshot()?.settings;

    let current_scope = serde_json::to_string(&(&settings.library_path, &settings.staging_path))
        .map_err(|error| error.to_string())?;

    if scope.is_some_and(|scope| scope != current_scope) {
        return Err("Configured folders changed; restart duplicate search".into());
    }

    disk_sync::recover(database.pool()).await?;
    replacement::recover(database.pool()).await?;

    let rows = sqlx::query(AssertSqlSafe(format!(
        "{} WHERE m.availability='present' AND m.id>? AND m.id<=? ORDER BY m.id LIMIT 16",
        queries::SELECT
    )))
    .bind(after)
    .bind(upper_id.unwrap_or(i64::MAX))
    .fetch_all(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    let more = rows.len() == 16;

    let cursor = rows.last().map_or(after, |row| row.get("id"));

    let mut work = Vec::new();

    for row in rows {
        let main = replacement::is_main(&row, &settings);

        if !main && !replacement::is_stage(&row, &settings) {
            continue;
        }

        let root = PathBuf::from(row.get::<String, _>("root_path"));

        let relative: String = row.get("relative_path");

        let revision: Option<String> = row.get("file_revision");

        let kind: String = row.get("kind");

        let id = row.get::<i64, _>("id");

        let cached = cache::get(database.pool(), id).await?;

        let file = queries::file(row, main)?;

        work.push((file, root, relative, revision, kind, id, cached));
    }

    let (page, fingerprints, invalid) = tauri::async_runtime::spawn_blocking(move || {
        let mut page = Page {
            files: Vec::new(),
            after: cursor.to_string(),
            more,
            skipped: 0,
            visual_unavailable: 0,
            scope: current_scope,
            processed: 0,
            reused: 0,
            hashed: 0,
        };

        let mut fingerprints = Vec::new();

        let mut invalid = Vec::new();

        for (mut file, root, relative, revision, kind, id, cached) in work {
            match cache::resolve(id, &root, &relative, revision.as_deref(), &kind, cached) {
                Ok((fingerprint, reused)) => {
                    if kind == "image" && fingerprint.visual.is_none() {
                        page.visual_unavailable += 1;
                    }

                    file.sha256 = fingerprint.sha256.clone();
                    file.technical.revision = Some(fingerprint.revision.clone());
                    file.visual = fingerprint.visual.clone();
                    page.files.push(file);
                    page.reused += usize::from(reused);
                    page.hashed += usize::from(!reused);
                    fingerprints.push(fingerprint);
                }
                Err(_) => {
                    page.skipped += 1;
                    invalid.push(id);
                }
            }

            page.processed += 1;

            if let Some(channel) = &on_progress {
                let _ = channel.send(Progress {
                    completed: page.processed,
                });
            }
        }

        if page.skipped > 0 || page.visual_unavailable > 0 {
            tracing::warn!(
                skipped = page.skipped,
                visual_unavailable = page.visual_unavailable,
                "Some duplicate fingerprints unavailable; rescan or check file access"
            );
        }

        (page, fingerprints, invalid)
    })
    .await
    .map_err(|error| error.to_string())?;

    cache::persist(database.pool(), &fingerprints, &invalid).await?;

    Ok(page)
}

#[cfg(test)]
mod tests;
