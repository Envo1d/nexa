//! Bounded Stage indexing. Shell/filesystem work never runs in write transactions.
use crate::{
    config::ConfigStore,
    database::Database,
    media_metadata::{self, MediaKind, MediaMetadata},
    preview_cache::{PreviewCache, PreviewOrigin},
};
use serde::Serialize;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};

const BATCH: usize = 64;
const EXTRACTOR_VERSION: i64 = 1;

struct FileActionGuard<'a>(&'a AtomicBool);
impl Drop for FileActionGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn checked_action_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    use std::path::Component;
    if !root.is_absolute() {
        return Err("Source folder must be absolute".into());
    }
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Invalid stored file path".into());
    }
    let path = root.join(relative);
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| "File is unavailable on disk")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Only regular files are supported".into());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|_| "Source folder is unavailable")?;
    let canonical = path
        .canonicalize()
        .map_err(|_| "File is unavailable on disk")?;
    if !canonical.starts_with(canonical_root) {
        return Err("File resolves outside its source folder".into());
    }
    Ok(path)
}

#[cfg(windows)]
fn recycle_file(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::UI::Shell::{
            FO_DELETE, FOF_ALLOWUNDO, FOF_NO_CONNECTED_ELEMENTS, FOF_NOCONFIRMATION,
            FOF_WANTNUKEWARNING, SHFILEOPSTRUCTW, SHFileOperationW,
        },
        core::PCWSTR,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();
    if wide[..wide.len() - 2].contains(&0) {
        return Err("Invalid file path".into());
    }
    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(wide.as_ptr()),
        fFlags: (FOF_ALLOWUNDO.0
            | FOF_NOCONFIRMATION.0
            | FOF_WANTNUKEWARNING.0
            | FOF_NO_CONNECTED_ELEMENTS.0) as u16,
        ..Default::default()
    };
    // The fully qualified, double-NUL path lives through the synchronous Shell call.
    let result = unsafe { SHFileOperationW(&mut operation) };
    if result != 0 || operation.fAnyOperationsAborted.as_bool() {
        return Err("Windows could not delete the file, or deletion was cancelled".into());
    }
    if path.try_exists().map_err(|_| "Cannot verify deletion")? {
        return Err("File still exists after deletion".into());
    }
    Ok(())
}
#[cfg(not(windows))]
fn recycle_file(_path: &Path) -> Result<(), String> {
    Err("File recycling is currently supported on Windows only".into())
}

#[derive(Serialize)]
pub struct FileActionReport {
    succeeded: Vec<String>,
    failed: Vec<String>,
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(action, count = ids.len()), err)]
pub async fn stage_file_action(
    app: tauri::AppHandle,
    ids: Vec<String>,
    action: String,
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, StageState>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<FileActionReport, String> {
    use tauri_plugin_opener::OpenerExt;
    if !matches!(action.as_str(), "open" | "reveal" | "delete") || ids.is_empty() || ids.len() > 500
    {
        return Err("Invalid file action or selection (maximum 500 files)".into());
    }
    state
        .scanning
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "Wait for the current Stage operation to finish")?;
    let _guard = FileActionGuard(&state.scanning);
    let configured_root = config.snapshot()?.settings.staging_path;
    let mut report = FileActionReport {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };
    tracing::info!("Stage file action started");
    let mut seen = std::collections::HashSet::new();
    for raw_id in ids {
        let id = raw_id.parse::<i64>().map_err(|_| "Invalid media ID")?;
        if !seen.insert(id) {
            continue;
        }
        let result = async {
            let row = sqlx::query("SELECT s.root_path,s.kind,m.relative_path,m.file_revision,m.needs_distribution FROM media m JOIN sources s ON s.id=m.source_id WHERE m.id=?")
                .bind(id).fetch_optional(database.pool()).await.map_err(|_| "Cannot read file record")?.ok_or("File record is unavailable")?;
            let root = PathBuf::from(row.get::<String,_>("root_path"));
            if row.get::<i64,_>("needs_distribution") == 0 && (row.get::<String,_>("kind") != "stage" || configured_root.as_ref() != Some(&root)) {
                return Err("File is outside the current Stage selection".to_string());
            }
            let relative: String = row.get("relative_path");
            let revision: Option<String> = row.get("file_revision");
            let worker_action = action.clone();
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let path = checked_action_path(&root, &relative)?;
                if worker_action == "delete" {
                    if let Some(expected) = revision {
                        let current = inspect(path.clone(), &root)?;
                        if current.is_none_or(|current| current.revision != expected) { return Err("File changed; rescan Stage before deleting".into()); }
                    }
                    recycle_file(&path)
                } else {
                    let result = if worker_action == "open" { app.opener().open_path(path.to_str().ok_or("File path is not valid Unicode")?, None::<&str>) } else { app.opener().reveal_item_in_dir(path) };
                    result.map_err(|_| "Windows could not open the file or folder".into())
                }
            }).await.map_err(|_| "File action worker failed")??;
            if action == "delete" {
                sqlx::query("DELETE FROM media WHERE id=?").bind(id).execute(database.pool()).await
                    .map_err(|_| "File deleted, but database cleanup failed; rescan Stage")?;
            }
            Ok::<_, String>(())
        }.await;
        match result {
            Ok(()) => report.succeeded.push(raw_id),
            Err(error) => {
                tracing::warn!(media_id = id, reason = %error, "Stage file action failed");
                report.failed.push(format!("File {raw_id}: {error}"));
            }
        }
    }
    tracing::info!(
        succeeded = report.succeeded.len(),
        failed = report.failed.len(),
        "Stage file action completed"
    );
    Ok(report)
}
pub const CACHE_MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStatus {
    bytes: u64,
    max_bytes: u64,
}

#[tauri::command]
pub async fn get_cache_usage(state: tauri::State<'_, StageState>) -> Result<CacheStatus, String> {
    let cache = state.cache.clone();
    tauri::async_runtime::spawn_blocking(move || {
        cache
            .usage()
            .map(|usage| CacheStatus {
                bytes: usage.bytes,
                max_bytes: CACHE_MAX_BYTES,
            })
            .map_err(|error| {
                tracing::warn!(%error, "Cache usage measurement failed");
                "Could not measure thumbnail cache".to_string()
            })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub running: bool,
    pub scanned: u64,
    pub extracted: u64,
    pub errors: u64,
    pub message: String,
}

pub struct StageState {
    scanning: AtomicBool,
    status: Mutex<ScanStatus>,
    pub cache: Arc<PreviewCache>,
    preview_slots: Arc<tokio::sync::Semaphore>,
    generated_previews: Arc<AtomicU64>,
}
impl StageState {
    pub fn new(cache_path: PathBuf) -> Self {
        Self {
            scanning: AtomicBool::new(false),
            status: Mutex::new(ScanStatus::default()),
            cache: Arc::new(PreviewCache::new(cache_path)),
            preview_slots: Arc::new(tokio::sync::Semaphore::new(2)),
            generated_previews: Arc::new(AtomicU64::new(0)),
        }
    }
}

fn now() -> i64 {
    timestamp(SystemTime::now()).unwrap_or(0)
}
fn timestamp(time: SystemTime) -> Option<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(value) => i64::try_from(value.as_millis()).ok(),
        Err(error) => i64::try_from(error.duration().as_millis())
            .ok()
            .map(|value| -value),
    }
}
fn kind(extension: &str) -> Option<MediaKind> {
    match extension {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "avif" | "heic"
        | "heif" => Some(MediaKind::Image),
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpeg" | "mpg" => {
            Some(MediaKind::Video)
        }
        _ => None,
    }
}

#[derive(Clone)]
struct Discovered {
    path: PathBuf,
    relative: String,
    extension: String,
    kind: MediaKind,
    size: i64,
    modified: Option<i64>,
    created: Option<i64>,
    revision: String,
}
fn inspect(path: PathBuf, root: &Path) -> Result<Option<Discovered>, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let Some(kind) = kind(&extension) else {
        return Ok(None);
    };
    let info = std::fs::symlink_metadata(&path).map_err(|_| "Cannot inspect media file")?;
    if !info.file_type().is_file() {
        return Err("Media is no longer a regular file".into());
    }
    let modified = info.modified().map_err(|_| "Cannot read file revision")?;
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "File escaped Stage root")?
        .to_str()
        .ok_or("Invalid Unicode path")?
        .to_owned();
    let revision = format!("{}:{modified:?}", info.len());
    Ok(Some(Discovered {
        relative,
        extension,
        kind,
        size: i64::try_from(info.len()).map_err(|_| "File size exceeds database range")?,
        modified: timestamp(modified),
        created: info.created().ok().and_then(timestamp),
        revision,
        path,
    }))
}

enum WalkMessage {
    Batch(Vec<Discovered>),
    Complete(u64),
}
fn walk(
    root: &Path,
    directory: &Path,
    depth: usize,
    batch: &mut Vec<Discovered>,
    errors: &mut u64,
    tx: &tokio::sync::mpsc::Sender<WalkMessage>,
) -> Result<(), String> {
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
            walk(root, &entry.path(), depth + 1, batch, errors, tx)?;
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

async fn activate_source(pool: &SqlitePool, root: &Path, scan: i64) -> Result<i64, String> {
    let root = root.to_str().ok_or("Invalid Unicode Stage root")?;
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
        .is_some_and(|row| row.get::<String, _>("kind") != "stage")
    {
        return Err("Stage root already belongs to Library".into());
    }
    sqlx::query("UPDATE sources SET is_active = 0 WHERE kind = 'stage'")
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
        sqlx::query_scalar("INSERT INTO sources(kind,root_path,path_key,created_at) VALUES('stage',?,?,?) RETURNING id").bind(root).bind(root).bind(scan).fetch_one(&mut *tx).await.map_err(|e| e.to_string())?
    };
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(id)
}

struct Prepared {
    file: Discovered,
    metadata: Option<MediaMetadata>,
    failed: bool,
}
async fn prepare(
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
    for file in files {
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
                tracing::warn!("Changing Stage file deferred to next scan");
            }
        }
    }
    Ok(prepared)
}

async fn save_batch(
    pool: &SqlitePool,
    source: i64,
    scan: i64,
    batch: Vec<Prepared>,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
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
        let id: i64 = sqlx::query_scalar("INSERT INTO media(source_id,relative_path,original_filename,path_key,kind,extension,size_bytes,file_modified_at,file_created_at,file_revision,metadata_version,metadata_state,width,height,last_seen_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(source_id,path_key) DO UPDATE SET availability='present',size_bytes=excluded.size_bytes,file_modified_at=excluded.file_modified_at,file_created_at=excluded.file_created_at,file_revision=excluded.file_revision,metadata_version=excluded.metadata_version,metadata_state=CASE WHEN ? THEN excluded.metadata_state ELSE media.metadata_state END,width=CASE WHEN ? THEN excluded.width ELSE media.width END,height=CASE WHEN ? THEN excluded.height ELSE media.height END,last_seen_at=excluded.last_seen_at,updated_at=excluded.updated_at RETURNING id")
            .bind(source).bind(&file.relative).bind(filename).bind(&file.relative).bind(if file.kind == MediaKind::Video { "video" } else { "image" }).bind(&file.extension).bind(file.size).bind(file.modified).bind(file.created).bind(&file.revision).bind(EXTRACTOR_VERSION).bind(if failed { "failed" } else { "ready" }).bind(metadata.as_ref().and_then(|m| m.width)).bind(metadata.as_ref().and_then(|m| m.height)).bind(scan).bind(scan).bind(scan).bind(changed).bind(changed).bind(changed).fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
        if file.kind == MediaKind::Video && changed {
            let value = metadata.unwrap_or_default();
            sqlx::query("INSERT INTO video_metadata(media_id,duration_ms,fps_num,fps_den,codec,bitrate_bps) VALUES(?,?,?,?,?,?) ON CONFLICT(media_id) DO UPDATE SET duration_ms=excluded.duration_ms,fps_num=excluded.fps_num,fps_den=excluded.fps_den,codec=excluded.codec,bitrate_bps=excluded.bitrate_bps")
                .bind(id).bind(value.duration_ms.and_then(|v| i64::try_from(v).ok())).bind(value.frame_rate.map(|v| v.numerator)).bind(value.frame_rate.map(|v| v.denominator)).bind(value.codec).bind(value.bitrate_bps).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
        sqlx::query("UPDATE media SET stage_search_key=? WHERE id=? AND stage_search_key IS NULL")
            .bind(crate::legacy_import::name_key(
                &file.path.to_string_lossy().replace('\\', "/"),
            ))
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

fn publish(app: &tauri::AppHandle, status: &ScanStatus) {
    if let Ok(mut stored) = app.state::<StageState>().status.lock() {
        *stored = status.clone();
    }
    if app.emit("stage-scan-status", status).is_err() {
        tracing::debug!("Scan progress listener unavailable");
    }
}

#[tracing::instrument(skip_all)]
async fn scan(
    pool: &SqlitePool,
    root: PathBuf,
    mut report: impl FnMut(&ScanStatus),
) -> Result<ScanStatus, String> {
    tracing::info!("Stage scan started");
    let checked_root = root.clone();
    let root = tauri::async_runtime::spawn_blocking(move || {
        if !checked_root.is_absolute() {
            return Err("Stage directory must be absolute");
        }
        if !std::fs::metadata(&checked_root)
            .map_err(|_| "Stage directory is unavailable")?
            .is_dir()
        {
            return Err("Stage path is not a directory");
        }
        std::fs::read_dir(&checked_root).map_err(|_| "Stage directory cannot be read")?;
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
    let source = activate_source(pool, &root, marker).await?;
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let walker = tauri::async_runtime::spawn_blocking(move || {
        let mut batch = Vec::with_capacity(BATCH);
        let mut errors = 0;
        walk(&root, &root, 0, &mut batch, &mut errors, &tx)?;
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
        message: "Scanning Stage…".into(),
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
                tracing::debug!(batch_files = count, "Stage batch committed");
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
            "Incomplete Stage scan; unseen records preserved"
        );
    }
    status.running = false;
    status.message = if traversal_errors == 0 {
        "Stage scan complete"
    } else {
        "Incomplete scan; unseen files preserved"
    }
    .into();
    tracing::info!(
        scanned = status.scanned,
        extracted = status.extracted,
        errors = status.errors,
        "Stage scan completed"
    );
    Ok(status)
}

async fn reconcile(
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
    sqlx::query("UPDATE media SET availability='missing',updated_at=? WHERE source_id=? AND last_seen_at<>?").bind(marker).bind(source).bind(marker).execute(&mut *tx).await.map_err(|e| e.to_string())?;
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
    if app
        .state::<StageState>()
        .scanning
        .swap(true, Ordering::AcqRel)
    {
        return Err("Stage scan is already running".into());
    }
    publish(
        &app,
        &ScanStatus {
            running: true,
            message: "Starting Stage scan…".into(),
            ..Default::default()
        },
    );
    tauri::async_runtime::spawn(async move {
        let result = scan(app.state::<Database>().pool(), root, |status| {
            publish(&app, status)
        })
        .await;
        let final_status = match result {
            Ok(status) => status,
            Err(error) => {
                tracing::error!(
                    "Stage scan failed; committed batches retained and missing reconciliation skipped"
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
        app.state::<StageState>()
            .scanning
            .store(false, Ordering::Release);
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageFile {
    id: String,
    path: String,
    filename: String,
    kind: String,
    extension: String,
    size: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    modified_at: Option<i64>,
    created_at: Option<i64>,
    duration: Option<i64>,
    fps: Option<f64>,
    codec: Option<String>,
    bitrate: Option<f64>,
    native_redistribution: bool,
    revision: Option<String>,
    availability: String,
}
#[derive(Serialize)]
pub struct StagePage {
    files: Vec<StageFile>,
    total: i64,
}

async fn browse(
    pool: &SqlitePool,
    root: Option<String>,
    after: Option<i64>,
    search: &str,
) -> Result<StagePage, String> {
    // Imported/redistribution records need searchable path keys, without filesystem reads.
    loop {
        let rows = sqlx::query("SELECT m.id,s.root_path,m.relative_path FROM media m JOIN sources s ON m.source_id=s.id WHERE m.stage_search_key IS NULL AND (s.kind='stage' OR m.needs_distribution=1) LIMIT 500").fetch_all(pool).await.map_err(|e| e.to_string())?;
        if rows.is_empty() {
            break;
        }
        let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
        for row in rows {
            let path = Path::new(&row.get::<String, _>("root_path"))
                .join(row.get::<String, _>("relative_path"));
            let key = crate::legacy_import::name_key(&path.to_string_lossy().replace('\\', "/"));
            sqlx::query(
                "UPDATE media SET stage_search_key=? WHERE id=? AND stage_search_key IS NULL",
            )
            .bind(key)
            .bind(row.get::<i64, _>("id"))
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        tx.commit().await.map_err(|e| e.to_string())?;
    }
    let search = crate::legacy_import::name_key(&search.trim().replace('\\', "/"));
    let total = sqlx::query_scalar("SELECT count(*) FROM media m JOIN sources s ON s.id=m.source_id WHERE ((s.kind='stage' AND s.path_key=? AND m.availability='present') OR m.needs_distribution=1) AND instr(m.stage_search_key,?)>0").bind(&root).bind(&search).fetch_one(pool).await.map_err(|e| e.to_string())?;
    let rows = sqlx::query("SELECT m.*,s.root_path,v.duration_ms,v.fps_num,v.fps_den,v.codec,v.bitrate_bps FROM media m JOIN sources s ON s.id=m.source_id LEFT JOIN video_metadata v ON v.media_id=m.id WHERE (? IS NULL OR m.id>?) AND ((s.kind='stage' AND s.path_key=? AND m.availability='present') OR m.needs_distribution=1) AND instr(m.stage_search_key,?)>0 ORDER BY m.id LIMIT 500").bind(after).bind(after).bind(root).bind(search).fetch_all(pool).await.map_err(|e| e.to_string())?;
    let files = rows.into_iter().map(stage_file).collect();
    Ok(StagePage { files, total })
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_stage_files(
    after: Option<String>,
    search: String,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<StagePage, String> {
    let root = config.snapshot()?.settings.staging_path.map(|path| {
        path.components()
            .collect::<PathBuf>()
            .to_string_lossy()
            .into_owned()
    });
    let after = after
        .map(|value| value.parse::<i64>())
        .transpose()
        .map_err(|_| "Invalid Stage cursor")?;
    browse(database.pool(), root, after, &search).await
}

fn stage_file(row: sqlx::sqlite::SqliteRow) -> StageFile {
    let fps = row
        .get::<Option<i64>, _>("fps_num")
        .zip(row.get::<Option<i64>, _>("fps_den"))
        .map(|(num, den)| num as f64 / den as f64);
    StageFile {
        id: row.get::<i64, _>("id").to_string(),
        path: Path::new(&row.get::<String, _>("root_path"))
            .join(row.get::<String, _>("relative_path"))
            .to_string_lossy()
            .into_owned(),
        filename: Path::new(&row.get::<String, _>("relative_path"))
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        kind: row.get("kind"),
        extension: row.get("extension"),
        size: row.get("size_bytes"),
        width: row.get("width"),
        height: row.get("height"),
        modified_at: row.get("file_modified_at"),
        created_at: row.get("file_created_at"),
        duration: row.get("duration_ms"),
        fps,
        codec: row.get("codec"),
        bitrate: row
            .get::<Option<i64>, _>("bitrate_bps")
            .map(|value| value as f64 / 1_000_000.0),
        native_redistribution: row.get::<i64, _>("needs_distribution") != 0,
        revision: row.get("file_revision"),
        availability: row.get("availability"),
    }
}

#[tauri::command]
pub async fn get_stage_preview(
    app: tauri::AppHandle,
    id: String,
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, StageState>,
) -> Result<tauri::ipc::Response, String> {
    let id = id.parse::<i64>().map_err(|_| "Invalid media ID")?;
    let permit = state
        .preview_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| "Preview workers busy")?;
    let row = sqlx::query("SELECT s.root_path,m.relative_path,m.file_revision FROM media m JOIN sources s ON s.id=m.source_id WHERE m.id=? AND m.availability='present' AND (s.kind='stage' OR m.needs_distribution=1)").bind(id).fetch_optional(database.pool()).await.map_err(|e| e.to_string())?.ok_or("Media unavailable")?;
    let path =
        Path::new(&row.get::<String, _>("root_path")).join(row.get::<String, _>("relative_path"));
    let cache = state.cache.clone();
    let generated = state.generated_previews.clone();
    let expected: Option<String> = row.get("file_revision");
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        if let Some(expected) = expected {
            let current = inspect(path.clone(), path.parent().ok_or("Invalid media path")?)?;
            if current.is_none_or(|current| current.revision != expected) {
                return Err("File changed; rescan Stage".into());
            }
        }
        let preview = cache
            .load_or_generate(&path, 512)
            .map_err(|_| "Preview unavailable")?;
        // Periodic oldest-write eviction bounds disk growth without inventory per card.
        if preview.origin == PreviewOrigin::Generated
            && generated.fetch_add(1, Ordering::Relaxed).is_multiple_of(64)
        {
            match cache.prune(CACHE_MAX_BYTES) {
                Ok(report) => {
                    let bytes = report.remaining.bytes.saturating_add(report.removed.bytes);
                    if bytes >= CACHE_MAX_BYTES {
                        tracing::info!(
                            bytes,
                            max_bytes = CACHE_MAX_BYTES,
                            "Preview cache budget reached"
                        );
                        if let Err(error) = app.emit(
                            "cache-limit-reached",
                            CacheStatus {
                                bytes,
                                max_bytes: CACHE_MAX_BYTES,
                            },
                        ) {
                            tracing::warn!(%error, "Cache limit notification unavailable");
                        }
                    }
                }
                Err(_) => tracing::warn!("Preview cache budget maintenance unavailable"),
            }
        }
        let mut bytes = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
            .encode(
                preview.image.as_raw(),
                preview.image.width(),
                preview.image.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|_| "Cannot encode preview")?;
        Ok::<_, String>(bytes)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_metadata::FrameRate;

    #[test]
    fn file_actions_reject_escaping_missing_and_non_file_paths() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Stage");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("日本.png"), b"test").unwrap();
        assert_eq!(
            checked_action_path(&root, "日本.png").unwrap(),
            root.join("日本.png")
        );
        std::fs::write(temp.path().join("outside.png"), b"preserve").unwrap();
        for relative in ["../outside.png", "", "missing.png", "."] {
            assert!(checked_action_path(&root, relative).is_err());
        }
        std::fs::create_dir(root.join("folder")).unwrap();
        assert!(checked_action_path(&root, "folder").is_err());
        assert!(
            checked_action_path(&root, temp.path().join("outside.png").to_str().unwrap()).is_err()
        );
        assert!(checked_action_path(Path::new("relative"), "image.png").is_err());
        assert_eq!(
            std::fs::read(temp.path().join("outside.png")).unwrap(),
            b"preserve"
        );
    }

    #[test]
    fn file_action_guard_releases_scan_gate_on_failure() {
        let gate = AtomicBool::new(true);
        {
            let _guard = FileActionGuard(&gate);
            assert!(gate.load(Ordering::Acquire));
        }
        assert!(!gate.load(Ordering::Acquire));
    }

    async fn fixture() -> (tempfile::TempDir, Database, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Stage 日本");
        std::fs::create_dir(&root).unwrap();
        let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
        (temp, db, root)
    }
    fn write_bmp(path: &Path, width: u32) {
        let row = (width * 3).div_ceil(4) * 4;
        let mut bytes = vec![0u8; (54 + row) as usize];
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&(54 + row).to_le_bytes());
        bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&width.to_le_bytes());
        bytes[22..26].copy_from_slice(&1u32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
        bytes[34..38].copy_from_slice(&row.to_le_bytes());
        std::fs::write(path, bytes).unwrap();
    }
    fn prepared(root: &Path, name: &str, width: u32) -> Prepared {
        let path = root.join(name);
        std::fs::write(&path, b"sample").unwrap();
        Prepared {
            file: inspect(path, root).unwrap().unwrap(),
            metadata: Some(MediaMetadata {
                width: Some(width),
                height: Some(1),
                duration_ms: Some(1234),
                frame_rate: Some(FrameRate {
                    numerator: 30000,
                    denominator: 1001,
                }),
                codec: Some("H.264".into()),
                bitrate_bps: Some(4_000_000),
                ..Default::default()
            }),
            failed: false,
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn scan_reuses_metadata_refreshes_changes_and_retains_missing_identity() {
        let (_temp, db, root) = fixture().await;
        let nested = root.join("nested");
        std::fs::create_dir(&nested).unwrap();
        let path = nested.join("Зображення.bmp");
        write_bmp(&path, 2);
        std::fs::write(root.join("ignore.zip"), b"archive").unwrap();
        let first = scan(db.pool(), root.clone(), |_| {}).await.unwrap();
        assert_eq!((first.scanned, first.extracted, first.errors), (1, 1, 0));
        let id: i64 = sqlx::query_scalar("SELECT id FROM media")
            .fetch_one(db.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE media SET favorite=1,needs_distribution=1")
            .execute(db.pool())
            .await
            .unwrap();
        let second = scan(db.pool(), root.clone(), |_| {}).await.unwrap();
        assert_eq!((second.scanned, second.extracted), (1, 0));
        write_bmp(&path, 5);
        let third = scan(db.pool(), root.clone(), |_| {}).await.unwrap();
        assert_eq!(third.extracted, 1);
        let row = sqlx::query(
            "SELECT id,width,size_bytes,favorite,needs_distribution,file_created_at FROM media",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("id"), id);
        assert_eq!(row.get::<i64, _>("width"), 5);
        assert_eq!(row.get::<i64, _>("favorite"), 1);
        assert_eq!(row.get::<i64, _>("needs_distribution"), 1);
        assert_eq!(
            row.get::<i64, _>("size_bytes"),
            std::fs::metadata(&path).unwrap().len() as i64
        );
        assert!(row.get::<Option<i64>, _>("file_created_at").is_some());
        // Tests remove only their own disposable fixture file.
        std::fs::remove_file(&path).unwrap();
        scan(db.pool(), root.clone(), |_| {}).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT availability FROM media")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            "missing"
        );
        write_bmp(&path, 2);
        scan(db.pool(), root, |_| {}).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT id FROM media WHERE availability='present'")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            id
        );
        db.close().await;
    }

    #[tokio::test]
    async fn persists_video_details_and_preserves_annotations_when_refreshing() {
        let (temp, db, root) = fixture().await;
        let source = activate_source(db.pool(), &root, 1).await.unwrap();
        save_batch(db.pool(), source, 1, vec![prepared(&root, "clip.mp4", 10)])
            .await
            .unwrap();
        sqlx::query("UPDATE video_metadata SET title='Keep title'")
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query(
            "UPDATE media SET favorite=1,needs_distribution=1,original_filename='original.mp4'",
        )
        .execute(db.pool())
        .await
        .unwrap();
        save_batch(db.pool(), source, 2, vec![prepared(&root, "clip.mp4", 20)])
            .await
            .unwrap();
        let row =
            sqlx::query("SELECT m.*,v.* FROM media m JOIN video_metadata v ON m.id=v.media_id")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(row.get::<i64, _>("width"), 20);
        assert_eq!(row.get::<i64, _>("duration_ms"), 1234);
        assert_eq!(row.get::<i64, _>("fps_den"), 1001);
        assert_eq!(row.get::<String, _>("codec"), "H.264");
        assert_eq!(row.get::<i64, _>("bitrate_bps"), 4_000_000);
        assert_eq!(row.get::<String, _>("title"), "Keep title");
        assert_eq!(row.get::<String, _>("original_filename"), "original.mp4");
        assert_eq!(row.get::<i64, _>("favorite"), 1);
        db.close().await;
        let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT width FROM media")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            20
        );
        db.close().await;
    }

    #[tokio::test]
    async fn incomplete_scans_preserve_unseen_records_and_failed_roots_do_not_activate() {
        let (_temp, db, root) = fixture().await;
        let source = activate_source(db.pool(), &root, 1).await.unwrap();
        save_batch(db.pool(), source, 1, vec![prepared(&root, "clip.mp4", 10)])
            .await
            .unwrap();
        assert!(!reconcile(db.pool(), source, 2, true, 1).await.unwrap());
        assert!(!reconcile(db.pool(), source, 2, false, 0).await.unwrap());
        assert!(scan(db.pool(), root.join("missing"), |_| {}).await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT availability FROM media")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            "present"
        );
        assert!(
            sqlx::query_scalar::<_, Option<i64>>("SELECT last_successful_scan_at FROM sources")
                .fetch_one(db.pool())
                .await
                .unwrap()
                .is_none()
        );
        assert!(reconcile(db.pool(), source, 2, true, 0).await.unwrap());
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT availability FROM media")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            "missing"
        );
        db.close().await;
    }

    #[tokio::test]
    async fn batch_failure_rolls_back_and_switching_roots_keeps_history() {
        let (_temp, db, root) = fixture().await;
        let source = activate_source(db.pool(), &root, 1).await.unwrap();
        sqlx::raw_sql("CREATE TRIGGER reject_second BEFORE INSERT ON media WHEN NEW.relative_path='bad.mp4' BEGIN SELECT RAISE(ABORT,'test failure'); END;").execute(db.pool()).await.unwrap();
        assert!(
            save_batch(
                db.pool(),
                source,
                1,
                vec![
                    prepared(&root, "ok.mp4", 10),
                    prepared(&root, "bad.mp4", 10)
                ]
            )
            .await
            .is_err()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM media")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            0
        );
        save_batch(db.pool(), source, 1, vec![prepared(&root, "ok.mp4", 10)])
            .await
            .unwrap();
        let other = root.join("other");
        std::fs::create_dir(&other).unwrap();
        let second = activate_source(db.pool(), &other, 2).await.unwrap();
        assert_ne!(source, second);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM media")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT is_active FROM sources WHERE id=?")
                .bind(source)
                .fetch_one(db.pool())
                .await
                .unwrap(),
            0
        );
        assert_eq!(activate_source(db.pool(), &root, 3).await.unwrap(), source);
        db.close().await;
    }

    #[tokio::test]
    async fn paginates_without_duplicates_searches_unicode_and_keeps_queued_files() {
        let (_temp, db, root) = fixture().await;
        let source = activate_source(db.pool(), &root, 1).await.unwrap();
        let mut batch = Vec::new();
        for index in 0..501 {
            batch.push(prepared(&root, &format!("ЗОБРАЖЕННЯ {index}.mp4"), 10));
            if batch.len() == BATCH {
                save_batch(db.pool(), source, 1, std::mem::take(&mut batch))
                    .await
                    .unwrap();
            }
        }
        save_batch(db.pool(), source, 1, batch).await.unwrap();
        let root_name = Some(root.to_string_lossy().into_owned());
        let first = browse(db.pool(), root_name.clone(), None, "")
            .await
            .unwrap();
        assert_eq!(first.total, 501);
        assert_eq!(first.files.len(), 500);
        let cursor = first.files.last().unwrap().id.parse().unwrap();
        let second = browse(db.pool(), root_name.clone(), Some(cursor), "")
            .await
            .unwrap();
        assert_eq!(second.files.len(), 1);
        assert_eq!(second.total, 501);
        let matched = browse(db.pool(), root_name, None, "зображення 500")
            .await
            .unwrap();
        assert_eq!(matched.total, 1);
        assert_eq!(matched.files[0].filename, "ЗОБРАЖЕННЯ 500.mp4");
        sqlx::query("UPDATE media SET needs_distribution=1,availability='missing',stage_search_key=NULL WHERE id=?").bind(cursor).execute(db.pool()).await.unwrap();
        let queued = browse(db.pool(), None, None, "").await.unwrap();
        assert_eq!(queued.total, 1);
        assert!(queued.files[0].native_redistribution);
        assert_eq!(queued.files[0].availability, "missing");
        db.close().await;
    }

    #[tokio::test]
    async fn walker_streams_bounded_batches_and_ignores_archives() {
        let (_temp, db, root) = fixture().await;
        for index in 0..130 {
            write_bmp(&root.join(format!("file{index}.bmp")), 2);
        }
        std::fs::write(root.join("archive.zip"), b"archive").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let worker = tauri::async_runtime::spawn_blocking(move || {
            let mut batch = Vec::new();
            let mut errors = 0;
            walk(&root, &root, 0, &mut batch, &mut errors, &tx).unwrap();
            tx.blocking_send(WalkMessage::Batch(batch)).unwrap();
            tx.blocking_send(WalkMessage::Complete(errors)).unwrap();
        });
        let mut count = 0;
        let mut batches = 0;
        while let Some(message) = rx.recv().await {
            match message {
                WalkMessage::Batch(files) => {
                    assert!(files.len() <= BATCH);
                    count += files.len();
                    batches += 1;
                }
                WalkMessage::Complete(errors) => assert_eq!(errors, 0),
            }
        }
        worker.await.unwrap();
        assert_eq!(count, 130);
        assert_eq!(batches, 3);
        db.close().await;
    }
}
