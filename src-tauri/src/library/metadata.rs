//! library: metadata responsibility.

use std::path::PathBuf;

use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use unicode_normalization::UnicodeNormalization;

use super::{Character, configured_root};
use crate::{config::ConfigStore, database::Database, naming::name_key, stage};

async fn ensure_library(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    id: i64,
    root: &Option<String>,
) -> Result<String, String> {
    sqlx::query_scalar(concat!(
        "SELECT m.kind FROM media m JOIN sources s ON s.id=m.source_id WHERE m.id=? AND ",
        "(m.library_assignment=? OR (s.kind='library' AND s.path_key=?)) AND ",
        "m.needs_distribution=0 AND m.availability='present'",
    ))
    .bind(id)
    .bind(root)
    .bind(root)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| e.to_string())?
    .ok_or("File is outside the current Library".into())
}

fn clean(value: &str) -> Result<String, String> {
    let value: String = value.trim().nfc().collect();

    if value.is_empty() || value.chars().any(char::is_control) {
        return Err("Metadata names must be nonempty without control characters".into());
    }
    Ok(value)
}

async fn dictionary(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    table: &str,
    value: &str,
) -> Result<i64, String> {
    let name = clean(value)?;

    let mut q = QueryBuilder::<Sqlite>::new("INSERT INTO ");
    q.push(table)
        .push("(name,name_key) VALUES(")
        .push_bind(&name)
        .push(",")
        .push_bind(name_key(&name))
        .push(") ON CONFLICT(name_key) DO UPDATE SET name_key=excluded.name_key RETURNING id");
    q.build_query_scalar()
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub(super) copyrights: Vec<String>,
    pub(super) artists: Vec<String>,
    pub(super) characters: Vec<Character>,
    pub(super) tags: Vec<String>,
    pub(super) video_title: String,
}

async fn write_tags(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    id: i64,
    tags: &[String],
) -> Result<(), String> {
    sqlx::query("DELETE FROM media_tags WHERE media_id=?")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;

    for tag in tags {
        let tag = dictionary(tx, "tags", tag).await?;
        sqlx::query("INSERT OR IGNORE INTO media_tags VALUES(?,?)")
            .bind(id)
            .bind(tag)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) async fn save(
    pool: &SqlitePool,
    root: Option<String>,
    ids: Vec<String>,
    draft: Draft,
) -> Result<(), String> {
    save_mode(pool, root, None, ids, draft).await
}

pub(super) async fn save_mode(
    pool: &SqlitePool,
    root: Option<String>,
    stage_root: Option<Option<String>>,
    ids: Vec<String>,
    draft: Draft,
) -> Result<(), String> {
    if ids.is_empty() || ids.len() > 500 || draft.artists.len() != 1 {
        return Err("Choose exactly one artist and 1–500 files".into());
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let artist = dictionary(&mut tx, "artists", &draft.artists[0]).await?;

    for raw in ids {
        let id = raw.parse::<i64>().map_err(|_| "Invalid media ID")?;

        let kind: String = if let Some(stage_root) = &stage_root {
            sqlx::query_scalar(concat!(
                "SELECT m.kind FROM media m JOIN sources s ON s.id=m.source_id WHERE m.id=? AND ",
                "m.availability='present' AND (m.needs_distribution=1 OR (s.kind='stage' AND ",
                "s.path_key=? AND m.library_assignment IS NULL))",
            ))
            .bind(id)
            .bind(stage_root)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("File is outside Stage")?
        } else {
            ensure_library(&mut tx, id, &root).await?
        };

        if kind == "video" && draft.video_title.trim().is_empty() {
            return Err("Video title is required".into());
        }
        sqlx::query("DELETE FROM media_copyrights WHERE media_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        let mut copyrights = std::collections::HashMap::new();

        for name in &draft.copyrights {
            let copyright = dictionary(&mut tx, "copyrights", name).await?;
            copyrights.insert(name_key(name.trim()), copyright);
            sqlx::query("INSERT OR IGNORE INTO media_copyrights VALUES(?,?)")
                .bind(id)
                .bind(copyright)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }

        for character in &draft.characters {
            let copyright = *copyrights
                .get(&name_key(character.copyright.trim()))
                .ok_or("Select the character's copyright")?;

            let name = clean(&character.name)?;

            let character: i64 = sqlx::query_scalar(concat!(
                "INSERT INTO characters(copyright_id,name,name_key) VALUES(?,?,?) ON ",
                "CONFLICT(copyright_id,name_key) DO UPDATE SET name_key=excluded.name_key RETURNING id",
            ))
.bind(copyright)
.bind(&name)
.bind(name_key(&name))
.fetch_one(&mut *tx)
.await
.map_err(|e| e.to_string())?;
            sqlx::query("INSERT OR IGNORE INTO media_characters VALUES(?,?,?)")
                .bind(id)
                .bind(character)
                .bind(copyright)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }
        write_tags(&mut tx, id, &draft.tags).await?;
        sqlx::query(concat!(
            "UPDATE media SET artist_id=?,library_assignment=?,needs_distribution=0,",
            "updated_at=CAST(strftime('%s','now') AS INTEGER)*1000 WHERE id=?",
        ))
        .bind(artist)
        .bind(&root)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        if kind == "video" {
            sqlx::query(concat!(
                "INSERT INTO video_metadata(media_id,title) VALUES(?,?) ON CONFLICT(media_id) DO ",
                "UPDATE SET title=excluded.title",
            ))
            .bind(id)
            .bind(draft.video_title.trim())
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().await.map_err(|e| e.to_string())
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(count=ids.len()), err)]
pub async fn save_library_metadata(
    app: tauri::AppHandle,
    state: tauri::State<'_, stage::StageState>,
    ids: Vec<String>,
    draft: Draft,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<(), String> {
    let _guard = state.operation().await;
    crate::disk_sync::recover(database.pool()).await?;

    tracing::info!("Library metadata save started");
    save(database.pool(), configured_root(&config)?, ids, draft).await?;
    crate::disk_sync::request(&app);

    tracing::info!("Library metadata save completed");
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(count=ids.len()), err)]
pub async fn distribute_files(
    app: tauri::AppHandle,
    ids: Vec<String>,
    draft: Draft,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
    state: tauri::State<'_, stage::StageState>,
) -> Result<(), String> {
    let _guard = state.operation().await;
    crate::disk_sync::recover(database.pool()).await?;

    let settings = config.snapshot()?.settings;

    let root = configured_root(&config)?.ok_or("Choose a Library directory before distribution")?;

    let staging = settings.staging_path.map(|p| {
        p.components()
            .collect::<PathBuf>()
            .to_string_lossy()
            .into_owned()
    });

    tracing::info!("Distribution metadata save started");
    save_mode(database.pool(), Some(root), Some(staging), ids, draft).await?;
    crate::disk_sync::request(&app);

    tracing::info!("Distribution saved; physical synchronization queued when enabled");
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub async fn save_library_tags(
    id: String,
    tags: Vec<String>,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<(), String> {
    let id = id.parse::<i64>().map_err(|_| "Invalid media ID")?;

    let mut tx = database.pool().begin().await.map_err(|e| e.to_string())?;
    ensure_library(&mut tx, id, &configured_root(&config)?).await?;
    write_tags(&mut tx, id, &tags).await?;
    sqlx::query(
        "UPDATE media SET updated_at=CAST(strftime('%s','now') AS INTEGER)*1000 WHERE id=?",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;

    tracing::info!("Library tags saved");
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub async fn set_library_favorite(
    id: String,
    favorite: bool,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<(), String> {
    let id = id.parse::<i64>().map_err(|_| "Invalid media ID")?;

    let mut tx = database.pool().begin().await.map_err(|e| e.to_string())?;
    ensure_library(&mut tx, id, &configured_root(&config)?).await?;
    sqlx::query("UPDATE media SET favorite=?,updated_at=CAST(strftime('%s','now') AS INTEGER)*1000 WHERE id=?")
    .bind(favorite)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;

    tracing::debug!("Library favorite saved");
    Ok(())
}
