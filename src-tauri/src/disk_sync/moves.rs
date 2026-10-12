//! disk_sync: moves responsibility.

use std::{fs, io::Write, path::Path};

use sqlx::{Row, SqlitePool};

use super::{equal_files, relative, revision};
use crate::naming::name_key;

#[derive(sqlx::FromRow, Clone)]
pub(super) struct Operation {
    pub(super) media_id: i64,
    pub(super) source_root: String,
    pub(super) source_relative: String,
    pub(super) source_revision: String,
    pub(super) target_source: i64,
    pub(super) target_root: String,
    pub(super) target_relative: String,
}

pub(super) async fn finish(
    pool: &SqlitePool,
    operation: Operation,
    recovery: bool,
) -> Result<(), String> {
    let op = operation.clone();

    let destination_revision = tauri::async_runtime::spawn_blocking(move || {
        let source = relative(Path::new(&op.source_root), &op.source_relative, false)?;

        let target = relative(Path::new(&op.target_root), &op.target_relative, true)?;

        if source.exists() {
            if revision(&source)? != op.source_revision {
                return Err("Source changed during synchronization; rescan and retry".into());
            }

            if target.exists() {
                if !recovery || !equal_files(&source, &target)? {
                    return Err("Destination already exists; existing content was preserved".into());
                }
            } else if fs::hard_link(&source, &target).is_err() {
                // Cross-volume fallback publishes only a fully flushed sibling temporary file.
                let mut temporary =
                    tempfile::NamedTempFile::new_in(target.parent().ok_or("Invalid destination")?)
                        .map_err(|e| e.to_string())?;

                let mut input = fs::File::open(&source).map_err(|e| e.to_string())?;
                std::io::copy(&mut input, &mut temporary).map_err(|e| e.to_string())?;
                temporary.flush().map_err(|e| e.to_string())?;
                temporary
                    .as_file()
                    .set_times(
                        fs::FileTimes::new().set_modified(
                            fs::metadata(&source)
                                .map_err(|e| e.to_string())?
                                .modified()
                                .map_err(|e| e.to_string())?,
                        ),
                    )
                    .map_err(|e| e.to_string())?;
                temporary.as_file().sync_all().map_err(|e| e.to_string())?;
                temporary
                    .persist_noclobber(&target)
                    .map_err(|e| e.to_string())?;
            }

            if revision(&source)? != op.source_revision || !equal_files(&source, &target)? {
                return Err("File changed while copying; both copies retained".into());
            }
        } else if !recovery {
            return Err("Source file is unavailable".into());
        }
        revision(&target)
    })
    .await
    .map_err(|e| e.to_string())??;
    // A missing source is recoverable only after the database commit already pointed at target.
    let current = sqlx::query("SELECT source_id,relative_path FROM media WHERE id=?")
        .bind(operation.media_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    let source_exists = {
        let op = operation.clone();
        tauri::async_runtime::spawn_blocking(move || {
            relative(Path::new(&op.source_root), &op.source_relative, false).map(|p| p.exists())
        })
        .await
        .map_err(|e| e.to_string())??
    };

    if !source_exists
        && (current.get::<i64, _>("source_id") != operation.target_source
            || current.get::<String, _>("relative_path") != operation.target_relative)
    {
        return Err(
            "Interrupted move requires recovery: source missing before database commit".into(),
        );
    }
    sqlx::query(concat!(
        "UPDATE media SET source_id=?,relative_path=?,path_key=?,stage_search_key=?,",
        "file_revision=?,availability='present',updated_at=CAST(unixepoch('subsec')*1000 AS ",
        "INTEGER) WHERE id=?",
    ))
    .bind(operation.target_source)
    .bind(&operation.target_relative)
    .bind(&operation.target_relative)
    .bind(name_key(
        &Path::new(&operation.target_root)
            .join(&operation.target_relative)
            .to_string_lossy()
            .replace('\\', "/"),
    ))
    .bind(destination_revision)
    .bind(operation.media_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    let op = operation.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let source = relative(Path::new(&op.source_root), &op.source_relative, false)?;

        if source.exists() {
            let target = relative(Path::new(&op.target_root), &op.target_relative, false)?;

            if revision(&source)? != op.source_revision || !equal_files(&source, &target)? {
                return Err("Source changed before cleanup; copies retained".to_string());
            }
            fs::remove_file(&source).map_err(|e| e.to_string())?;
            // Remove only empty ancestor directories, stopping at the source root.
            let root = Path::new(&op.source_root);

            let mut parent = source.parent();

            while let Some(path) = parent {
                if path == root || !path.starts_with(root) || fs::remove_dir(path).is_err() {
                    break;
                }
                parent = path.parent();
            }
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    sqlx::query("DELETE FROM file_operations WHERE media_id=?")
        .bind(operation.media_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    tracing::debug!(
        media_id = operation.media_id,
        recovery,
        "Managed move committed"
    );
    Ok(())
}

pub(super) async fn relocate(
    pool: &SqlitePool,
    id: i64,
    target_source: i64,
    root: &str,
    destination: &str,
) -> Result<(), String> {
    let row = sqlx::query(concat!(
        "SELECT s.root_path,m.relative_path,m.file_revision FROM media m JOIN sources s ON ",
        "s.id=m.source_id WHERE m.id=?",
    ))
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    let source_root: String = row.get("root_path");

    let source_relative: String = row.get("relative_path");

    if source_root == root && source_relative == destination {
        return Ok(());
    }

    let target_root = root.to_owned();

    let target_relative = destination.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        if relative(Path::new(&target_root), &target_relative, false)?
            .try_exists()
            .map_err(|e| e.to_string())?
        {
            return Err("Destination already exists; existing content was preserved".to_string());
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;

    let source = source_root.clone();

    let rel = source_relative.clone();

    let expected: String = tauri::async_runtime::spawn_blocking(move || {
        revision(&relative(Path::new(&source), &rel, false)?)
    })
    .await
    .map_err(|e| e.to_string())??;

    if row
        .get::<Option<String>, _>("file_revision")
        .is_some_and(|r| r != expected)
    {
        return Err("File changed; rescan before synchronization".into());
    }
    sqlx::query("INSERT INTO file_operations VALUES(?,?,?,?,?,?,?)")
        .bind(id)
        .bind(&source_root)
        .bind(&source_relative)
        .bind(&expected)
        .bind(target_source)
        .bind(root)
        .bind(destination)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    finish(
        pool,
        Operation {
            media_id: id,
            source_root,
            source_relative,
            source_revision: expected,
            target_source,
            target_root: root.into(),
            target_relative: destination.into(),
        },
        false,
    )
    .await
}
