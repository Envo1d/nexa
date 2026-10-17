//! Serialized, restartable filesystem reconciliation. Never overwrite destinations.
mod changes;
mod moves;
mod ordering;
mod planning;
pub use ordering::{
    __cmd__get_duplicate_order, __cmd__save_duplicate_order,
    __tauri_command_name_get_duplicate_order, __tauri_command_name_save_duplicate_order,
    get_duplicate_order, save_duplicate_order,
};

use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use moves::{Operation, finish, relocate};
use planning::targets;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{Emitter, Manager};

use crate::{
    config::{AppConfig, ConfigStore},
    database::Database,
    naming::name_key,
    stage::{self, StageState},
};

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
    forced: AtomicBool,
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
    app.state::<SyncState>()
        .forced
        .store(true, Ordering::Release);
    request(&app);
}

fn publish(app: &tauri::AppHandle, value: &Status) {
    if let Ok(mut status) = app.state::<SyncState>().status.lock() {
        *status = value.clone();
    }

    let _ = app.emit("disk-sync-status", value);
}

pub fn start(app: tauri::AppHandle) {
    retry_disk_sync(app.clone());
    tauri::async_runtime::spawn(async move {
        let mut previous_inputs = None;

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

            let forced = app
                .state::<SyncState>()
                .forced
                .swap(false, Ordering::AcqRel);

            let preparation = async {
                let settings = app.state::<ConfigStore>().snapshot()?.settings;

                // Startup/Retry must reach journal recovery even if input inspection fails.
                let inputs = if forced {
                    None
                } else {
                    changes::Inputs::capture(app.state::<Database>().pool(), &settings).await?
                };

                Ok::<_, String>((settings, inputs))
            }
            .await;

            if let Ok((_, inputs)) = &preparation
                && !changes::Inputs::requires_sync(inputs, &previous_inputs, forced)
            {
                tracing::debug!("Disk reconciliation skipped; structure unchanged or disabled");
                continue;
            }

            let mut status = Status {
                running: true,
                ..Default::default()
            };
            publish(&app, &status);

            tracing::info!(forced, "Disk reconciliation started");

            let result = async {
                let (settings, inputs) = preparation?;
                previous_inputs = inputs;
                reconcile(&app, &mut status, &settings).await?;

                // Our own moves change the stored paths. Remember the resulting state so
                // the next unchanged scan does not invalidate this completed pass.
                previous_inputs =
                    changes::Inputs::capture(app.state::<Database>().pool(), &settings).await?;
                Ok::<_, String>(())
            }
            .await;
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

pub(crate) fn revision(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    Ok(format!(
        "{}:{:?}",
        metadata.len(),
        metadata.modified().map_err(|e| e.to_string())?
    ))
}

pub(crate) fn equal_files(a: &Path, b: &Path) -> Result<bool, String> {
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

async fn reconcile(
    app: &tauri::AppHandle,
    status: &mut Status,
    settings: &AppConfig,
) -> Result<(), String> {
    let database = app.state::<Database>();

    let pool = database.pool();
    // Finish journaled work even when synchronization was disabled after publication.
    recover(pool).await?;

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

    let plan = targets(pool, &root_text, settings).await?;
    status.total = plan
        .iter()
        .filter(|entry| needs_move(entry, &root_text))
        .count() as u64;
    publish(app, status);
    execute_plan(
        pool,
        source,
        &root_text,
        plan,
        || {
            app.state::<ConfigStore>()
                .snapshot()
                .is_ok_and(|s| s.settings.same_disk_structure(settings))
        },
        |completed| {
            status.completed = completed;

            if completed.is_multiple_of(16) {
                publish(app, status);
            }
        },
    )
    .await?;

    if !app
        .state::<ConfigStore>()
        .snapshot()?
        .settings
        .same_disk_structure(settings)
    {
        request(app);
    }
    Ok(())
}

type Plan = Vec<(i64, String, String, String)>;

fn needs_move(entry: &(i64, String, String, String), root: &str) -> bool {
    entry.2 != root || entry.3 != entry.1
}

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
    for entry @ (id, destination, _, _) in &plan {
        if !needs_move(entry, root) {
            continue;
        }

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

    let mut completed = 0;

    for entry @ (id, destination, _, _) in &plan {
        if !needs_move(entry, root) {
            continue;
        }

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
        completed += 1;
        progress(completed);
        tokio::task::yield_now().await;
    }
    Ok(())
}

pub(crate) async fn recover(pool: &SqlitePool) -> Result<(), String> {
    crate::replacement::recover(pool).await?;

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
mod tests;
