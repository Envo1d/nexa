//! Recoverable content adoption: keep Main identity and never overwrite media bytes.
use crate::{
    config::{AppConfig, ConfigStore},
    database::Database,
    legacy_import::name_key,
    stage::{self, StageState},
};
use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, QueryBuilder, Row, Sqlite, SqlitePool, sqlite::SqliteRow};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const SELECT: &str = "SELECT m.*,s.root_path,s.kind source_kind,v.duration_ms,v.fps_num,v.fps_den,v.codec,v.bitrate_bps,v.title,a.name artist_name FROM media m JOIN sources s ON s.id=m.source_id LEFT JOIN video_metadata v ON v.media_id=m.id LEFT JOIN artists a ON a.id=m.artist_id";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    #[serde(flatten)]
    file: stage::StageFile,
    token: String,
    artist: Option<String>,
    video_title: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    file: Candidate,
    sync_with_disk: bool,
    has_annotations: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidates {
    files: Vec<Candidate>,
    next: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    main_id: String,
    stage_id: String,
    main_token: String,
    stage_token: String,
    sync_with_disk: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    old_file_deleted: bool,
}
fn id(value: &str) -> Result<i64, String> {
    value
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or("Invalid media ID".into())
}
fn token(row: &SqliteRow) -> String {
    serde_json::to_string(&(
        row.get::<i64, _>("id"),
        row.get::<i64, _>("source_id"),
        row.get::<String, _>("root_path"),
        row.get::<String, _>("relative_path"),
        row.get::<Option<String>, _>("file_revision"),
        row.get::<String, _>("kind"),
    ))
    .expect("Serializable identity")
}
fn candidate(row: SqliteRow) -> Candidate {
    Candidate {
        token: token(&row),
        artist: row.get("artist_name"),
        video_title: row.get("title"),
        file: stage::stage_file(row),
    }
}
async fn record(pool: &SqlitePool, value: i64) -> Result<SqliteRow, String> {
    // Only the constant SELECT fragment is interpolated; IDs remain bound values.
    sqlx::query(AssertSqlSafe(format!("{SELECT} WHERE m.id=?")))
        .bind(value)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Media is unavailable".into())
}
fn root(settings: &AppConfig) -> Result<String, String> {
    settings
        .library_path
        .as_ref()
        .map(|p| {
            p.components()
                .collect::<PathBuf>()
                .to_string_lossy()
                .into_owned()
        })
        .ok_or("Configure Main Library first".into())
}
fn is_main(row: &SqliteRow, settings: &AppConfig) -> bool {
    let Ok(library) = root(settings) else {
        return false;
    };
    let physical: String = row.get("root_path");
    let assigned = row
        .get::<Option<String>, _>("library_assignment")
        .as_deref()
        == Some(&library);
    let in_stage = settings.staging_path.as_ref().is_some_and(|p| {
        Path::new(&physical)
            .join(row.get::<String, _>("relative_path"))
            .starts_with(p)
    });
    row.get::<String, _>("availability") == "present"
        && row.get::<i64, _>("needs_distribution") == 0
        && (assigned
            || row.get::<String, _>("source_kind") == "library" && physical == library && !in_stage)
}
fn is_stage(row: &SqliteRow, settings: &AppConfig) -> bool {
    row.get::<String, _>("availability") == "present"
        && (row.get::<i64, _>("needs_distribution") != 0
            || row.get::<String, _>("source_kind") == "stage"
                && row.get::<Option<String>, _>("library_assignment").is_none()
                && settings
                    .staging_path
                    .as_ref()
                    .is_some_and(|p| p == Path::new(&row.get::<String, _>("root_path"))))
}

#[tauri::command]
pub async fn get_replacement_source(
    id: String,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<Source, String> {
    let id = self::id(&id)?;
    let settings = config.snapshot()?.settings;
    root(&settings)?;
    let row = record(database.pool(), id).await?;
    if !is_stage(&row, &settings) {
        return Err("Select a present Stage file".into());
    }
    let has_annotations: bool = sqlx::query_scalar("SELECT artist_id IS NOT NULL OR favorite=1 OR EXISTS(SELECT 1 FROM media_tags WHERE media_id=m.id) OR EXISTS(SELECT 1 FROM media_copyrights WHERE media_id=m.id) OR EXISTS(SELECT 1 FROM media_characters WHERE media_id=m.id) OR EXISTS(SELECT 1 FROM video_metadata WHERE media_id=m.id AND title IS NOT NULL) FROM media m WHERE id=?")
        .bind(id).fetch_one(database.pool()).await.map_err(|e| e.to_string())?;
    Ok(Source {
        file: candidate(row),
        sync_with_disk: settings.sync_structure,
        has_annotations,
    })
}

// The independent picker searches the full Main database, not the current view/page.
async fn candidates(
    pool: &SqlitePool,
    settings: &AppConfig,
    kind: &str,
    search: &str,
    after: i64,
) -> Result<Candidates, String> {
    if !matches!(kind, "image" | "video") || after < 0 || search.len() > 512 {
        return Err("Invalid replacement search".into());
    }
    let library = root(settings)?;
    let stage_prefix = settings.staging_path.as_ref().map(|p| {
        format!(
            "{}/",
            p.to_string_lossy().replace('\\', "/").trim_end_matches('/')
        )
    });
    let mut query = QueryBuilder::<Sqlite>::new(SELECT);
    query
        .push(" WHERE m.id>")
        .push_bind(after)
        .push(" AND m.kind=")
        .push_bind(kind)
        .push(" AND m.availability='present' AND m.needs_distribution=0 AND (m.library_assignment=")
        .push_bind(&library)
        .push(" OR (s.kind='library' AND s.path_key=")
        .push_bind(&library)
        .push("))");
    if let Some(prefix) = &stage_prefix {
        query
            .push(" AND (m.library_assignment=")
            .push_bind(&library)
            .push(" OR substr(replace(s.root_path || '/' || m.relative_path,char(92),'/'),1,")
            .push_bind(prefix.chars().count() as i64)
            .push(") COLLATE NOCASE != ")
            .push_bind(prefix)
            .push(")");
    }
    crate::library::apply_metadata_search(&mut query, search);
    query.push(" ORDER BY m.id LIMIT 51");
    tracing::debug!(
        kind,
        after,
        "Loading replacement candidates using Main metadata filters"
    );
    let mut rows = query
        .build()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    let more = rows.len() > 50;
    rows.truncate(50);
    let next = if more {
        rows.last().map(|r| r.get::<i64, _>("id").to_string())
    } else {
        None
    };
    Ok(Candidates {
        files: rows.into_iter().map(candidate).collect(),
        next,
    })
}
#[tauri::command]
pub async fn get_replacement_candidates(
    kind: String,
    search: String,
    after: Option<String>,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<Candidates, String> {
    candidates(
        database.pool(),
        &config.snapshot()?.settings,
        &kind,
        &search,
        after.as_deref().map(id).transpose()?.unwrap_or(0),
    )
    .await
}

#[derive(Clone, sqlx::FromRow)]
struct Operation {
    id: i64,
    main_id: i64,
    stage_id: i64,
    old_source: i64,
    old_root: String,
    old_relative: String,
    old_revision: String,
    new_source: i64,
    new_root: String,
    new_relative: String,
    new_revision: String,
    library_root: String,
    backup_root: Option<String>,
    committed: i64,
    delete_old: i64,
}
fn backup(op: &Operation) -> Option<PathBuf> {
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
fn verified(root: &str, relative: &str, expected: &str) -> Result<PathBuf, String> {
    let path = stage::checked_action_path(Path::new(root), relative)?;
    if crate::disk_sync::revision(&path)? != expected {
        return Err("File changed; rescan and select both versions again".into());
    }
    Ok(path)
}
fn preserve(op: &Operation) -> Result<(), String> {
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
async fn finish(pool: &SqlitePool, mut op: Operation) -> Result<Report, String> {
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
            let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM media WHERE id=? AND source_id=? AND relative_path=? AND file_revision=? AND availability='present')").bind(id).bind(source).bind(relative).bind(rev).fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
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
        sqlx::query("UPDATE media SET source_id=?,relative_path=?,path_key=?,library_assignment=?,file_revision=?,extension=?,size_bytes=?,width=?,height=?,file_modified_at=?,file_created_at=?,metadata_state=?,metadata_version=?,stage_search_key=?,last_seen_at=?,needs_distribution=0,updated_at=CAST(unixepoch('subsec')*1000 AS INTEGER) WHERE id=?")
            .bind(op.new_source).bind(&op.new_relative).bind(&op.new_relative).bind(&op.library_root).bind(&op.new_revision)
            .bind(incoming.get::<String,_>("extension")).bind(incoming.get::<Option<i64>,_>("size_bytes")).bind(incoming.get::<Option<i64>,_>("width")).bind(incoming.get::<Option<i64>,_>("height"))
            .bind(incoming.get::<Option<i64>,_>("file_modified_at")).bind(incoming.get::<Option<i64>,_>("file_created_at")).bind(incoming.get::<String,_>("metadata_state")).bind(incoming.get::<i64,_>("metadata_version"))
            .bind(name_key(&Path::new(&op.new_root).join(&op.new_relative).to_string_lossy().replace('\\', "/"))).bind(incoming.get::<i64,_>("last_seen_at")).bind(op.main_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        if kind == "video" {
            sqlx::query("INSERT INTO video_metadata(media_id,duration_ms,fps_num,fps_den,codec,bitrate_bps) VALUES(?,?,?,?,?,?) ON CONFLICT(media_id) DO UPDATE SET duration_ms=excluded.duration_ms,fps_num=excluded.fps_num,fps_den=excluded.fps_den,codec=excluded.codec,bitrate_bps=excluded.bitrate_bps")
                .bind(op.main_id).bind(video.as_ref().and_then(|r| r.get::<Option<i64>,_>("duration_ms"))).bind(video.as_ref().and_then(|r| r.get::<Option<i64>,_>("fps_num"))).bind(video.as_ref().and_then(|r| r.get::<Option<i64>,_>("fps_den"))).bind(video.as_ref().and_then(|r| r.get::<Option<String>,_>("codec"))).bind(video.as_ref().and_then(|r| r.get::<Option<i64>,_>("bitrate_bps"))).execute(&mut *tx).await.map_err(|e| e.to_string())?;
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
fn delete_old_files(op: &Operation) -> Result<(), String> {
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
        fs::remove_file(path).map_err(|_|"Old file deleted; could not remove the temporary replacement backup. Retry disk synchronization.".to_string())?;
        tracing::debug!(operation_id = op.id, "Temporary replacement backup deleted");
    }
    Ok(())
}
fn verify_backup(op: &Operation, path: &Path) -> Result<(), String> {
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
async fn begin(
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
    op.id=sqlx::query_scalar("INSERT INTO media_replacements(main_id,stage_id,old_source,old_root,old_relative,old_revision,new_source,new_root,new_relative,new_revision,incoming_filename,library_root,backup_root,delete_old) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,1) RETURNING id")
        .bind(main_id).bind(stage_id).bind(op.old_source).bind(&op.old_root).bind(&op.old_relative).bind(&op.old_revision).bind(op.new_source).bind(&op.new_root).bind(&op.new_relative).bind(&op.new_revision).bind(incoming.get::<String,_>("original_filename")).bind(&op.library_root).bind(&op.backup_root).fetch_one(pool).await.map_err(|e|e.to_string())?;
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
async fn abandon_uncommitted(pool: &SqlitePool, operation_id: i64) -> Result<bool, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        temp: tempfile::TempDir,
        db: Database,
        settings: AppConfig,
    }
    impl Fixture {
        fn old(&self) -> PathBuf {
            self.settings.library_path.as_ref().unwrap().join("old.mp4")
        }
        fn incoming(&self) -> PathBuf {
            self.settings
                .staging_path
                .as_ref()
                .unwrap()
                .join("full.webm")
        }
        fn backups(&self) -> PathBuf {
            self.temp.path().join("backups")
        }
        async fn request(&self) -> Request {
            Request {
                main_id: "1".into(),
                stage_id: "2".into(),
                main_token: token(&record(self.db.pool(), 1).await.unwrap()),
                stage_token: token(&record(self.db.pool(), 2).await.unwrap()),
                sync_with_disk: self.settings.sync_structure,
            }
        }
    }
    async fn fixture(sync: bool) -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("Library");
        let stage = temp.path().join("Stage");
        fs::create_dir(&library).unwrap();
        fs::create_dir(&stage).unwrap();
        fs::write(library.join("old.mp4"), b"old partial video").unwrap();
        fs::write(stage.join("full.webm"), b"complete new video contents").unwrap();
        let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
        sqlx::query("INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(1,'library',?,?,1),(2,'stage',?,?,1)").bind(library.to_str().unwrap()).bind(library.to_str().unwrap()).bind(stage.to_str().unwrap()).bind(stage.to_str().unwrap()).execute(db.pool()).await.unwrap();
        sqlx::raw_sql("INSERT INTO artists VALUES(1,'Artist','artist'); INSERT INTO tags VALUES(1,'Keep','keep'); INSERT INTO copyrights VALUES(1,'Copyright','copyright'); INSERT INTO characters VALUES(1,1,'Character','character');").execute(db.pool()).await.unwrap();
        for (id, source, path, filename, width) in [
            (1, 1, library.join("old.mp4"), "old.mp4", 1280),
            (2, 2, stage.join("full.webm"), "full.webm", 1920),
        ] {
            sqlx::query("INSERT INTO media(id,source_id,relative_path,path_key,original_filename,kind,extension,size_bytes,width,height,file_revision,metadata_state,metadata_version,last_seen_at,created_at,updated_at,stage_search_key) VALUES(?,?,?,?,?,'video',?,?,?,?,?,'ready',1,1,1,1,?)").bind(id).bind(source).bind(filename).bind(filename).bind(filename).bind(if id==1 {"mp4"} else {"webm"}).bind(fs::metadata(&path).unwrap().len() as i64).bind(width).bind(1080).bind(crate::disk_sync::revision(&path).unwrap()).bind(name_key(filename)).execute(db.pool()).await.unwrap();
        }
        sqlx::raw_sql("UPDATE media SET artist_id=1,favorite=1,naming_order_key='group',naming_order=3 WHERE id=1; INSERT INTO media_tags VALUES(1,1); INSERT INTO media_copyrights VALUES(1,1); INSERT INTO media_characters VALUES(1,1,1); INSERT INTO video_metadata(media_id,title,duration_ms,codec) VALUES(1,'Keep title',1000,'old'),(2,'Incoming title',8000,'new');").execute(db.pool()).await.unwrap();
        Fixture {
            temp,
            db,
            settings: AppConfig {
                library_path: Some(library),
                staging_path: Some(stage),
                sync_structure: sync,
                ..Default::default()
            },
        }
    }
    async fn pending(f: &Fixture) -> Operation {
        let a = record(f.db.pool(), 1).await.unwrap();
        let b = record(f.db.pool(), 2).await.unwrap();
        sqlx::query("INSERT INTO media_replacements(main_id,stage_id,old_source,old_root,old_relative,old_revision,new_source,new_root,new_relative,new_revision,incoming_filename,library_root,backup_root,delete_old) VALUES(1,2,1,?,'old.mp4',?,2,?,'full.webm',?,'full.webm',?,?,1)")
            .bind(a.get::<String,_>("root_path")).bind(a.get::<Option<String>,_>("file_revision")).bind(b.get::<String,_>("root_path")).bind(b.get::<Option<String>,_>("file_revision")).bind(root(&f.settings).unwrap()).bind(f.settings.sync_structure.then(||f.backups().to_string_lossy().into_owned())).execute(f.db.pool()).await.unwrap();
        sqlx::query_as("SELECT * FROM media_replacements ORDER BY id DESC LIMIT 1")
            .fetch_one(f.db.pool())
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn replacement_retains_main_identity_annotations_order_and_refreshes_technical_fields() {
        let f = fixture(false).await;
        let before = token(&record(f.db.pool(), 1).await.unwrap());
        let report = begin(f.db.pool(), &f.settings, &f.backups(), f.request().await)
            .await
            .unwrap();
        assert!(report.old_file_deleted);
        assert!(!f.old().exists());
        assert_eq!(
            fs::read(f.incoming()).unwrap(),
            b"complete new video contents"
        );
        let row = record(f.db.pool(), 1).await.unwrap();
        assert_ne!(token(&row), before);
        assert_eq!(row.get::<i64, _>("source_id"), 2);
        assert_eq!(row.get::<String, _>("original_filename"), "old.mp4");
        assert_eq!(row.get::<String, _>("extension"), "webm");
        assert_eq!(row.get::<i64, _>("width"), 1920);
        assert_eq!(row.get::<i64, _>("favorite"), 1);
        assert_eq!(row.get::<i64, _>("artist_id"), 1);
        assert_eq!(row.get::<String, _>("title"), "Keep title");
        assert_eq!(row.get::<i64, _>("duration_ms"), 8000);
        assert_eq!(row.get::<String, _>("codec"), "new");
        assert_eq!(row.get::<i64, _>("naming_order"), 3);
        assert_eq!(row.get::<String, _>("naming_order_key"), "group");
        let links:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM media_tags WHERE media_id=1)+(SELECT count(*) FROM media_characters WHERE media_id=1)+(SELECT count(*) FROM media_copyrights WHERE media_id=1)").fetch_one(f.db.pool()).await.unwrap();
        assert_eq!(links, 3);
        assert!(record(f.db.pool(), 2).await.is_err());
        assert_eq!(
            stage::browse(
                f.db.pool(),
                Some(
                    f.settings
                        .staging_path
                        .as_ref()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                ),
                None,
                ""
            )
            .await
            .unwrap()
            .total,
            0
        );
        assert_eq!(
            candidates(f.db.pool(), &f.settings, "video", "", 0)
                .await
                .unwrap()
                .files
                .len(),
            1
        );
        f.db.close().await;
        let reopened = Database::open(&f.temp.path().join("vault.db"))
            .await
            .unwrap();
        recover(reopened.pool()).await.unwrap();
        assert_eq!(
            record(reopened.pool(), 1)
                .await
                .unwrap()
                .get::<String, _>("title"),
            "Keep title"
        );
        reopened.close().await;
    }
    #[tokio::test]
    async fn enabled_sync_removes_old_file_and_temporary_backup_after_commit() {
        let f = fixture(true).await;
        let report = begin(f.db.pool(), &f.settings, &f.backups(), f.request().await)
            .await
            .unwrap();
        assert!(!f.old().exists());
        assert!(report.old_file_deleted);
        assert_eq!(fs::read_dir(f.backups()).unwrap().count(), 0);
        assert_eq!(
            fs::read(f.incoming()).unwrap(),
            b"complete new video contents"
        );
        f.db.close().await;
    }
    #[tokio::test]
    async fn cleanup_keeps_original_and_temporary_backup_until_adopted_file_is_available() {
        let f = fixture(true).await;
        let op = pending(&f).await;
        preserve(&op).unwrap();
        // Crash boundary: adoption committed, filesystem cleanup has not run.
        let mut tx = f.db.pool().begin().await.unwrap();
        sqlx::query("DELETE FROM media WHERE id=2")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE media SET source_id=2,relative_path='full.webm',path_key='full.webm',file_revision=? WHERE id=1").bind(&op.new_revision).execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE media_replacements SET committed=1")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let moved = f.temp.path().join("held.webm");
        fs::rename(f.incoming(), &moved).unwrap();
        assert!(recover(f.db.pool()).await.is_err());
        assert_eq!(fs::read(f.old()).unwrap(), b"old partial video");
        assert_eq!(
            fs::read(backup(&op).unwrap()).unwrap(),
            b"old partial video"
        );
        fs::rename(moved, f.incoming()).unwrap();
        f.db.close().await;
        let db = Database::open(&f.temp.path().join("vault.db"))
            .await
            .unwrap();
        recover(db.pool()).await.unwrap();
        assert!(!f.old().exists());
        assert!(!backup(&op).unwrap().exists());
        assert_eq!(
            fs::read(f.incoming()).unwrap(),
            b"complete new video contents"
        );
        // Completion is also recoverable after both cleanup deletions.
        sqlx::query("UPDATE media_replacements SET finished=0")
            .execute(db.pool())
            .await
            .unwrap();
        recover(db.pool()).await.unwrap();
        db.close().await;
    }
    #[tokio::test]
    async fn existing_retention_policy_does_not_retroactively_delete_old_versions() {
        let f = fixture(false).await;
        pending(&f).await;
        sqlx::query("UPDATE media_replacements SET delete_old=0")
            .execute(f.db.pool())
            .await
            .unwrap();
        recover(f.db.pool()).await.unwrap();
        assert_eq!(fs::read(f.old()).unwrap(), b"old partial video");
        assert_eq!(
            record(f.db.pool(), 1)
                .await
                .unwrap()
                .get::<i64, _>("source_id"),
            2
        );
        f.db.close().await;
    }
    #[tokio::test]
    async fn replaces_images_across_formats_without_creating_video_metadata() {
        let f = fixture(false).await;
        let old = f.settings.library_path.as_ref().unwrap().join("old.png");
        let new = f.settings.staging_path.as_ref().unwrap().join("new.webp");
        fs::rename(f.old(), &old).unwrap();
        fs::rename(f.incoming(), &new).unwrap();
        sqlx::raw_sql("DELETE FROM video_metadata; UPDATE media SET kind='image',extension='png',relative_path='old.png',path_key='old.png',original_filename='old.png' WHERE id=1; UPDATE media SET kind='image',extension='webp',relative_path='new.webp',path_key='new.webp',original_filename='new.webp' WHERE id=2;").execute(f.db.pool()).await.unwrap();
        begin(f.db.pool(), &f.settings, &f.backups(), f.request().await)
            .await
            .unwrap();
        let row = record(f.db.pool(), 1).await.unwrap();
        assert_eq!(row.get::<String, _>("kind"), "image");
        assert_eq!(row.get::<String, _>("extension"), "webp");
        assert_eq!(row.get::<String, _>("original_filename"), "old.png");
        assert_eq!(row.get::<i64, _>("width"), 1920);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM video_metadata")
            .fetch_one(f.db.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
        assert!(!old.exists());
        assert!(new.exists());
        f.db.close().await;
    }
    #[tokio::test]
    async fn changed_incoming_abandons_uncommitted_recovery_without_blocking_scans() {
        let f = fixture(false).await;
        pending(&f).await;
        fs::write(f.incoming(), b"changed after intent").unwrap();
        recover(f.db.pool()).await.unwrap();
        assert_eq!(
            record(f.db.pool(), 1)
                .await
                .unwrap()
                .get::<i64, _>("source_id"),
            1
        );
        assert!(record(f.db.pool(), 2).await.is_ok());
        let row = sqlx::query("SELECT committed,finished FROM media_replacements")
            .fetch_one(f.db.pool())
            .await
            .unwrap();
        assert_eq!(row.get::<i64, _>("committed"), 0);
        assert_eq!(row.get::<i64, _>("finished"), 1);
        assert_eq!(fs::read(f.old()).unwrap(), b"old partial video");
        f.db.close().await;
    }
    #[tokio::test]
    async fn rejects_stale_tokens_settings_kinds_and_changed_bytes_without_retiring_stage() {
        let f = fixture(false).await;
        let mut request = f.request().await;
        request.main_token = "stale".into();
        assert!(
            begin(f.db.pool(), &f.settings, &f.backups(), request)
                .await
                .is_err()
        );
        let mut request = f.request().await;
        request.sync_with_disk = true;
        assert!(
            begin(f.db.pool(), &f.settings, &f.backups(), request)
                .await
                .is_err()
        );
        let request = f.request().await;
        fs::write(f.incoming(), b"changed source").unwrap();
        assert!(
            begin(f.db.pool(), &f.settings, &f.backups(), request)
                .await
                .is_err()
        );
        sqlx::raw_sql(
            "DELETE FROM video_metadata WHERE media_id=2; UPDATE media SET kind='image' WHERE id=2",
        )
        .execute(f.db.pool())
        .await
        .unwrap();
        assert!(
            begin(f.db.pool(), &f.settings, &f.backups(), f.request().await)
                .await
                .is_err()
        );
        assert!(record(f.db.pool(), 2).await.is_ok());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM media_replacements")
            .fetch_one(f.db.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(fs::read(f.old()).unwrap(), b"old partial video");
        f.db.close().await;
    }
    #[tokio::test]
    async fn recovers_intent_and_existing_backup_after_restart_without_duplicate_records() {
        for backed_up in [false, true] {
            let f = fixture(true).await;
            let op = pending(&f).await;
            if backed_up {
                preserve(&op).unwrap();
            }
            f.db.close().await;
            let db = Database::open(&f.temp.path().join("vault.db"))
                .await
                .unwrap();
            recover(db.pool()).await.unwrap();
            recover(db.pool()).await.unwrap();
            assert!(!f.old().exists());
            assert!(!backup(&op).unwrap().exists());
            assert!(record(db.pool(), 2).await.is_err());
            assert_eq!(
                record(db.pool(), 1)
                    .await
                    .unwrap()
                    .get::<i64, _>("source_id"),
                2
            );
            db.close().await;
        }
    }
    #[tokio::test]
    async fn failed_database_commit_rolls_back_stage_retirement_and_keeps_originals() {
        let f = fixture(true).await;
        sqlx::raw_sql("CREATE TRIGGER reject_replacement BEFORE UPDATE OF source_id ON media WHEN OLD.id=1 BEGIN SELECT RAISE(ABORT,'test commit failure'); END;").execute(f.db.pool()).await.unwrap();
        assert!(
            begin(f.db.pool(), &f.settings, &f.backups(), f.request().await)
                .await
                .is_err()
        );
        assert!(record(f.db.pool(), 2).await.is_ok());
        assert_eq!(
            record(f.db.pool(), 1)
                .await
                .unwrap()
                .get::<i64, _>("source_id"),
            1
        );
        assert_eq!(fs::read(f.old()).unwrap(), b"old partial video");
        let finished: i64 = sqlx::query_scalar("SELECT finished FROM media_replacements")
            .fetch_one(f.db.pool())
            .await
            .unwrap();
        assert_eq!(finished, 1);
        f.db.close().await;
    }
    #[tokio::test]
    async fn committed_cleanup_failure_recovers_after_reopen_and_preserves_changed_old_file() {
        let f = fixture(true).await;
        let op = pending(&f).await;
        let old_time = fs::metadata(f.old()).unwrap().modified().unwrap();
        finish(f.db.pool(), op.clone()).await.unwrap();
        // Restore the transient copy to represent the postcommit cleanup boundary.
        let temporary = backup(&op).unwrap();
        fs::write(&temporary, b"old partial video").unwrap();
        fs::File::options()
            .write(true)
            .open(&temporary)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old_time))
            .unwrap();
        // Simulate a crash just after commit, with the old file still present.
        fs::write(f.old(), b"externally changed old file").unwrap();
        sqlx::query("UPDATE media_replacements SET finished=0")
            .execute(f.db.pool())
            .await
            .unwrap();
        assert!(recover(f.db.pool()).await.is_err());
        assert_eq!(fs::read(f.old()).unwrap(), b"externally changed old file");
        assert_eq!(fs::read(&temporary).unwrap(), b"old partial video");
        fs::remove_file(f.old()).unwrap();
        f.db.close().await;
        let db = Database::open(&f.temp.path().join("vault.db"))
            .await
            .unwrap();
        recover(db.pool()).await.unwrap();
        assert!(!temporary.exists());
        assert_eq!(
            record(db.pool(), 1)
                .await
                .unwrap()
                .get::<i64, _>("source_id"),
            2
        );
        db.close().await;
    }
    #[tokio::test]
    async fn picker_searches_metadata_and_pages_full_main_independent_of_stage() {
        let f = fixture(false).await;
        for search in ["Artist", "Character", "Keep", "Copyright"] {
            assert_eq!(
                candidates(f.db.pool(), &f.settings, "video", search, 0)
                    .await
                    .unwrap()
                    .files
                    .len(),
                1,
                "{search}"
            );
        }
        assert!(
            candidates(f.db.pool(), &f.settings, "video", "full", 0)
                .await
                .unwrap()
                .files
                .is_empty()
        );
        assert!(
            candidates(f.db.pool(), &f.settings, "image", "", 0)
                .await
                .unwrap()
                .files
                .is_empty()
        );
        for value in 3..=56 {
            let filename = format!("item-{value}.mp4");
            sqlx::query("INSERT INTO media(id,source_id,relative_path,path_key,original_filename,kind,extension,last_seen_at,created_at,updated_at) VALUES(?,1,?,?,?,'video','mp4',1,1,1)").bind(value).bind(&filename).bind(&filename).bind(&filename).execute(f.db.pool()).await.unwrap();
        }
        let first = candidates(f.db.pool(), &f.settings, "video", "", 0)
            .await
            .unwrap();
        assert_eq!(first.files.len(), 50);
        let second = candidates(
            f.db.pool(),
            &f.settings,
            "video",
            "",
            id(first.next.as_deref().unwrap()).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(second.files.len(), 5);
        assert!(second.next.is_none());
        sqlx::query("UPDATE artists SET name='Тестовый Художник',name_key=? WHERE id=1")
            .bind(name_key("Тестовый Художник"))
            .execute(f.db.pool())
            .await
            .unwrap();
        assert_eq!(
            candidates(
                f.db.pool(),
                &f.settings,
                "video",
                "тестовый художник; CHARACTER",
                0
            )
            .await
            .unwrap()
            .files
            .len(),
            1
        );
        f.db.close().await;
    }
    #[tokio::test]
    async fn replacement_metadata_search_matches_main_and_requires_all_terms_on_one_file() {
        let f = fixture(false).await;
        sqlx::raw_sql("INSERT INTO artists VALUES(2,'Other','other'); INSERT INTO media(id,source_id,relative_path,path_key,original_filename,kind,extension,artist_id,last_seen_at,created_at,updated_at) VALUES(3,1,'artist-only.mp4','artist-only.mp4','artist-only.mp4','video','mp4',1,1,1,1),(4,1,'character-only.mp4','character-only.mp4','character-only.mp4','video','mp4',2,1,1,1); INSERT INTO media_copyrights VALUES(4,1); INSERT INTO media_characters VALUES(4,1,1);").execute(f.db.pool()).await.unwrap();
        for (search, expected) in [
            ("Artist; Character", 1),
            (" artist ; CHARACTER; Keep; Copyright ", 1),
            ("Artist;Artist; ;Character;", 1),
            ("Artist; Other", 0),
            ("Art", 0),
            ("old", 0),
            ("Keep title", 0),
            ("Other;Keep", 0),
            ("Artist", 2),
            ("Character", 2),
            (" ; ; ", 3),
        ] {
            let result = candidates(f.db.pool(), &f.settings, "video", search, 0)
                .await
                .unwrap();
            assert_eq!(result.files.len(), expected, "{search}");
            let filters=serde_json::from_value(serde_json::json!({"scope":"gallery","search":search,"kind":"video","sort":"newest","tagged":false,"offset":0})).unwrap();
            let main =
                crate::library::browse(f.db.pool(), Some(root(&f.settings).unwrap()), filters)
                    .await
                    .unwrap();
            let main = serde_json::to_value(main).unwrap();
            assert_eq!(
                main["total"].as_u64().unwrap() as usize,
                expected,
                "Main comparison: {search}"
            );
        }
        f.db.close().await;
    }
}
