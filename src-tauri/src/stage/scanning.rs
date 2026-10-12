//! Bounded discovery, extraction, persistence and scan lifecycle.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use tauri::{Emitter, Manager};

use super::{BATCH, Discovered, EXTRACTOR_VERSION, ScanStatus, StageState, inspect, now};
use crate::{
    config::ConfigStore,
    database::Database,
    media_metadata::{self, MediaKind, MediaMetadata},
};

pub(super) enum WalkMessage {
    Batch(Vec<Discovered>),
    Complete(u64),
}

pub(super) fn walk(
    root: &Path,
    directory: &Path,
    excluded: Option<&Path>,
    depth: usize,
    batch: &mut Vec<Discovered>,
    errors: &mut u64,
    tx: &tokio::sync::mpsc::Sender<WalkMessage>,
) -> Result<(), String> {
    if excluded.is_some_and(|path| {
        directory
            .canonicalize()
            .unwrap_or_else(|_| directory.to_path_buf())
            .starts_with(path)
    }) {
        tracing::debug!("Configured Stage subtree excluded from Library traversal");

        return Ok(());
    }

    if depth > 256 {
        *errors += 1;

        return Ok(());
    }

    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => {
            *errors += 1;

            return Ok(());
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                *errors += 1;
                continue;
            }
        };

        let file_type = match entry.file_type() {
            Ok(value) => value,
            Err(_) => {
                *errors += 1;
                continue;
            }
        };
        // Do not follow symlinks/junctions outside the source or into cycles.
        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            walk(root, &entry.path(), excluded, depth + 1, batch, errors, tx)?;
        } else if file_type.is_file() {
            match inspect(entry.path(), root) {
                Ok(Some(file)) => batch.push(file),
                Ok(None) => {}
                Err(_) => *errors += 1,
            }

            if batch.len() == BATCH {
                tx.blocking_send(WalkMessage::Batch(std::mem::take(batch)))
                    .map_err(|_| "Scanner stopped")?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) async fn activate_source(
    pool: &SqlitePool,
    root: &Path,
    scan: i64,
) -> Result<i64, String> {
    activate_source_for(pool, root, scan, "stage").await
}

pub(crate) async fn activate_source_for(
    pool: &SqlitePool,
    root: &Path,
    scan: i64,
    source_kind: &str,
) -> Result<i64, String> {
    let root = root.to_str().ok_or("Invalid Unicode media root")?;

    let other_roots: Vec<String> =
        sqlx::query_scalar("SELECT root_path FROM sources WHERE is_active=1 AND kind<>?")
            .bind(source_kind)
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;

    let candidate = PathBuf::from(root);

    let library = source_kind == "library";
    tauri::async_runtime::spawn_blocking(move || {
        let candidate = candidate
            .canonicalize()
            .map_err(|_| "Source folder is unavailable")?;

        for other in other_roots {
            let other = PathBuf::from(other);

            let other = other.canonicalize().unwrap_or(other);

            if candidate == other
                || (library && candidate.starts_with(&other))
                || (!library && other.starts_with(&candidate))
            {
                return Err("Roots must differ and Library must not be inside Stage".to_string());
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| "Root validation worker failed")??;

    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|e| e.to_string())?;

    let existing = sqlx::query("SELECT id, kind FROM sources WHERE path_key = ?")
        .bind(root)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    if existing
        .as_ref()
        .is_some_and(|row| row.get::<String, _>("kind") != source_kind)
    {
        return Err("Root already belongs to another source kind".into());
    }
    sqlx::query("UPDATE sources SET is_active = 0 WHERE kind = ?")
        .bind(source_kind)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    let id = if let Some(row) = existing {
        let id: i64 = row.get("id");
        sqlx::query("UPDATE sources SET is_active = 1 WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        id
    } else {
        sqlx::query_scalar(
            "INSERT INTO sources(kind,root_path,path_key,created_at) VALUES(?,?,?,?) RETURNING id",
        )
        .bind(source_kind)
        .bind(root)
        .bind(root)
        .bind(scan)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?
    };
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(id)
}

pub(super) struct Prepared {
    pub(super) file: Discovered,
    pub(super) metadata: Option<MediaMetadata>,
    pub(super) failed: bool,
}

pub(super) async fn prepare(
    pool: &SqlitePool,
    source: i64,
    files: Vec<Discovered>,
    status: &mut ScanStatus,
) -> Result<Vec<Prepared>, String> {
    let mut prepared = Vec::with_capacity(files.len());

    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT path_key,file_revision,metadata_version,metadata_state FROM media WHERE source_id=",
    );
    query.push_bind(source).push(" AND path_key IN (");

    let mut paths = query.separated(",");

    for file in &files {
        paths.push_bind(&file.relative);
    }
    paths.push_unseparated(")");

    let rows = query
        .build()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let stored: HashMap<String, _> = rows
        .into_iter()
        .map(|row| (row.get("path_key"), row))
        .collect();

    let mut retired_query = QueryBuilder::<Sqlite>::new(concat!(
        "SELECT old_relative,old_revision FROM media_replacements WHERE committed=1 AND ",
        "delete_old=0 AND backup_root IS NULL AND old_source=",
    ));
    retired_query
        .push_bind(source)
        .push(" AND old_relative IN (");

    let mut paths = retired_query.separated(",");

    for file in &files {
        paths.push_bind(&file.relative);
    }
    paths.push_unseparated(")");

    let retired: std::collections::HashSet<(String, String)> = retired_query
        .build()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| (r.get("old_relative"), r.get("old_revision")))
        .collect();

    for file in files {
        if retired.contains(&(file.relative.clone(), file.revision.clone())) {
            tracing::debug!(
                source_id = source,
                "Retained replaced version skipped during scan"
            );
            continue;
        }

        let unchanged = stored.get(&file.relative).is_some_and(|row| {
            row.get::<Option<String>, _>("file_revision").as_deref() == Some(&file.revision)
                && row.get::<i64, _>("metadata_version") == EXTRACTOR_VERSION
                && row.get::<String, _>("metadata_state") == "ready"
        });

        if unchanged {
            prepared.push(Prepared {
                file,
                metadata: None,
                failed: false,
            });
            continue;
        }

        let extraction = file.clone();

        let result = tauri::async_runtime::spawn_blocking(move || {
            let result = media_metadata::query_media_metadata(&extraction.path, extraction.kind);
            // A changing file must not associate stale metadata with a new revision.
            let stable = inspect(
                extraction.path.clone(),
                extraction.path.parent().unwrap_or(Path::new("")),
            )?
            .is_some_and(|current| current.revision == extraction.revision);

            if !stable {
                return Err("Source changed during metadata extraction".to_owned());
            }
            Ok(result.ok())
        })
        .await
        .map_err(|e| e.to_string())?;

        match result {
            Ok(metadata) => {
                status.extracted += 1;

                let failed = metadata.is_none();

                if failed {
                    status.errors += 1;
                }
                prepared.push(Prepared {
                    file,
                    metadata,
                    failed,
                });
            }
            Err(_) => {
                status.errors += 1;

                tracing::warn!("Changing media file deferred to next scan");
            }
        }
    }
    Ok(prepared)
}

pub(super) fn video_title_from_filename(filename: &str) -> Option<&str> {
    let stem = Path::new(filename).file_stem()?.to_str()?.trim();

    let artist_start = stem.rfind(" [")?;

    let (name, attribution) = stem.split_at(artist_start);

    let (artist, suffix) = attribution.strip_prefix(" [")?.split_once(']')?;

    if artist.trim().is_empty() || artist.contains(['[', ']']) {
        return None;
    }

    let suffix = suffix.trim();

    if !suffix.is_empty() {
        let index = suffix.strip_prefix("- ")?;

        if index.is_empty()
            || !index.bytes().all(|b| b.is_ascii_digit())
            || index.parse::<u64>().ok()? == 0
        {
            return None;
        }
    }

    let (characters, title) = name.split_once(" - ")?;

    let title = title.trim();

    if characters.trim().is_empty() || title.is_empty() {
        return None;
    }
    Some(title)
}

pub(super) async fn save_batch(
    pool: &SqlitePool,
    source: i64,
    scan: i64,
    batch: Vec<Prepared>,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let library: bool = sqlx::query_scalar("SELECT kind='library' FROM sources WHERE id=?")
        .bind(source)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    let mut recovered_titles = 0;

    for Prepared {
        file,
        metadata,
        failed,
    } in batch
    {
        let changed = metadata.is_some() || failed;

        let filename = file
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("Invalid filename")?;

        let id: i64 = sqlx::query_scalar(concat!(
            "INSERT INTO media(source_id,relative_path,original_filename,path_key,kind,extension,",
            "size_bytes,file_modified_at,file_created_at,file_revision,metadata_version,",
            "metadata_state,width,height,last_seen_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,",
            "?,?,?,?,?,?,?,?,?,?) ON CONFLICT(source_id,path_key) DO UPDATE SET ",
            "availability='present',size_bytes=excluded.size_bytes,",
            "file_modified_at=excluded.file_modified_at,file_created_at=excluded.file_created_at,",
            "file_revision=excluded.file_revision,metadata_version=excluded.metadata_version,",
            "metadata_state=CASE WHEN ? THEN excluded.metadata_state ELSE media.metadata_state END,",
            "width=CASE WHEN ? THEN excluded.width ELSE media.width END,height=CASE WHEN ? THEN ",
            "excluded.height ELSE media.height END,last_seen_at=excluded.last_seen_at,",
            "updated_at=excluded.updated_at RETURNING id",
        ))
            .bind(source)
            .bind(&file.relative)
            .bind(filename)
            .bind(&file.relative)
            .bind(if file.kind == MediaKind::Video { "video" } else { "image" })
            .bind(&file.extension)
            .bind(file.size)
            .bind(file.modified)
            .bind(file.created)
            .bind(&file.revision)
            .bind(EXTRACTOR_VERSION)
            .bind(if failed { "failed" } else { "ready" })
            .bind(metadata.as_ref().and_then(|m| m.width))
            .bind(metadata.as_ref().and_then(|m| m.height))
            .bind(scan)
            .bind(scan)
            .bind(scan)
            .bind(changed)
            .bind(changed)
            .bind(changed)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        if file.kind == MediaKind::Video && changed {
            let value = metadata.unwrap_or_default();
            sqlx::query(concat!(
                "INSERT INTO video_metadata(media_id,duration_ms,fps_num,fps_den,codec,bitrate_bps) ",
                "VALUES(?,?,?,?,?,?) ON CONFLICT(media_id) DO UPDATE SET ",
                "duration_ms=excluded.duration_ms,fps_num=excluded.fps_num,fps_den=excluded.fps_den,",
                "codec=excluded.codec,bitrate_bps=excluded.bitrate_bps",
            ))
                .bind(id)
                .bind(value.duration_ms.and_then(|v| i64::try_from(v).ok()))
                .bind(value.frame_rate.map(|v| v.numerator))
                .bind(value.frame_rate.map(|v| v.denominator))
                .bind(value.codec)
                .bind(value.bitrate_bps)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }
        // Backfill on every Library scan, including unchanged/failed technical extraction.
        if library
            && file.kind == MediaKind::Video
            && let Some(title) = video_title_from_filename(filename)
        {
            recovered_titles += sqlx::query(concat!(
                "INSERT INTO video_metadata(media_id,title) VALUES(?,?) ON CONFLICT(media_id) DO ",
                "UPDATE SET title=excluded.title WHERE video_metadata.title IS NULL OR ",
                "trim(video_metadata.title,char(9)||char(10)||char(13)||' ')=''",
            ))
            .bind(id)
            .bind(title)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected();
        }
        sqlx::query("UPDATE media SET stage_search_key=? WHERE id=? AND stage_search_key IS NULL")
            .bind(crate::naming::name_key(
                &file.path.to_string_lossy().replace('\\', "/"),
            ))
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;

    if recovered_titles > 0 {
        tracing::info!(
            source_id = source,
            recovered_titles,
            "Missing Library video titles recovered from structured filenames"
        );
    }
    Ok(())
}

pub(super) fn publish(app: &tauri::AppHandle, status: &ScanStatus) {
    if let Ok(mut stored) = app.state::<StageState>().status.lock() {
        *stored = status.clone();
    }

    if app.emit("stage-scan-status", status).is_err() {
        tracing::debug!("Scan progress listener unavailable");
    }
}

#[cfg(test)]
pub(super) async fn scan(
    pool: &SqlitePool,
    root: PathBuf,
    report: impl FnMut(&ScanStatus),
) -> Result<ScanStatus, String> {
    scan_source(pool, root, "stage", None, report).await
}

#[tracing::instrument(skip_all, fields(source_kind))]
pub(super) async fn scan_source(
    pool: &SqlitePool,
    root: PathBuf,
    source_kind: &str,
    excluded: Option<PathBuf>,
    mut report: impl FnMut(&ScanStatus),
) -> Result<ScanStatus, String> {
    tracing::info!(
        source_kind,
        excludes_stage = excluded.is_some(),
        "Media scan started"
    );

    let checked_root = root.clone();

    let checked_exclusion = excluded.clone();

    let root = tauri::async_runtime::spawn_blocking(move || {
        if !checked_root.is_absolute() {
            return Err("Media directory must be absolute");
        }

        if !std::fs::metadata(&checked_root)
            .map_err(|_| "Media directory is unavailable")?
            .is_dir()
        {
            return Err("Media path is not a directory");
        }
        std::fs::read_dir(&checked_root).map_err(|_| "Media directory cannot be read")?;

        if let Some(stage) = checked_exclusion {
            let stage = stage.canonicalize().unwrap_or(stage);

            let library = checked_root
                .canonicalize()
                .map_err(|_| "Media directory is unavailable")?;

            if library.starts_with(stage) {
                return Err("Roots must differ and Library must not be inside Stage");
            }
        }
        Ok(checked_root.components().collect::<PathBuf>())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(str::to_owned)?;
    // A monotonic per-source marker avoids same-millisecond repeat scans.
    let previous: Option<i64> = sqlx::query_scalar("SELECT max(last_seen_at) FROM media")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    let marker = now().max(previous.unwrap_or(0).saturating_add(1));
    crate::disk_sync::recover(pool).await?;

    let source = activate_source_for(pool, &root, marker, source_kind).await?;

    let (tx, mut rx) = tokio::sync::mpsc::channel(4);

    let walker = tauri::async_runtime::spawn_blocking(move || {
        let excluded = excluded.map(|path| path.canonicalize().unwrap_or(path));

        let mut batch = Vec::with_capacity(BATCH);

        let mut errors = 0;
        walk(
            &root,
            &root,
            excluded.as_deref(),
            0,
            &mut batch,
            &mut errors,
            &tx,
        )?;

        if !batch.is_empty() {
            tx.blocking_send(WalkMessage::Batch(batch))
                .map_err(|_| "Scanner stopped")?;
        }
        tx.blocking_send(WalkMessage::Complete(errors))
            .map_err(|_| "Scanner stopped")?;
        Ok::<_, String>(())
    });

    let mut status = ScanStatus {
        running: true,
        message: format!("Scanning {source_kind}…"),
        ..Default::default()
    };
    report(&status);

    let mut complete = false;

    let mut traversal_errors = 0;

    while let Some(message) = rx.recv().await {
        match message {
            WalkMessage::Batch(files) => {
                let count = files.len() as u64;

                let before = status.errors;

                let prepared = prepare(pool, source, files, &mut status).await?;
                // Files that changed during extraction also make reconciliation unsafe.
                if prepared.len() as u64 != count {
                    traversal_errors += status.errors - before;
                }
                save_batch(pool, source, marker, prepared).await?;

                tracing::debug!(batch_files = count, "Media batch committed");
                status.scanned += count;
                report(&status);
            }
            WalkMessage::Complete(errors) => {
                traversal_errors += errors;
                status.errors += errors;
                complete = true;
            }
        }
    }
    walker.await.map_err(|e| e.to_string())??;

    if !reconcile(pool, source, marker, complete, traversal_errors).await? {
        tracing::warn!(
            traversal_errors,
            "Incomplete media scan; unseen records preserved"
        );
    }
    status.running = false;
    status.message = if traversal_errors == 0 {
        format!("{source_kind} scan complete")
    } else {
        "Incomplete scan; unseen files preserved".into()
    };

    tracing::info!(
        scanned = status.scanned,
        extracted = status.extracted,
        errors = status.errors,
        "Media scan completed"
    );
    Ok(status)
}

pub(super) async fn reconcile(
    pool: &SqlitePool,
    source: i64,
    marker: i64,
    complete: bool,
    errors: u64,
) -> Result<bool, String> {
    if !complete || errors != 0 {
        return Ok(false);
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("UPDATE media SET availability='missing',updated_at=? WHERE source_id=? AND last_seen_at<>?")
    .bind(marker)
    .bind(source)
    .bind(marker)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query("UPDATE sources SET last_successful_scan_at=? WHERE id=?")
        .bind(marker)
        .bind(source)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(true)
}

pub fn launch(app: tauri::AppHandle, root: PathBuf) -> Result<(), String> {
    launch_sources(app, vec![(root, "stage")])
}

pub fn launch_sources(
    app: tauri::AppHandle,
    roots: Vec<(PathBuf, &'static str)>,
) -> Result<(), String> {
    if app
        .state::<StageState>()
        .scanning
        .swap(true, Ordering::AcqRel)
    {
        return Err("A media operation is already running".into());
    }
    publish(
        &app,
        &ScanStatus {
            running: true,
            message: "Starting media scan…".into(),
            ..Default::default()
        },
    );
    tauri::async_runtime::spawn(async move {
        let staging = app
            .state::<ConfigStore>()
            .snapshot()
            .map(|s| s.settings.staging_path);

        for (root, source_kind) in roots {
            let result = match &staging {
                Ok(staging) => {
                    scan_source(
                        app.state::<Database>().pool(),
                        root,
                        source_kind,
                        if source_kind == "library" {
                            staging.clone()
                        } else {
                            None
                        },
                        |status| publish(&app, status),
                    )
                    .await
                }
                Err(error) => Err(error.clone()),
            };

            let final_status = match result {
                Ok(status) => status,
                Err(error) => {
                    tracing::error!(
                        "Media scan failed; committed batches retained and missing reconciliation skipped"
                    );

                    let mut status = app
                        .state::<StageState>()
                        .status
                        .lock()
                        .map(|value| value.clone())
                        .unwrap_or_default();
                    status.running = false;
                    status.message = error;
                    status.errors += 1;
                    status
                }
            };
            publish(&app, &final_status);
        }
        app.state::<StageState>()
            .scanning
            .store(false, Ordering::Release);
        crate::disk_sync::request(&app);
    });
    Ok(())
}

#[tauri::command]
pub fn start_stage_scan(
    app: tauri::AppHandle,
    config: tauri::State<'_, ConfigStore>,
) -> Result<(), String> {
    let root = config
        .snapshot()?
        .settings
        .staging_path
        .ok_or("Save a Stage directory in Settings first")?;
    launch(app, root)
}

#[tauri::command]
pub fn get_stage_scan_status(state: tauri::State<'_, StageState>) -> Result<ScanStatus, String> {
    state
        .status
        .lock()
        .map(|value| value.clone())
        .map_err(|_| "Scan status unavailable".into())
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub fn start_library_scan(
    app: tauri::AppHandle,
    config: tauri::State<'_, ConfigStore>,
) -> Result<(), String> {
    let root = config
        .snapshot()?
        .settings
        .library_path
        .ok_or("Save a Library directory in Settings first")?;
    launch_sources(app, vec![(root, "library")])
}
