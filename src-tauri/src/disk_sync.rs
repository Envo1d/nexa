//! Serialized, restartable filesystem reconciliation. Never overwrite destinations.
use crate::{
    config::{AppConfig, ConfigStore},
    database::Database,
    legacy_import::name_key,
    stage::{self, StageState},
};
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, Manager};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub running: bool,
    pub completed: u64,
    pub total: u64,
    pub error: Option<String>,
}
#[derive(Default)]
pub struct SyncState {
    dirty: AtomicBool,
    status: Mutex<Status>,
}
pub fn request(app: &tauri::AppHandle) {
    app.state::<SyncState>()
        .dirty
        .store(true, Ordering::Release);
}
#[tauri::command]
pub fn get_disk_sync_status(state: tauri::State<'_, SyncState>) -> Result<Status, String> {
    state
        .status
        .lock()
        .map(|s| s.clone())
        .map_err(|_| "Synchronization status unavailable".into())
}
#[tauri::command]
pub fn retry_disk_sync(app: tauri::AppHandle) {
    request(&app);
}
fn publish(app: &tauri::AppHandle, value: &Status) {
    if let Ok(mut status) = app.state::<SyncState>().status.lock() {
        *status = value.clone();
    }
    let _ = app.emit("disk-sync-status", value);
}

pub fn start(app: tauri::AppHandle) {
    request(&app);
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if !app.state::<SyncState>().dirty.load(Ordering::Acquire) {
                continue;
            }
            let state = app.state::<StageState>();
            let Some(_guard) = state.try_operation() else {
                continue;
            };
            app.state::<SyncState>()
                .dirty
                .store(false, Ordering::Release);
            let mut status = Status {
                running: true,
                ..Default::default()
            };
            publish(&app, &status);
            tracing::info!("Disk reconciliation started");
            let result = reconcile(&app, &mut status).await;
            status.running = false;
            if let Err(error) = result {
                tracing::error!(
                    completed = status.completed,
                    "Disk reconciliation stopped; journal and files retained"
                );
                status.error = Some(error);
            } else {
                tracing::info!(
                    completed = status.completed,
                    "Disk reconciliation completed or paused"
                );
            }
            publish(&app, &status);
        }
    });
}

/// Replace forbidden Windows filename characters deterministically, without path traversal.
fn component(value: &str) -> Result<String, String> {
    let value: String = value
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut value = value.trim().trim_end_matches(['.', ' ']).to_string();
    if value.is_empty() || value == "." || value == ".." {
        return Err("Filename component is empty".into());
    }
    let base = value.split('.').next().unwrap_or("").to_ascii_uppercase();
    if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && matches!(base.as_bytes()[3], b'1'..=b'9'))
    {
        value.insert(0, '_');
    }
    if value.encode_utf16().count() > 240 {
        return Err("Filename component is too long".into());
    }
    Ok(value)
}
fn relative(root: &Path, value: &str, create: bool) -> Result<PathBuf, String> {
    if !root.is_absolute()
        || Path::new(value)
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
        || value.is_empty()
    {
        return Err("Invalid managed path".into());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|_| "Library/source directory is unavailable")?;
    let mut current = root.to_path_buf();
    let parts: Vec<_> = Path::new(value).components().collect();
    for (i, part) in parts.iter().enumerate() {
        current.push(part.as_os_str());
        if create && i + 1 < parts.len() && !current.exists() {
            fs::create_dir(&current).map_err(|e| e.to_string())?;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || !current
                        .canonicalize()
                        .map_err(|e| e.to_string())?
                        .starts_with(&canonical_root)
                {
                    return Err("Managed path crosses a link or source boundary".into());
                }
                if i + 1 < parts.len() && !metadata.is_dir() {
                    return Err("Destination parent is not a directory".into());
                }
                if i + 1 == parts.len() && !metadata.is_file() {
                    return Err("Only regular media files are supported".into());
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && (!create || i + 1 == parts.len()) =>
            {
                return Ok(root.join(value));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(current)
}
fn revision(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    Ok(format!(
        "{}:{:?}",
        metadata.len(),
        metadata.modified().map_err(|e| e.to_string())?
    ))
}
fn equal_files(a: &Path, b: &Path) -> Result<bool, String> {
    if fs::metadata(a).map_err(|e| e.to_string())?.len()
        != fs::metadata(b).map_err(|e| e.to_string())?.len()
    {
        return Ok(false);
    }
    let mut a = fs::File::open(a).map_err(|e| e.to_string())?;
    let mut b = fs::File::open(b).map_err(|e| e.to_string())?;
    let mut x = [0; 65536];
    let mut y = [0; 65536];
    loop {
        let n = a.read(&mut x).map_err(|e| e.to_string())?;
        b.read_exact(&mut y[..n]).map_err(|e| e.to_string())?;
        if x[..n] != y[..n] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

#[derive(sqlx::FromRow, Clone)]
struct Operation {
    media_id: i64,
    source_root: String,
    source_relative: String,
    source_revision: String,
    target_source: i64,
    target_root: String,
    target_relative: String,
}
async fn finish(pool: &SqlitePool, operation: Operation, recovery: bool) -> Result<(), String> {
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
    sqlx::query("UPDATE media SET source_id=?,relative_path=?,path_key=?,stage_search_key=?,file_revision=?,availability='present',updated_at=CAST(unixepoch('subsec')*1000 AS INTEGER) WHERE id=?")
        .bind(operation.target_source).bind(&operation.target_relative).bind(&operation.target_relative)
        .bind(name_key(&Path::new(&operation.target_root).join(&operation.target_relative).to_string_lossy().replace('\\', "/")))
        .bind(destination_revision).bind(operation.media_id).execute(pool).await.map_err(|e| e.to_string())?;
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
async fn relocate(
    pool: &SqlitePool,
    id: i64,
    target_source: i64,
    root: &str,
    destination: &str,
) -> Result<(), String> {
    let row = sqlx::query("SELECT s.root_path,m.relative_path,m.file_revision FROM media m JOIN sources s ON s.id=m.source_id WHERE m.id=?").bind(id).fetch_one(pool).await.map_err(|e| e.to_string())?;
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

struct Target {
    id: i64,
    folder: PathBuf,
    stem: String,
    extension: String,
    current_root: String,
    current_relative: String,
}
async fn targets(
    pool: &SqlitePool,
    root: &str,
    settings: &AppConfig,
) -> Result<Vec<(i64, String, String, String)>, String> {
    let mut groups: BTreeMap<String, Vec<Target>> = BTreeMap::new();
    let mut cursor = 0;
    loop {
        let rows = sqlx::query("SELECT m.id,m.kind,m.extension,m.original_filename,m.relative_path,s.root_path,a.name artist,v.title FROM media m JOIN sources s ON s.id=m.source_id JOIN artists a ON a.id=m.artist_id LEFT JOIN video_metadata v ON v.media_id=m.id WHERE m.id>? AND m.availability='present' AND m.needs_distribution=0 AND (m.library_assignment=? OR (s.kind='library' AND s.path_key=?)) ORDER BY m.id LIMIT 128").bind(cursor).bind(root).bind(root).fetch_all(pool).await.map_err(|e| e.to_string())?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let id: i64 = row.get("id");
            cursor = id;
            let characters: Vec<String> = sqlx::query_scalar("SELECT c.name FROM media_characters l JOIN characters c ON c.id=l.character_id WHERE l.media_id=? ORDER BY c.name_key,c.id").bind(id).fetch_all(pool).await.map_err(|e| e.to_string())?;
            let copyright: Option<String> = sqlx::query_scalar("SELECT c.name FROM media_copyrights l JOIN copyrights c ON c.id=l.copyright_id WHERE l.media_id=? ORDER BY c.name_key,c.id LIMIT 1").bind(id).fetch_optional(pool).await.map_err(|e| e.to_string())?;
            let artist: String = row.get("artist");
            let is_video = row.get::<String, _>("kind") == "video";
            let original: String = row.get("original_filename");
            let fallback = Path::new(&original)
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Invalid original filename")?;
            let base = if is_video {
                let title = row
                    .get::<Option<String>, _>("title")
                    .filter(|s| !s.trim().is_empty());
                let Some(title) = title else {
                    continue;
                };
                if characters.is_empty() {
                    title
                } else {
                    format!(
                        "{} - {title}",
                        characters.join(&settings.character_separator)
                    )
                }
            } else if characters.is_empty() {
                fallback.into()
            } else {
                characters.join(&settings.character_separator)
            };
            let stem = component(&format!("{base} [{artist}]"))?;
            let extension = component(&row.get::<String, _>("extension"))?.to_lowercase();
            let mut folder = PathBuf::new();
            if settings.copyright_directory
                && let Some(copyright) = copyright
            {
                folder.push(component(&copyright)?);
            }
            if settings.artist_directory {
                folder.push(component(&artist)?);
            }
            if is_video && !settings.video_subfolder.trim().is_empty() {
                folder.push(component(&settings.video_subfolder)?);
            } else if extension == "gif" && !settings.gif_subfolder.trim().is_empty() {
                folder.push(component(&settings.gif_subfolder)?);
                tracing::debug!(
                    media_id = id,
                    "GIF destination uses configured animation subfolder"
                );
            }
            let key = name_key(&folder.join(format!("{stem}.{extension}")).to_string_lossy());
            groups.entry(key).or_default().push(Target {
                id,
                folder,
                stem,
                extension,
                current_root: row.get("root_path"),
                current_relative: row.get("relative_path"),
            });
        }
    }
    let mut result = Vec::new();
    for group in groups.into_values() {
        let count = group.len();
        for (index, target) in group.into_iter().enumerate() {
            let suffix = if count > 1 {
                format!(" - {}", index + 1)
            } else {
                String::new()
            };
            let filename = component(&format!("{}{suffix}.{}", target.stem, target.extension))?;
            result.push((
                target.id,
                target.folder.join(filename).to_string_lossy().into_owned(),
                target.current_root,
                target.current_relative,
            ));
        }
    }
    Ok(result)
}
async fn reconcile(app: &tauri::AppHandle, status: &mut Status) -> Result<(), String> {
    let database = app.state::<Database>();
    let pool = database.pool();
    // Finish journaled work even when synchronization was disabled after publication.
    recover(pool).await?;
    let settings = app.state::<ConfigStore>().snapshot()?.settings;
    if !settings.sync_structure {
        return Ok(());
    }
    let root = settings
        .library_path
        .as_ref()
        .ok_or("Choose a Library directory before synchronizing")?;
    let root_text = root
        .components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned();
    let source = stage::activate_source_for(pool, root, stage::now(), "library").await?;
    let plan = targets(pool, &root_text, &settings).await?;
    status.total = plan.len() as u64;
    publish(app, status);
    execute_plan(
        pool,
        source,
        &root_text,
        plan,
        || {
            app.state::<ConfigStore>()
                .snapshot()
                .is_ok_and(|s| s.settings == settings)
        },
        |completed| {
            status.completed = completed;
            if completed.is_multiple_of(16) {
                publish(app, status);
            }
        },
    )
    .await?;
    if app.state::<ConfigStore>().snapshot()?.settings != settings {
        request(app);
    }
    Ok(())
}

type Plan = Vec<(i64, String, String, String)>;
async fn execute_plan(
    pool: &SqlitePool,
    source: i64,
    root: &str,
    plan: Plan,
    mut proceed: impl FnMut() -> bool,
    mut progress: impl FnMut(u64),
) -> Result<(), String> {
    if !proceed() {
        return Ok(());
    }
    let mut destinations = std::collections::HashSet::new();
    let mut occupied = std::collections::HashMap::new();
    for (id, destination, current_root, current_relative) in &plan {
        if !destinations.insert(name_key(destination)) {
            return Err(
                "Generated destination names collide; edit the conflicting metadata".into(),
            );
        }
        if current_root == root {
            occupied.insert(name_key(current_relative), *id);
        }
    }
    // Validate external collisions before moving any file out of its current location.
    for (id, destination, _, _) in &plan {
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT id FROM media WHERE source_id=? AND path_key=?")
                .bind(source)
                .bind(destination)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
        if existing.is_some_and(|other| {
            other != *id && occupied.get(&name_key(destination)) != Some(&other)
        }) {
            return Err(
                "Destination belongs to another indexed record; rescan or resolve the conflict"
                    .into(),
            );
        }
        let destination_key = name_key(destination);
        let root = root.to_owned();
        let destination = destination.clone();
        let exists = tauri::async_runtime::spawn_blocking(move || {
            relative(Path::new(&root), &destination, false)?
                .try_exists()
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        if exists && !occupied.contains_key(&destination_key) {
            return Err(format!(
                "Destination {} already exists; resolve the conflict and retry. Existing content was preserved",
                plan.iter()
                    .find(|entry| entry.0 == *id)
                    .ok_or("Missing plan entry")?
                    .1
            ));
        }
    }
    // Ordinary moves publish directly; vacate only names involved in a dependency or case-only rename.
    let mut positions: std::collections::HashMap<i64, String> = occupied
        .iter()
        .map(|(path, id)| (*id, path.clone()))
        .collect();
    for (index, (id, destination, _, _)) in plan.iter().enumerate() {
        if !proceed() {
            return Ok(());
        }
        let key = name_key(destination);
        if let Some(other) = occupied.get(&key).copied() {
            let entry = plan
                .iter()
                .find(|entry| entry.0 == other)
                .ok_or("Missing destination owner")?;
            let unchanged = other == *id && entry.2 == root && entry.3 == *destination;
            if !unchanged {
                let extension = Path::new(&entry.1)
                    .extension()
                    .and_then(|s| s.to_str())
                    .ok_or("Missing extension")?;
                let neutral = format!(".nexa-sync-{other}.{extension}");
                relocate(pool, other, source, root, &neutral).await?;
                if let Some(old) = positions.insert(other, name_key(&neutral)) {
                    occupied.remove(&old);
                }
                occupied.insert(name_key(&neutral), other);
                if !proceed() {
                    return Ok(());
                }
            }
        }
        relocate(pool, *id, source, root, destination).await?;
        if let Some(old) = positions.insert(*id, key.clone()) {
            occupied.remove(&old);
        }
        occupied.insert(key, *id);
        progress(index as u64 + 1);
        tokio::task::yield_now().await;
    }
    Ok(())
}

pub(crate) async fn recover(pool: &SqlitePool) -> Result<(), String> {
    let pending: Vec<Operation> = sqlx::query_as("SELECT * FROM file_operations ORDER BY media_id")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    for operation in pending {
        tracing::warn!(media_id = operation.media_id, "Recovering interrupted move");
        finish(pool, operation, true).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture() -> (tempfile::TempDir, Database, String, String) {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("Library");
        let stage = temp.path().join("Stage");
        fs::create_dir(&library).unwrap();
        fs::create_dir(&stage).unwrap();
        let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
        for (id, kind, root) in [(1, "library", &library), (2, "stage", &stage)] {
            sqlx::query(
                "INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(?,?,?,?,1)",
            )
            .bind(id)
            .bind(kind)
            .bind(root.to_str())
            .bind(root.to_str())
            .execute(db.pool())
            .await
            .unwrap();
        }
        sqlx::raw_sql("INSERT INTO artists VALUES(1,'Artist','artist'); INSERT INTO copyrights VALUES(1,'Zebra','zebra'),(2,'Alpha','alpha'); INSERT INTO characters VALUES(1,1,'Zed','zed'),(2,2,'Alice','alice');").execute(db.pool()).await.unwrap();
        for id in 1..=4 {
            let filename = format!("capture-{id}.png");
            fs::write(stage.join(&filename), format!("independent content {id}")).unwrap();
            sqlx::query("INSERT INTO media(id,source_id,artist_id,relative_path,path_key,original_filename,kind,extension,last_seen_at,created_at,updated_at,library_assignment) VALUES(?,2,1,?,?,?,'image','png',1,1,1,?)")
                .bind(id).bind(&filename).bind(&filename).bind(&filename).bind(library.to_str()).execute(db.pool()).await.unwrap();
            sqlx::query("INSERT INTO media_copyrights VALUES(?,1),(?,2)")
                .bind(id)
                .bind(id)
                .execute(db.pool())
                .await
                .unwrap();
            sqlx::query("INSERT INTO media_characters VALUES(?,1,1),(?,2,2)")
                .bind(id)
                .bind(id)
                .execute(db.pool())
                .await
                .unwrap();
        }
        (
            temp,
            db,
            library.to_string_lossy().into_owned(),
            stage.to_string_lossy().into_owned(),
        )
    }
    async fn run(pool: &SqlitePool, root: &str, settings: &AppConfig) {
        let plan = targets(pool, root, settings).await.unwrap();
        execute_plan(pool, 1, root, plan, || true, |_| {})
            .await
            .unwrap();
    }
    fn indexed(root: &str, index: i64) -> PathBuf {
        Path::new(root)
            .join("Alpha/Artist")
            .join(format!("Alice х Zed [Artist] - {index}.png"))
    }
    #[tokio::test]
    async fn sorts_primary_copyright_preserves_contents_and_compacts_single_and_batch_deletions() {
        let (_temp, db, root, stage) = fixture().await;
        let settings = AppConfig::default();
        run(db.pool(), &root, &settings).await;
        for id in 1..=4 {
            assert_eq!(
                fs::read(indexed(&root, id)).unwrap(),
                format!("independent content {id}").as_bytes()
            );
        }
        assert_eq!(fs::read_dir(&stage).unwrap().count(), 0);
        fs::remove_file(indexed(&root, 2)).unwrap();
        sqlx::query("DELETE FROM media WHERE id=2")
            .execute(db.pool())
            .await
            .unwrap();
        run(db.pool(), &root, &settings).await;
        assert_eq!(
            fs::read(indexed(&root, 2)).unwrap(),
            b"independent content 3"
        );
        assert_eq!(
            fs::read(indexed(&root, 3)).unwrap(),
            b"independent content 4"
        );
        assert!(!indexed(&root, 4).exists());
        fs::remove_file(indexed(&root, 1)).unwrap();
        fs::remove_file(indexed(&root, 3)).unwrap();
        sqlx::query("DELETE FROM media WHERE id IN (1,4)")
            .execute(db.pool())
            .await
            .unwrap();
        run(db.pool(), &root, &settings).await;
        let single = Path::new(&root).join("Alpha/Artist/Alice х Zed [Artist].png");
        assert_eq!(fs::read(single).unwrap(), b"independent content 3");
        assert!(!indexed(&root, 2).exists());
        let original: String = sqlx::query_scalar("SELECT original_filename FROM media WHERE id=3")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(original, "capture-3.png");
        db.close().await;
    }
    #[tokio::test]
    async fn pauses_between_operations_and_rebuilds_using_current_folder_settings() {
        let (_temp, db, root, _stage) = fixture().await;
        let settings = AppConfig::default();
        let plan = targets(db.pool(), &root, &settings).await.unwrap();
        let mut calls = 0;
        execute_plan(
            db.pool(),
            1,
            &root,
            plan,
            || {
                calls += 1;
                calls <= 2
            },
            |_| {},
        )
        .await
        .unwrap();
        assert!(indexed(&root, 1).exists());
        let settings = AppConfig {
            copyright_directory: false,
            artist_directory: false,
            ..settings
        };
        run(db.pool(), &root, &settings).await;
        for id in 1..=4 {
            assert!(
                Path::new(&root)
                    .join(format!("Alice х Zed [Artist] - {id}.png"))
                    .exists()
            );
        }
        assert!(!Path::new(&root).join(".nexa-sync-1.png").exists());
        run(db.pool(), &root, &AppConfig::default()).await;
        assert!(indexed(&root, 1).exists());
        db.close().await;
    }
    #[tokio::test]
    async fn refuses_external_conflicts_and_changed_sources_without_losing_files() {
        let (_temp, db, root, stage) = fixture().await;
        fs::create_dir_all(Path::new(&root).join("Alpha/Artist")).unwrap();
        fs::write(indexed(&root, 1), b"external content").unwrap();
        let plan = targets(db.pool(), &root, &AppConfig::default())
            .await
            .unwrap();
        assert!(
            execute_plan(db.pool(), 1, &root, plan, || true, |_| {})
                .await
                .is_err()
        );
        assert_eq!(fs::read(indexed(&root, 1)).unwrap(), b"external content");
        assert!(Path::new(&stage).join("capture-1.png").exists());
        assert!(!Path::new(&root).join(".nexa-sync-1.png").exists());
        sqlx::query("UPDATE media SET file_revision='stale' WHERE id=1")
            .execute(db.pool())
            .await
            .unwrap();
        assert!(relocate(db.pool(), 1, 1, &root, "moved.png").await.is_err());
        assert!(Path::new(&stage).join("capture-1.png").exists());
        db.close().await;
    }
    #[tokio::test]
    async fn recovers_publication_and_database_commit_boundaries_after_reopen() {
        let (temp, db, root, stage) = fixture().await;
        let source = Path::new(&stage).join("capture-1.png");
        let rev = revision(&source).unwrap();
        sqlx::query(
            "INSERT INTO file_operations VALUES(1,?,'capture-1.png',?,1,?,'published.png')",
        )
        .bind(&stage)
        .bind(&rev)
        .bind(&root)
        .execute(db.pool())
        .await
        .unwrap();
        fs::hard_link(&source, Path::new(&root).join("published.png")).unwrap();
        db.close().await;
        let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
        recover(db.pool()).await.unwrap();
        assert!(!source.exists());
        let current: String = sqlx::query_scalar("SELECT relative_path FROM media WHERE id=1")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(current, "published.png");
        // Simulate committed target followed by source cleanup, before journal deletion.
        sqlx::query(
            "INSERT INTO file_operations VALUES(1,?,'capture-1.png',?,1,?,'published.png')",
        )
        .bind(&stage)
        .bind(&rev)
        .bind(&root)
        .execute(db.pool())
        .await
        .unwrap();
        recover(db.pool()).await.unwrap();
        let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM file_operations")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(pending, 0);
        assert_eq!(
            fs::read(Path::new(&root).join("published.png")).unwrap(),
            b"independent content 1"
        );
        db.close().await;
    }
    #[test]
    fn confines_components_and_rejects_escaping_paths() {
        assert_eq!(component("CON.png").unwrap(), "_CON.png");
        assert_eq!(component("Artist/../Other").unwrap(), "Artist_.._Other");
        assert!(component(".. ").is_err());
        let root = tempfile::tempdir().unwrap();
        assert!(relative(root.path(), "../outside.png", false).is_err());
        assert!(relative(root.path(), "missing/folder/image.png", false).is_ok());
    }

    #[tokio::test]
    async fn relocates_gifs_to_configured_subfolder_preserving_bytes_and_image_naming() {
        let (_temp, db, root, stage) = fixture().await;
        let source = Path::new(&stage).join("capture-1.gif");
        fs::rename(Path::new(&stage).join("capture-1.png"), &source).unwrap();
        let bytes = fs::read(&source).unwrap();
        sqlx::query("UPDATE media SET extension='GIF',relative_path='capture-1.gif',path_key='capture-1.gif',original_filename='capture-1.gif' WHERE id=1")
            .execute(db.pool()).await.unwrap();
        let mut settings = AppConfig::default();
        run(db.pool(), &root, &settings).await;
        let default_path =
            Path::new(&root).join("Alpha/Artist/Animations/Alice х Zed [Artist].gif");
        assert_eq!(fs::read(&default_path).unwrap(), bytes);
        assert!(!source.exists());
        assert!(indexed(&root, 1).exists()); // Other images stay outside Animations.
        settings.gif_subfolder = "GIFs".into();
        run(db.pool(), &root, &settings).await;
        let custom_path = Path::new(&root).join("Alpha/Artist/GIFs/Alice х Zed [Artist].gif");
        assert_eq!(fs::read(&custom_path).unwrap(), bytes);
        assert!(!default_path.exists());
        settings.gif_subfolder = " ".into();
        settings.copyright_directory = false;
        settings.artist_directory = false;
        run(db.pool(), &root, &settings).await;
        assert_eq!(
            fs::read(Path::new(&root).join("Alice х Zed [Artist].gif")).unwrap(),
            bytes
        );
        assert!(!custom_path.exists());
        let kind: String = sqlx::query_scalar("SELECT kind FROM media WHERE id=1")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(kind, "image");
        db.close().await;
    }

    #[tokio::test]
    async fn names_videos_without_characters_and_images_using_original_provenance() {
        let (_temp, db, root, _stage) = fixture().await;
        sqlx::query("DELETE FROM media_characters WHERE media_id IN (1,2)")
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE media SET kind='video',extension='mp4' WHERE id=1")
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("INSERT INTO video_metadata(media_id,title) VALUES(1,'Video title')")
            .execute(db.pool())
            .await
            .unwrap();
        let plan = targets(db.pool(), &root, &AppConfig::default())
            .await
            .unwrap();
        assert_eq!(
            Path::new(&plan.iter().find(|p| p.0 == 1).unwrap().1),
            Path::new("Alpha/Artist/Videos/Video title [Artist].mp4")
        );
        assert_eq!(
            Path::new(&plan.iter().find(|p| p.0 == 2).unwrap().1),
            Path::new("Alpha/Artist/capture-2 [Artist].png")
        );
        // Physical renames cannot change the fallback base.
        sqlx::query("UPDATE media SET relative_path='edited-name.png',path_key='edited-name.png' WHERE id=2").execute(db.pool()).await.unwrap();
        let again = targets(db.pool(), &root, &AppConfig::default())
            .await
            .unwrap();
        assert_eq!(
            again.iter().find(|p| p.0 == 2).unwrap().1,
            plan.iter().find(|p| p.0 == 2).unwrap().1
        );
        db.close().await;
    }

    #[tokio::test]
    async fn resumes_a_paused_filename_swap_without_overwriting_either_file() {
        let (_temp, db, root, _stage) = fixture().await;
        sqlx::raw_sql("UPDATE media SET needs_distribution=1 WHERE id IN (3,4); DELETE FROM media_characters WHERE media_id IN (1,2); INSERT INTO media_characters VALUES(1,2,2),(2,1,1);").execute(db.pool()).await.unwrap();
        run(db.pool(), &root, &AppConfig::default()).await;
        sqlx::raw_sql("DELETE FROM media_characters WHERE media_id IN (1,2); INSERT INTO media_characters VALUES(1,1,1),(2,2,2);").execute(db.pool()).await.unwrap();
        let plan = targets(db.pool(), &root, &AppConfig::default())
            .await
            .unwrap();
        let mut calls = 0;
        execute_plan(
            db.pool(),
            1,
            &root,
            plan,
            || {
                calls += 1;
                calls <= 2
            },
            |_| {},
        )
        .await
        .unwrap();
        assert!(Path::new(&root).join(".nexa-sync-1.png").exists());
        run(db.pool(), &root, &AppConfig::default()).await;
        assert_eq!(
            fs::read(Path::new(&root).join("Alpha/Artist/Zed [Artist].png")).unwrap(),
            b"independent content 1"
        );
        assert_eq!(
            fs::read(Path::new(&root).join("Alpha/Artist/Alice [Artist].png")).unwrap(),
            b"independent content 2"
        );
        assert!(!Path::new(&root).join(".nexa-sync-1.png").exists());
        db.close().await;
    }
}
