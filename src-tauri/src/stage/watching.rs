//! Bounded notifications trigger the existing serialized, revision-aware scanner.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::Manager;

use super::{ScanStatus, StageState, scanning};
use crate::{
    config::{AppConfig, ConfigStore},
    database::Database,
};

const QUIET: Duration = Duration::from_secs(2);
const MAX_WAIT: Duration = Duration::from_secs(10);
const REATTACH: Duration = Duration::from_secs(30);

struct Pending {
    first: Instant,
    last: Instant,
}

impl Pending {
    fn new(now: Instant) -> Self {
        Self {
            first: now,
            last: now,
        }
    }

    fn ready(&self, now: Instant) -> bool {
        now >= self.last + QUIET || now >= self.first + MAX_WAIT
    }
}

struct SourceWatch {
    root: PathBuf,
    kind: &'static str,
    excluded: Option<PathBuf>,
    watcher: Option<RecommendedWatcher>,
    changed: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    pending: Option<Pending>,
    attempted: Option<Instant>,
    retries: u8,
    renames: super::renames::RenameTracker,
}

fn relevant(event: &Event, excluded: Option<&Path>) -> bool {
    if event.need_rescan() {
        return true;
    }

    let excluded_only = excluded.is_some_and(|root| {
        !event.paths.is_empty() && event.paths.iter().all(|path| path.starts_with(root))
    });

    !matches!(event.kind, EventKind::Access(_)) && !excluded_only
}

impl SourceWatch {
    fn new(root: PathBuf, kind: &'static str, excluded: Option<PathBuf>) -> Self {
        Self {
            root,
            kind,
            excluded,
            watcher: None,
            changed: Arc::new(AtomicBool::new(false)),
            failed: Arc::new(AtomicBool::new(false)),
            pending: None,
            attempted: None,
            retries: 0,
            renames: super::renames::RenameTracker::default(),
        }
    }

    // Runs on a blocking worker: registration and root probes may touch slow disks.
    fn maintain(mut self) -> Self {
        let now = Instant::now();
        let failed = self.failed.swap(false, Ordering::AcqRel);

        if self.watcher.is_some() && (failed || !self.root.is_dir()) {
            self.watcher = None;
            self.attempted = None;
            tracing::warn!(
                source_kind = self.kind,
                "Folder watch interrupted; reattachment scheduled"
            );
        }

        let retry_due = self.attempted.is_none_or(|last| now >= last + REATTACH);

        if self.watcher.is_none() && retry_due {
            let first_attempt = self.attempted.is_none();
            self.attempted = Some(now);
            let changed = self.changed.clone();
            let failed = self.failed.clone();
            let excluded = self.excluded.clone();
            let registration = (|| {
                let mut watcher = notify::recommended_watcher(
                    move |result: notify::Result<Event>| match result {
                        Ok(event) if relevant(&event, excluded.as_deref()) => {
                            changed.store(true, Ordering::Release);
                        }
                        Err(_) => {
                            failed.store(true, Ordering::Release);
                            changed.store(true, Ordering::Release);
                        }
                        _ => {}
                    },
                )?;
                watcher.watch(&self.root, RecursiveMode::Recursive)?;

                Ok::<_, notify::Error>(watcher)
            })();

            match registration {
                Ok(watcher) => {
                    self.watcher = Some(watcher);
                    tracing::info!(source_kind = self.kind, "Recursive folder watch attached");
                }
                Err(_) if first_attempt => {
                    tracing::warn!(
                        source_kind = self.kind,
                        "Folder watch unavailable; retrying with periodic reconciliation"
                    );
                }
                Err(_) => {}
            }

            // Establish a baseline after registration, including newly reconnected roots.
            // Failed native registrations also get a periodic scanner fallback.
            self.changed.store(true, Ordering::Release);
        }

        if self.changed.swap(false, Ordering::AcqRel) {
            self.pending.get_or_insert_with(|| Pending::new(now)).last = now;
            self.retries = 0;
        }

        self
    }

    fn finished(&mut self, errors: u64) {
        self.pending = None;

        if errors != 0 && self.retries < 3 {
            self.retries += 1;
            self.pending = Some(Pending::new(Instant::now() + Duration::from_secs(5)));
            tracing::debug!(
                source_kind = self.kind,
                retry = self.retries,
                "Incomplete automatic scan scheduled for retry"
            );
        }
    }
}

fn sources(settings: &AppConfig) -> Vec<(PathBuf, &'static str, Option<PathBuf>)> {
    if !settings.watch_folders {
        return Vec::new();
    }

    [
        (
            settings.library_path.clone(),
            "library",
            settings.staging_path.clone(),
        ),
        (settings.staging_path.clone(), "stage", None),
    ]
    .into_iter()
    .filter_map(|(root, kind, excluded)| root.map(|root| (root, kind, excluded)))
    .collect()
}

pub(crate) fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut watches: Vec<SourceWatch> = Vec::new();

        tracing::info!("Folder change monitoring service started");

        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let Ok(snapshot) = app.state::<ConfigStore>().snapshot() else {
                continue;
            };
            let desired = sources(&snapshot.settings);

            watches.retain(|watch| {
                let keep = desired.iter().any(|(root, kind, excluded)| {
                    watch.root == *root && watch.kind == *kind && watch.excluded == *excluded
                });

                if !keep {
                    tracing::info!(source_kind = watch.kind, "Folder watch stopped");
                }

                keep
            });

            for (root, kind, excluded) in desired {
                if !watches
                    .iter()
                    .any(|watch| watch.root == root && watch.kind == kind)
                {
                    watches.push(SourceWatch::new(root, kind, excluded));
                }
            }

            watches = match tauri::async_runtime::spawn_blocking(move || {
                watches.into_iter().map(SourceWatch::maintain).collect()
            })
            .await
            {
                Ok(watches) => watches,
                Err(_) => {
                    tracing::error!("Folder watch worker failed; registrations will be recreated");
                    Vec::new()
                }
            };

            for watch in &mut watches {
                if !watch
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.ready(Instant::now()))
                {
                    continue;
                }

                let state = app.state::<StageState>();
                let Some(_guard) = state.try_operation() else {
                    continue;
                };

                // Recheck saved settings under the same guard as saves and restores.
                let Ok(snapshot) = app.state::<ConfigStore>().snapshot() else {
                    continue;
                };
                let settings = snapshot.settings;
                let current = if watch.kind == "library" {
                    &settings.library_path
                } else {
                    &settings.staging_path
                };

                if !settings.watch_folders || current.as_ref() != Some(&watch.root) {
                    continue;
                }

                tracing::debug!(
                    source_kind = watch.kind,
                    "Reconciling pending folder notifications"
                );
                scanning::publish(
                    &app,
                    &ScanStatus {
                        running: true,
                        message: "Starting media scan…".into(),
                        ..Default::default()
                    },
                );

                let result = scanning::scan_watched_source(
                    app.state::<Database>().pool(),
                    watch.root.clone(),
                    watch.kind,
                    if watch.kind == "library" {
                        settings.staging_path
                    } else {
                        None
                    },
                    |status| scanning::publish(&app, status),
                    &mut watch.renames,
                )
                .await;

                let status = match result {
                    Ok(status) => status,
                    Err(error) => {
                        tracing::warn!(
                            source_kind = watch.kind,
                            "Automatic scan failed; committed records preserved"
                        );
                        ScanStatus {
                            errors: 1,
                            message: error,
                            ..Default::default()
                        }
                    }
                };
                watch.finished(status.errors);
                scanning::publish(&app, &status);
                crate::disk_sync::request(&app);
            }
        }
    });
}

#[cfg(test)]
mod tests;
