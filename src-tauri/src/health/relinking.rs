use std::{collections::BTreeMap, path::PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef, sqlite::SqliteRow};

use crate::{
    config::AppConfig,
    media_metadata::{self, MediaKind},
    media_paths::checked_action_path,
    naming::name_key,
    stage,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Proposal {
    pub(super) token: String,
    pub(super) path: String,
    pub(super) consumes_discovery: bool,
}

struct Prepared {
    source: SqliteRow,
    destination: Option<SqliteRow>,
    root: PathBuf,
    source_kind: String,
    file: stage::Discovered,
    proposal: Proposal,
}

async fn record(pool: &SqlitePool, id: i64) -> Result<SqliteRow, String> {
    sqlx::query(concat!(
        "SELECT m.*,s.root_path,s.kind source_kind,v.title,",
        "(SELECT group_concat(tag_id,',') FROM (SELECT tag_id FROM media_tags WHERE media_id=m.id ORDER BY tag_id)) tags,",
        "(SELECT group_concat(copyright_id,',') FROM (SELECT copyright_id FROM media_copyrights WHERE media_id=m.id ORDER BY copyright_id)) copyrights,",
        "(SELECT group_concat(character_id,',') FROM (SELECT character_id FROM media_characters WHERE media_id=m.id ORDER BY character_id)) characters ",
        "FROM media m JOIN sources s ON s.id=m.source_id LEFT JOIN video_metadata v ON v.media_id=m.id WHERE m.id=?",
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or("Media unavailable".into())
}

fn signature(row: &SqliteRow) -> Result<serde_json::Value, String> {
    let mut fields = BTreeMap::new();

    for (index, column) in row.columns().iter().enumerate() {
        let raw = row.try_get_raw(index).map_err(|error| error.to_string())?;
        let value = if raw.is_null() {
            serde_json::Value::Null
        } else if raw.type_info().name() == "INTEGER" {
            serde_json::json!(
                row.try_get::<i64, _>(index)
                    .map_err(|error| error.to_string())?
            )
        } else {
            serde_json::json!(
                row.try_get::<String, _>(index)
                    .map_err(|error| error.to_string())?
            )
        };
        fields.insert(column.name(), value);
    }

    serde_json::to_value(fields).map_err(|error| error.to_string())
}

fn unannotated(row: &SqliteRow) -> bool {
    let relative: String = row.get("relative_path");
    let inferred_title = std::path::Path::new(&relative)
        .file_name()
        .and_then(|value| value.to_str())
        .and_then(stage::video_title_from_filename);
    let title: Option<String> = row.get("title");
    let automatic_title = title
        .as_deref()
        .is_none_or(|title| title.is_empty() || Some(title) == inferred_title);

    row.get::<Option<i64>, _>("artist_id").is_none()
        && row.get::<i64, _>("favorite") == 0
        && row.get::<i64, _>("needs_distribution") == 0
        && row.get::<Option<String>, _>("library_assignment").is_none()
        && row.get::<Option<String>, _>("naming_order_key").is_none()
        && automatic_title
        && ["tags", "copyrights", "characters"].iter().all(|column| {
            row.get::<Option<String>, _>(*column)
                .is_none_or(|value| value.is_empty())
        })
}

async fn ensure_no_journal(pool: &SqlitePool, id: i64, history: bool) -> Result<(), String> {
    let pending: bool = sqlx::query_scalar(concat!(
        "SELECT EXISTS(SELECT 1 FROM file_operations WHERE media_id=?) OR ",
        "EXISTS(SELECT 1 FROM media_replacements WHERE (main_id=? OR stage_id=?) AND (? OR finished=0))",
    ))
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(history)
    .fetch_one(pool)
    .await
    .map_err(|error| error.to_string())?;

    if pending {
        return Err("This file has recovery or replacement history. Resolve the operation before relinking.".into());
    }

    Ok(())
}

fn locate(
    path: PathBuf,
    settings: &AppConfig,
) -> Result<(PathBuf, String, stage::Discovered), String> {
    if !path.is_absolute() {
        return Err("Choose an absolute file path.".into());
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| "Selected file is unavailable.")?;

    // Stage takes precedence when configured inside Library.
    for (candidate, kind) in [
        (&settings.staging_path, "stage"),
        (&settings.library_path, "library"),
    ] {
        if let Some(root) = candidate
            && let Ok(canonical_root) = root.canonicalize()
            && let Ok(relative) = canonical.strip_prefix(canonical_root)
        {
            let root: PathBuf = root.components().collect();
            let relative = relative.to_str().ok_or("Invalid Unicode path")?;
            let checked = checked_action_path(&root, relative)?;
            let file = stage::inspect(checked, &root)?.ok_or("Unsupported media file")?;
            return Ok((root, kind.into(), file));
        }
    }

    let root = path.parent().ok_or("Invalid selected path")?.to_path_buf();
    let relative = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("Invalid Unicode path")?;
    let checked = checked_action_path(&root, relative)?;
    let file = stage::inspect(checked, &root)?.ok_or("Unsupported media file")?;

    Ok((root, "external".into(), file))
}

async fn prepare(
    pool: &SqlitePool,
    id: i64,
    path: PathBuf,
    settings: &AppConfig,
) -> Result<Prepared, String> {
    let source = record(pool, id).await?;

    if source.get::<String, _>("availability") != "missing" {
        return Err("This record is no longer missing. Refresh the health check.".into());
    }

    ensure_no_journal(pool, id, false).await?;
    let old = PathBuf::from(source.get::<String, _>("root_path"))
        .join(source.get::<String, _>("relative_path"));
    let settings_copy = settings.clone();
    let (root, mut source_kind, file) = tauri::async_runtime::spawn_blocking(move || {
        let located = locate(path, &settings_copy)?;

        match std::fs::symlink_metadata(&old) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(metadata)
                if metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && old.canonicalize().ok().is_some_and(|path| {
                        located.2.path.canonicalize().ok().as_ref() == Some(&path)
                    }) => {}
            _ => {
                return Err(
                    "The original file exists or cannot be verified. Relinking was cancelled."
                        .into(),
                );
            }
        }
        Ok::<_, String>(located)
    })
    .await
    .map_err(|_| "Relink inspection worker failed")??;

    let kind = if file.kind == MediaKind::Video {
        "video"
    } else {
        "image"
    };
    if source.get::<String, _>("kind") != kind {
        return Err("Choose a file of the same media type.".into());
    }

    if source_kind == "external" {
        source_kind = source.get("source_kind");
    }

    // Match canonical full paths too: selected aliases must not create a second physical owner.
    let target_path = file.path.clone();
    let target_canonical = tauri::async_runtime::spawn_blocking(move || target_path.canonicalize())
        .await
        .map_err(|_| "Relink conflict inspection failed")?
        .map_err(|_| "Selected file is unavailable.")?;
    let mut cursor: Option<i64> = None;
    let mut destination = None;

    loop {
        let rows = sqlx::query(concat!(
            "SELECT m.id,s.root_path,m.relative_path FROM media m JOIN sources s ON s.id=m.source_id ",
            "WHERE (? IS NULL OR m.id>?) AND m.id<>? ORDER BY m.id LIMIT 128",
        ))
        .bind(cursor)
        .bind(cursor)
        .bind(id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
        if rows.is_empty() {
            break;
        }
        cursor = Some(rows.last().unwrap().get("id"));
        let target = target_canonical.clone();
        let matches = tauri::async_runtime::spawn_blocking(move || {
            rows.into_iter()
                .filter_map(|row| {
                    let path = PathBuf::from(row.get::<String, _>("root_path"))
                        .join(row.get::<String, _>("relative_path"));
                    // Include a stored exact path even if an external writer briefly removes it.
                    (path == target || path.canonicalize().ok().as_ref() == Some(&target))
                        .then(|| row.get::<i64, _>("id"))
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|_| "Relink conflict inspection failed")?;

        for target_id in matches {
            if destination.is_some() {
                return Err(
                    "Multiple catalog records refer to this file. Resolve the conflict first."
                        .into(),
                );
            }
            let row = record(pool, target_id).await?;
            ensure_no_journal(pool, target_id, true).await?;
            if !unannotated(&row) {
                return Err("The selected file already has catalog metadata. It cannot be replaced by relinking.".into());
            }
            destination = Some(row);
        }
    }

    let digest = Sha256::digest(serde_json::to_vec(&serde_json::json!({
        "source": signature(&source)?, "destination": destination.as_ref().map(signature).transpose()?,
        "root": root, "path": file.relative, "revision": file.revision,
        "settings": settings,
    })).map_err(|error| error.to_string())?);
    let token = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let proposal = Proposal {
        token,
        path: file.path.to_string_lossy().into_owned(),
        consumes_discovery: destination.is_some(),
    };

    Ok(Prepared {
        source,
        destination,
        root,
        source_kind,
        file,
        proposal,
    })
}

pub(super) async fn inspect(
    pool: &SqlitePool,
    id: i64,
    path: PathBuf,
    settings: &AppConfig,
) -> Result<Proposal, String> {
    Ok(prepare(pool, id, path, settings).await?.proposal)
}

pub(super) async fn confirm(
    pool: &SqlitePool,
    id: i64,
    path: PathBuf,
    token: &str,
    settings: &AppConfig,
) -> Result<(), String> {
    let prepared = prepare(pool, id, path.clone(), settings).await?;

    if token != prepared.proposal.token {
        return Err("The file or catalog changed. Select the file again.".into());
    }

    let probe = prepared.file.clone();
    let root = prepared.root.clone();
    let metadata = tauri::async_runtime::spawn_blocking(move || {
        let metadata = media_metadata::query_media_metadata(&probe.path, probe.kind).ok();
        let checked = checked_action_path(&root, &probe.relative)?;
        let current = stage::inspect(checked, &root)?.ok_or("Unsupported media file")?;
        if current.revision != probe.revision {
            return Err("The file or catalog changed. Select the file again.".into());
        }
        Ok::<_, String>(metadata)
    })
    .await
    .map_err(|_| "Relink metadata worker failed")??;

    // Revalidate after Shell work and before opening a write transaction.
    let current = prepare(pool, id, path, settings).await?;
    if current.proposal.token != token {
        return Err("The file or catalog changed. Select the file again.".into());
    }

    let file = &prepared.file;
    let root_string = prepared.root.to_str().ok_or("Invalid Unicode path")?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|error| error.to_string())?;
    let existing_source: Option<i64> =
        sqlx::query_scalar("SELECT id FROM sources WHERE path_key=?")
            .bind(root_string)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    let source = if let Some(source) = existing_source {
        source
    } else {
        sqlx::query_scalar(concat!(
            "INSERT INTO sources(kind,root_path,path_key,is_active,created_at) ",
            "VALUES(?,?,?,0,?) RETURNING id",
        ))
        .bind(&prepared.source_kind)
        .bind(root_string)
        .bind(root_string)
        .bind(stage::now())
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| error.to_string())?
    };

    if let Some(destination) = prepared.destination {
        sqlx::query("DELETE FROM media WHERE id=?")
            .bind(destination.get::<i64, _>("id"))
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }

    let old_assignment: Option<String> = prepared.source.get("library_assignment");
    let was_main = old_assignment.is_some()
        || (prepared.source.get::<String, _>("source_kind") == "library"
            && prepared.source.get::<i64, _>("needs_distribution") == 0);
    let assignment = if was_main {
        settings
            .library_path
            .as_ref()
            .map(|root| {
                root.components()
                    .collect::<PathBuf>()
                    .to_string_lossy()
                    .into_owned()
            })
            .or(old_assignment)
            .or_else(|| Some(prepared.source.get::<String, _>("root_path")))
    } else {
        None
    };
    let queued = !was_main || prepared.source.get::<i64, _>("needs_distribution") != 0;
    let ready = metadata.is_some();

    sqlx::query(concat!(
        "UPDATE media SET source_id=?,relative_path=?,path_key=?,extension=?,availability='present',",
        "size_bytes=?,file_modified_at=?,file_created_at=?,file_revision=?,metadata_version=?,",
        "metadata_state=?,width=?,height=?,stage_search_key=?,library_assignment=?,needs_distribution=?,",
        "last_seen_at=?,updated_at=? WHERE id=?",
    ))
    .bind(source)
    .bind(&file.relative)
    .bind(&file.relative)
    .bind(&file.extension)
    .bind(file.size)
    .bind(file.modified)
    .bind(file.created)
    .bind(&file.revision)
    .bind(stage::EXTRACTOR_VERSION)
    .bind(if ready { "ready" } else { "failed" })
    .bind(metadata.as_ref().and_then(|value| value.width))
    .bind(metadata.as_ref().and_then(|value| value.height))
    .bind(name_key(&file.path.to_string_lossy().replace('\\', "/")))
    .bind(assignment)
    .bind(queued)
    .bind(stage::now())
    .bind(stage::now())
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    if file.kind == MediaKind::Video {
        let metadata = metadata.unwrap_or_default();
        sqlx::query(concat!(
            "INSERT INTO video_metadata(media_id,duration_ms,fps_num,fps_den,codec,bitrate_bps) ",
            "VALUES(?,?,?,?,?,?) ON CONFLICT(media_id) DO UPDATE SET duration_ms=excluded.duration_ms,",
            "fps_num=excluded.fps_num,fps_den=excluded.fps_den,codec=excluded.codec,bitrate_bps=excluded.bitrate_bps",
        ))
        .bind(id)
        .bind(metadata.duration_ms.and_then(|value| i64::try_from(value).ok()))
        .bind(metadata.frame_rate.map(|rate| rate.numerator))
        .bind(metadata.frame_rate.map(|rate| rate.denominator))
        .bind(metadata.codec)
        .bind(metadata.bitrate_bps)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }

    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}
