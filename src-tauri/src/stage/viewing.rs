//! Original-media authorization and annotations for Main and Stage viewing.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::Serialize;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use tauri::Manager;

use super::{StageState, file_actions::action_file, inspect};
use crate::{
    config::ConfigStore, database::Database, library::write_tags, media_paths::checked_action_path,
};

#[derive(Default)]
pub struct ViewerFullscreenState(Mutex<Option<FullscreenSession>>);

struct FullscreenSession {
    owner: String,
    previous: bool,
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(enabled), err)]
pub fn set_viewer_fullscreen(
    window: tauri::WebviewWindow,
    session: String,
    enabled: bool,
    state: tauri::State<'_, ViewerFullscreenState>,
) -> Result<bool, String> {
    if window.label() != "main" || session.is_empty() || session.len() > 64 {
        return Err("Invalid viewer fullscreen request".into());
    }

    let mut lease = state
        .0
        .lock()
        .map_err(|_| "Fullscreen state is unavailable")?;

    if enabled {
        if let Some(active) = lease.as_ref() {
            if active.owner == session {
                return Ok(true);
            }

            return Err("Another viewer fullscreen transition is active".into());
        }

        let previous = window
            .is_fullscreen()
            .map_err(|_| "Cannot read window fullscreen state")?;

        tracing::info!(previous, "Entering native viewer fullscreen");
        window
            .set_fullscreen(true)
            .map_err(|_| "Cannot enter native fullscreen")?;
        *lease = Some(FullscreenSession {
            owner: session,
            previous,
        });
        tracing::info!("Native viewer fullscreen entered");
        Ok(true)
    } else {
        let Some(active) = lease.as_ref().filter(|active| active.owner == session) else {
            return Ok(false);
        };

        tracing::info!(
            previous = active.previous,
            "Restoring window after viewer fullscreen"
        );
        window
            .set_fullscreen(active.previous)
            .map_err(|_| "Cannot restore window fullscreen state")?;
        *lease = None;
        tracing::info!("Viewer fullscreen window state restored");
        Ok(false)
    }
}

#[derive(Serialize)]
pub struct ViewerSource {
    path: String,
    favorite: bool,
    tags: Vec<String>,
    fps: Option<f64>,
}

#[derive(Serialize)]
pub struct ViewerEntry {
    id: String,
    filename: String,
    kind: String,
    extension: String,
}

impl ViewerEntry {
    pub(crate) fn from_row(row: sqlx::sqlite::SqliteRow) -> Self {
        Self {
            id: row.get::<i64, _>("id").to_string(),
            filename: Path::new(&row.get::<String, _>("relative_path"))
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            kind: row.get("kind"),
            extension: row.get("extension"),
        }
    }
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(library), err)]
pub async fn get_viewer_sequence(
    library: bool,
    filters: crate::library::Filters,
    search: String,
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, StageState>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<Vec<ViewerEntry>, String> {
    let _guard = state.operation().await;
    crate::disk_sync::recover(database.pool()).await?;

    tracing::debug!("Capturing viewer order");

    let entries = if library {
        crate::library::viewer_sequence(database.pool(), &config, filters).await?
    } else {
        let root = config.snapshot()?.settings.staging_path;

        let root = root.map(|path| path.to_string_lossy().into_owned());

        capture_stage_sequence(database.pool(), root, &search, &filters.sort).await?
    };

    tracing::debug!(count = entries.len(), "Viewer order captured");
    Ok(entries)
}

pub(super) async fn capture_stage_sequence(
    pool: &SqlitePool,
    root: Option<String>,
    search: &str,
    sort: &str,
) -> Result<Vec<ViewerEntry>, String> {
    let order = super::queries::sort_order(sort)?;

    let mut query = QueryBuilder::<Sqlite>::new(concat!(
        "SELECT m.id,m.relative_path,m.kind,m.extension FROM media m ",
        "JOIN sources s ON s.id=m.source_id WHERE ((s.kind='stage' AND s.path_key=? ",
        "AND m.availability='present' AND m.library_assignment IS NULL) OR ",
        "m.needs_distribution=1)",
    ));

    query
        .push(super::queries::FILENAME_SEARCH)
        .push(super::queries::FILENAME_RANK)
        .push(order);

    let search = crate::naming::name_key(&search.trim().replace('\\', "/"));

    let entries = query
        .build()
        .bind(root)
        .bind(&search)
        .bind(&search)
        .fetch_all(pool)
        .await
        .map_err(|_| "Cannot read viewer sequence")?
        .into_iter()
        .map(ViewerEntry::from_row)
        .collect();

    Ok(entries)
}

fn media_id(raw: &str) -> Result<i64, String> {
    raw.parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| "Invalid media ID".into())
}

pub(super) fn checked_viewer_path(
    root: &Path,
    relative: &str,
    revision: Option<&str>,
) -> Result<PathBuf, String> {
    let path = checked_action_path(root, relative)?;

    if let Some(expected) = revision {
        let current = inspect(path.clone(), root)?;

        if current.is_none_or(|current| current.revision != expected) {
            return Err("File changed; rescan its source before viewing".into());
        }
    }

    Ok(path)
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(library), err)]
pub async fn get_viewer_source(
    app: tauri::AppHandle,
    id: String,
    library: bool,
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, StageState>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<ViewerSource, String> {
    let id = media_id(&id)?;

    let _guard = state.operation().await;
    crate::disk_sync::recover(database.pool()).await?;

    let settings = config.snapshot()?.settings;

    let root = if library {
        settings.library_path
    } else {
        settings.staging_path
    };

    tracing::info!(media_id = id, "Original media access started");

    let file = action_file(database.pool(), root.as_deref(), library, id).await?;

    let path = tauri::async_runtime::spawn_blocking(move || {
        checked_viewer_path(
            Path::new(&file.root_path),
            &file.relative_path,
            file.file_revision.as_deref(),
        )
    })
    .await
    .map_err(|_| "Media access worker failed")??;

    let metadata = sqlx::query(concat!(
        "SELECT m.favorite,v.fps_num,v.fps_den FROM media m ",
        "LEFT JOIN video_metadata v ON v.media_id=m.id WHERE m.id=?",
    ))
    .bind(id)
    .fetch_one(database.pool())
    .await
    .map_err(|_| "Cannot read playback metadata")?;

    let favorite = metadata.get::<bool, _>("favorite");

    let fps = metadata
        .get::<Option<i64>, _>("fps_num")
        .zip(metadata.get::<Option<i64>, _>("fps_den"))
        .filter(|(_, denominator)| *denominator > 0)
        .map(|(numerator, denominator)| numerator as f64 / denominator as f64);

    let tags = sqlx::query_scalar(concat!(
        "SELECT t.name FROM tags t JOIN media_tags mt ON mt.tag_id=t.id ",
        "WHERE mt.media_id=? ORDER BY t.name COLLATE NOCASE",
    ))
    .bind(id)
    .fetch_all(database.pool())
    .await
    .map_err(|_| "Cannot read tags")?;

    // Authorize only this validated file, never its directory or a frontend path.
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|_| "Cannot authorize original media")?;

    tracing::info!(media_id = id, "Original media access ready");
    Ok(ViewerSource {
        path: path
            .to_str()
            .ok_or("File path is not valid Unicode")?
            .into(),
        favorite,
        tags,
        fps,
    })
}

pub(super) async fn save_annotations(
    pool: &SqlitePool,
    root: Option<&Path>,
    library: bool,
    id: i64,
    favorite: Option<bool>,
    tags: Option<&[String]>,
) -> Result<(), String> {
    if favorite.is_none() && tags.is_none() {
        return Err("Choose annotations to save".into());
    }

    if tags.is_some_and(|tags| tags.len() > 100 || tags.iter().any(|tag| tag.chars().count() > 24))
    {
        return Err("Use at most 100 tags of up to 24 characters".into());
    }

    action_file(pool, root, library, id).await?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "Cannot start annotation save")?;

    if let Some(tags) = tags {
        write_tags(&mut tx, id, tags).await?;
    }

    sqlx::query(concat!(
        "UPDATE media SET favorite=COALESCE(?,favorite),",
        "updated_at=CAST(strftime('%s','now') AS INTEGER)*1000 WHERE id=?",
    ))
    .bind(favorite)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|_| "Cannot save annotations")?;

    tx.commit()
        .await
        .map_err(|_| "Cannot commit annotations".into())
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(library), err)]
pub async fn save_viewer_annotations(
    id: String,
    library: bool,
    favorite: Option<bool>,
    tags: Option<Vec<String>>,
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, StageState>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<(), String> {
    let id = media_id(&id)?;

    let _guard = state.operation().await;
    crate::disk_sync::recover(database.pool()).await?;

    let settings = config.snapshot()?.settings;

    let root = if library {
        settings.library_path
    } else {
        settings.staging_path
    };

    tracing::info!(media_id = id, "Viewer annotation save started");
    save_annotations(
        database.pool(),
        root.as_deref(),
        library,
        id,
        favorite,
        tags.as_deref(),
    )
    .await?;
    tracing::info!(media_id = id, "Viewer annotations saved");
    Ok(())
}
