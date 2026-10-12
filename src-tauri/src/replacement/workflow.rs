//! Journaled adoption, conservative file cleanup and restart recovery.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use sqlx::{Row, SqlitePool};

use super::{Report, Request, id, is_main, is_stage, record, root, token};
use crate::naming::name_key;
use crate::{
    config::{AppConfig, ConfigStore},
    database::Database,
    stage::{self, StageState},
};

#[derive(Clone, sqlx::FromRow)]
pub(super) struct Operation {
    pub(super) id: i64,
    pub(super) main_id: i64,
    pub(super) stage_id: i64,
    pub(super) old_source: i64,
    pub(super) old_root: String,
    pub(super) old_relative: String,
    pub(super) old_revision: String,
    pub(super) new_source: i64,
    pub(super) new_root: String,
    pub(super) new_relative: String,
    pub(super) new_revision: String,
    pub(super) library_root: String,
    pub(super) backup_root: Option<String>,
    pub(super) committed: i64,
    pub(super) delete_old: i64,
}

pub(super) fn backup(op: &Operation) -> Option<PathBuf> {
    op.backup_root.as_ref().map(|p| {
        Path::new(p).join(format!(
            "replacement-{}.{}",
            op.id,
            Path::new(&op.old_relative)
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
        ))
    })
}

pub(super) fn verified(root: &str, relative: &str, expected: &str) -> Result<PathBuf, String> {
    let path = stage::checked_action_path(Path::new(root), relative)?;

    if crate::disk_sync::revision(&path)? != expected {
        return Err("File changed; rescan and select both versions again".into());
    }
    Ok(path)
}

pub(super) fn preserve(op: &Operation) -> Result<(), String> {
    let incoming = verified(&op.new_root, &op.new_relative, &op.new_revision)?;

    let old = verified(&op.old_root, &op.old_relative, &op.old_revision)?;

    if old.canonicalize().map_err(|e| e.to_string())?
        == incoming.canonicalize().map_err(|e| e.to_string())?
    {
        return Err("Cannot replace a file with itself".into());
    }

    if let Some(backup) = backup(op) {
        let parent = backup.parent().ok_or("Invalid backup directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;

        if backup.exists() {
            let metadata = fs::symlink_metadata(&backup).map_err(|e| e.to_string())?;

            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || !crate::disk_sync::equal_files(&old, &backup)?
            {
                return Err("Replacement backup conflict; originals preserved".into());
            }
        } else {
            let mut temporary =
                tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
            std::io::copy(
                &mut fs::File::open(&old).map_err(|e| e.to_string())?,
                &mut temporary,
            )
            .map_err(|e| e.to_string())?;
            temporary.flush().map_err(|e| e.to_string())?;
            temporary
                .as_file()
                .set_times(
                    fs::FileTimes::new().set_modified(
                        fs::metadata(&old)
                            .map_err(|e| e.to_string())?
                            .modified()
                            .map_err(|e| e.to_string())?,
                    ),
                )
                .map_err(|e| e.to_string())?;
            temporary.as_file().sync_all().map_err(|e| e.to_string())?;
            temporary
                .persist_noclobber(&backup)
                .map_err(|e| e.to_string())?;
        }

        if crate::disk_sync::revision(&old)? != op.old_revision
            || !crate::disk_sync::equal_files(&old, &backup)?
        {
            return Err("Old file changed while backing up; all copies retained".into());
        }
    }
    verified(&op.new_root, &op.new_relative, &op.new_revision)?;
    Ok(())
}

pub(super) async fn finish(pool: &SqlitePool, mut op: Operation) -> Result<Report, String> {
    if op.committed == 0 {
        let copy = op.clone();
        tauri::async_runtime::spawn_blocking(move || preserve(&copy))
            .await
            .map_err(|e| e.to_string())??;

        let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
        // Location/revision checks are inside the same transaction as retirement/rebinding.
        for (id, source, relative, rev) in [
            (
                op.main_id,
                op.old_source,
                &op.old_relative,
                &op.old_revision,
            ),
            (
                op.stage_id,
                op.new_source,
                &op.new_relative,
                &op.new_revision,
            ),
        ] {
            let valid: bool = sqlx::query_scalar(concat!(
                "SELECT EXISTS(SELECT 1 FROM media WHERE id=? AND source_id=? AND relative_path=? AND ",
                "file_revision=? AND availability='present')",
            ))
.bind(id)
.bind(source)
.bind(relative)
.bind(rev)
.fetch_one(&mut *tx)
.await
.map_err(|e| e.to_string())?;

            if !valid {
                return Err(
                    "Replacement records changed; recovery requires the original records".into(),
                );
            }
        }
        // Snapshot incoming technical values before deleting its record. Main annotations are untouched.
        let incoming = sqlx::query("SELECT * FROM media WHERE id=?")
            .bind(op.stage_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        let video = sqlx::query("SELECT * FROM video_metadata WHERE media_id=?")
            .bind(op.stage_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        let kind: String = incoming.get("kind");

        let target_kind: String = sqlx::query_scalar("SELECT kind FROM media WHERE id=?")
            .bind(op.main_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        if kind != target_kind {
            return Err("Replacement must have the same media type".into());
        }
        sqlx::query("DELETE FROM media WHERE id=?")
            .bind(op.stage_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query(concat!(
            "UPDATE media SET source_id=?,relative_path=?,path_key=?,library_assignment=?,",
            "file_revision=?,extension=?,size_bytes=?,width=?,height=?,file_modified_at=?,",
            "file_created_at=?,metadata_state=?,metadata_version=?,stage_search_key=?,",
            "last_seen_at=?,needs_distribution=0,updated_at=CAST(unixepoch('subsec')*1000 AS ",
            "INTEGER) WHERE id=?",
        ))
        .bind(op.new_source)
        .bind(&op.new_relative)
        .bind(&op.new_relative)
        .bind(&op.library_root)
        .bind(&op.new_revision)
        .bind(incoming.get::<String, _>("extension"))
        .bind(incoming.get::<Option<i64>, _>("size_bytes"))
        .bind(incoming.get::<Option<i64>, _>("width"))
        .bind(incoming.get::<Option<i64>, _>("height"))
        .bind(incoming.get::<Option<i64>, _>("file_modified_at"))
        .bind(incoming.get::<Option<i64>, _>("file_created_at"))
        .bind(incoming.get::<String, _>("metadata_state"))
        .bind(incoming.get::<i64, _>("metadata_version"))
        .bind(name_key(
            &Path::new(&op.new_root)
                .join(&op.new_relative)
                .to_string_lossy()
                .replace('\\', "/"),
        ))
        .bind(incoming.get::<i64, _>("last_seen_at"))
        .bind(op.main_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        if kind == "video" {
            sqlx::query(concat!(
                "INSERT INTO video_metadata(media_id,duration_ms,fps_num,fps_den,codec,bitrate_bps) ",
                "VALUES(?,?,?,?,?,?) ON CONFLICT(media_id) DO UPDATE SET ",
                "duration_ms=excluded.duration_ms,fps_num=excluded.fps_num,fps_den=excluded.fps_den,",
                "codec=excluded.codec,bitrate_bps=excluded.bitrate_bps",
            ))
                .bind(op.main_id)
                .bind(video.as_ref().and_then(|r| r.get::<Option<i64>,_>("duration_ms")))
                .bind(video.as_ref().and_then(|r| r.get::<Option<i64>,_>("fps_num")))
                .bind(video.as_ref().and_then(|r| r.get::<Option<i64>,_>("fps_den")))
                .bind(video.as_ref().and_then(|r| r.get::<Option<String>,_>("codec")))
                .bind(video.as_ref().and_then(|r| r.get::<Option<i64>,_>("bitrate_bps")))
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }
        sqlx::query("UPDATE media_replacements SET committed=1 WHERE id=?")
            .bind(op.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        op.committed = 1;

        tracing::info!(
            operation_id = op.id,
            media_id = op.main_id,
            "Replacement identity committed"
        );
    }

    if op.delete_old != 0 {
        let adopted = record(pool, op.main_id).await?;

        if adopted.get::<i64, _>("source_id") != op.new_source
            || adopted.get::<String, _>("relative_path") != op.new_relative
            || adopted.get::<Option<String>, _>("file_revision").as_deref()
                != Some(&op.new_revision)
        {
            return Err("Main changed before replacement cleanup; old files preserved".into());
        }

        let cleanup = op.clone();
        tauri::async_runtime::spawn_blocking(move || delete_old_files(&cleanup))
            .await
            .map_err(|e| e.to_string())??;
    } else if op.backup_root.is_some() {
        let cleanup = op.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let backup = backup(&cleanup).ok_or("Missing backup")?;

            let metadata = fs::symlink_metadata(&backup).map_err(|e| e.to_string())?;

            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || crate::disk_sync::revision(&backup)? != cleanup.old_revision
            {
                return Err("Replacement backup changed; old file retained".to_string());
            }

            let old = Path::new(&cleanup.old_root).join(&cleanup.old_relative);

            if old.try_exists().map_err(|e| e.to_string())? {
                let old = verified(
                    &cleanup.old_root,
                    &cleanup.old_relative,
                    &cleanup.old_revision,
                )?;

                if !crate::disk_sync::equal_files(&old, &backup)? {
                    return Err("Backup verification failed; old file retained".into());
                }
                verified(
                    &cleanup.new_root,
                    &cleanup.new_relative,
                    &cleanup.new_revision,
                )?;
                fs::remove_file(old).map_err(|e| e.to_string())?;
            }
            Ok::<_, String>(())
        })
        .await
        .map_err(|e| e.to_string())??;
    }
    sqlx::query("UPDATE media_replacements SET finished=1 WHERE id=?")
        .bind(op.id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!(
        operation_id = op.id,
        old_file_deleted = op.delete_old != 0,
        "Replacement completed"
    );
    Ok(Report {
        old_file_deleted: op.delete_old != 0,
    })
}
// Missing old/temporary files are valid crash-recovery boundaries. Validate the
// adopted version before deleting anything, even if the old original is gone.
pub(super) fn delete_old_files(op: &Operation) -> Result<(), String> {
    let incoming = verified(&op.new_root, &op.new_relative, &op.new_revision)?;

    let old = Path::new(&op.old_root).join(&op.old_relative);

    let temporary = backup(op);

    if old.try_exists().map_err(|e| e.to_string())? {
        let old = verified(&op.old_root, &op.old_relative, &op.old_revision)?;

        if old.canonicalize().map_err(|e| e.to_string())?
            == incoming.canonicalize().map_err(|e| e.to_string())?
        {
            return Err("Old and new paths now resolve to the same file; cleanup stopped".into());
        }

        if let Some(path) = &temporary {
            verify_backup(op, path)?;

            if !crate::disk_sync::equal_files(&old, path)? {
                return Err("Temporary backup differs; old version preserved".into());
            }
        }
        verified(&op.new_root, &op.new_relative, &op.new_revision)?;
        fs::remove_file(old).map_err(|_| {
            "Replacement saved; could not delete the old file. Retry disk synchronization."
                .to_string()
        })?;

        tracing::debug!(operation_id = op.id, "Replaced original deleted");
    }

    if let Some(path) = temporary
        && path.try_exists().map_err(|e| e.to_string())?
    {
        verify_backup(op, &path)?;
        verified(&op.new_root, &op.new_relative, &op.new_revision)?;
        fs::remove_file(path)
        .map_err(|_|"Old file deleted; could not remove the temporary replacement backup. Retry disk synchronization.".to_string())?;

        tracing::debug!(operation_id = op.id, "Temporary replacement backup deleted");
    }
    Ok(())
}

pub(super) fn verify_backup(op: &Operation, path: &Path) -> Result<(), String> {
    let root = op.backup_root.as_ref().ok_or("Missing backup directory")?;

    let name = path
        .file_name()
        .and_then(|p| p.to_str())
        .ok_or("Invalid backup filename")?;
    verified(root, name, &op.old_revision).map_err(|_| {
        "Temporary replacement backup is missing or changed; cleanup stopped".to_string()
    })?;
    Ok(())
}

pub(crate) async fn recover(pool: &SqlitePool) -> Result<(), String> {
    let pending: Vec<Operation> =
        sqlx::query_as("SELECT * FROM media_replacements WHERE finished=0 ORDER BY id")
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;

    for op in pending {
        tracing::warn!(
            operation_id = op.id,
            "Recovering interrupted media replacement"
        );

        let operation_id = op.id;

        if let Err(error) = finish(pool, op).await {
            if abandon_uncommitted(pool, operation_id).await? {
                tracing::warn!(
                    operation_id,
                    "Uncommitted replacement abandoned; originals and any backup retained"
                );
            } else {
                return Err(error);
            }
        }
    }
    Ok(())
}

pub(super) async fn begin(
    pool: &SqlitePool,
    settings: &AppConfig,
    backups: &Path,
    request: Request,
) -> Result<Report, String> {
    let main_id = id(&request.main_id)?;

    let stage_id = id(&request.stage_id)?;

    if main_id == stage_id {
        return Err("Cannot replace a file with itself".into());
    }

    let main = record(pool, main_id).await?;

    let incoming = record(pool, stage_id).await?;

    if !is_main(&main, settings) || !is_stage(&incoming, settings) {
        return Err("Files are outside the current Main/Stage selection".into());
    }

    if token(&main) != request.main_token
        || token(&incoming) != request.stage_token
        || settings.sync_structure != request.sync_with_disk
    {
        return Err("Files or synchronization settings changed; reload before replacing".into());
    }

    if main.get::<String, _>("kind") != incoming.get::<String, _>("kind") {
        return Err("Replacement must have the same media type".into());
    }

    let old_revision = main
        .get::<Option<String>, _>("file_revision")
        .ok_or("Rescan Main before replacement")?;

    let new_revision = incoming
        .get::<Option<String>, _>("file_revision")
        .ok_or("Rescan Stage before replacement")?;

    let mut op = Operation {
        id: 0,
        main_id,
        stage_id,
        old_source: main.get("source_id"),
        old_root: main.get("root_path"),
        old_relative: main.get("relative_path"),
        old_revision,
        new_source: incoming.get("source_id"),
        new_root: incoming.get("root_path"),
        new_relative: incoming.get("relative_path"),
        new_revision,
        library_root: root(settings)?,
        backup_root: settings
            .sync_structure
            .then(|| backups.to_string_lossy().into_owned()),
        committed: 0,
        delete_old: 1,
    };
    // Reject changed bytes/alias paths before creating durable intent.
    let check = op.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let a = verified(&check.old_root, &check.old_relative, &check.old_revision)?;

        let b = verified(&check.new_root, &check.new_relative, &check.new_revision)?;

        if a.canonicalize().map_err(|e| e.to_string())?
            == b.canonicalize().map_err(|e| e.to_string())?
        {
            return Err("Cannot replace a file with itself".into());
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    op.id = sqlx::query_scalar(concat!(
        "INSERT INTO media_replacements(main_id,stage_id,old_source,old_root,old_relative,",
        "old_revision,new_source,new_root,new_relative,new_revision,incoming_filename,",
        "library_root,backup_root,delete_old) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,1) RETURNING id",
    ))
    .bind(main_id)
    .bind(stage_id)
    .bind(op.old_source)
    .bind(&op.old_root)
    .bind(&op.old_relative)
    .bind(&op.old_revision)
    .bind(op.new_source)
    .bind(&op.new_root)
    .bind(&op.new_relative)
    .bind(&op.new_revision)
    .bind(incoming.get::<String, _>("original_filename"))
    .bind(&op.library_root)
    .bind(&op.backup_root)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    let operation_id = op.id;

    let result = finish(pool, op).await;

    if result.is_err() && abandon_uncommitted(pool, operation_id).await? {
        tracing::warn!(
            operation_id,
            "Replacement cancelled before commit; originals retained"
        );
    }
    result
}

pub(super) async fn abandon_uncommitted(
    pool: &SqlitePool,
    operation_id: i64,
) -> Result<bool, String> {
    Ok(
        sqlx::query("UPDATE media_replacements SET finished=1 WHERE id=? AND committed=0")
            .bind(operation_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected()
            == 1,
    )
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub async fn replace_library_file(
    app: tauri::AppHandle,
    request: Request,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
    state: tauri::State<'_, StageState>,
) -> Result<Report, String> {
    let _guard = state.operation().await;
    crate::disk_sync::recover(database.pool()).await?;

    tracing::info!("Media replacement started");

    let backups = config
        .paths
        .database_path
        .parent()
        .and_then(Path::parent)
        .ok_or("Invalid database directory")?
        .join("replacement-backups");

    let result = begin(
        database.pool(),
        &config.snapshot()?.settings,
        &backups,
        request,
    )
    .await;
    // Also retries a committed replacement whose cleanup failed, even with sync disabled.
    crate::disk_sync::request(&app);
    result
}
