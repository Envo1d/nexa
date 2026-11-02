//! Preserve catalog identity only for verified same-file moves within one source.

use std::{collections::HashMap, path::Path};

use sqlx::SqlitePool;

use super::{Discovered, inspect};
use crate::{media_paths::checked_action_path, naming::name_key};

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct Identity {
    volume: u64,
    object: u64,
    created: u64,
}

#[cfg(windows)]
fn identity(path: &Path) -> Option<Identity> {
    use std::{fs::File, os::windows::io::AsRawHandle};

    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
    };

    let file = File::open(path).ok()?;
    let mut information = BY_HANDLE_FILE_INFORMATION::default();

    // The owning File keeps the handle alive; the output is a valid writable struct.
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut information).ok()?;
    }

    // Hard links are deliberately ambiguous, including links outside the source.
    let missing_identity = information.nFileIndexHigh == 0 && information.nFileIndexLow == 0;

    if information.nNumberOfLinks != 1 || missing_identity {
        return None;
    }

    Some(Identity {
        volume: u64::from(information.dwVolumeSerialNumber),
        object: (u64::from(information.nFileIndexHigh) << 32)
            | u64::from(information.nFileIndexLow),
        created: (u64::from(information.ftCreationTime.dwHighDateTime) << 32)
            | u64::from(information.ftCreationTime.dwLowDateTime),
    })
}

#[cfg(not(windows))]
fn identity(_path: &Path) -> Option<Identity> {
    None
}

#[derive(Clone)]
struct Tracked {
    relative: String,
    revision: String,
}

#[derive(Default)]
pub(super) struct RenameTracker {
    previous: HashMap<Identity, Tracked>,
    current: HashMap<Identity, Tracked>,
}

struct Candidate {
    identity: Identity,
    file: Discovered,
}

impl RenameTracker {
    pub(super) fn begin(&mut self) {
        self.current.clear();
    }

    pub(super) fn finish(&mut self, complete: bool) {
        if complete {
            self.previous = std::mem::take(&mut self.current);
        } else {
            self.current.clear();
        }
    }

    pub(super) async fn observe(
        &mut self,
        pool: &SqlitePool,
        source: i64,
        root: &Path,
        files: &[Discovered],
    ) -> Result<(), String> {
        let observations = files.to_vec();
        let checked_root = root.to_path_buf();
        let candidates = tauri::async_runtime::spawn_blocking(move || {
            let mut candidates = Vec::new();

            for file in observations {
                let Ok(path) = checked_action_path(&checked_root, &file.relative) else {
                    continue;
                };
                let Some(id) = identity(&path) else {
                    continue;
                };
                let stable = inspect(path, &checked_root)
                    .ok()
                    .flatten()
                    .is_some_and(|current| current.revision == file.revision);

                if !stable {
                    continue;
                }

                candidates.push(Candidate { identity: id, file });
            }

            candidates
        })
        .await
        .map_err(|_| "File identity worker failed")?;

        for Candidate { identity, file } in candidates {
            // Follow repeated renames within this scan as well as the previous baseline.
            let old = self
                .current
                .get(&identity)
                .or_else(|| self.previous.get(&identity))
                .filter(|old| old.relative != file.relative)
                .cloned();

            if let Some(old) = old {
                let check_root = root.to_path_buf();
                let check_file = file.clone();
                let old_relative = old.relative.clone();
                let confirmed = tauri::async_runtime::spawn_blocking(move || {
                    let old_path = check_root.join(&old_relative);
                    // Permission errors and dangling symlinks do not prove absence.
                    let old_gone = match std::fs::symlink_metadata(&old_path) {
                        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
                        Ok(_) => {
                            cfg!(windows)
                                && name_key(&old_relative) == name_key(&check_file.relative)
                                && self::identity(&old_path) == Some(identity)
                        }
                    };
                    let target = checked_action_path(&check_root, &check_file.relative).ok();
                    let same_file = target
                        .as_ref()
                        .is_some_and(|path| self::identity(path) == Some(identity));
                    let stable = target
                        .and_then(|path| inspect(path, &check_root).ok().flatten())
                        .is_some_and(|current| current.revision == check_file.revision);

                    old_gone && same_file && stable
                })
                .await
                .map_err(|_| "Rename validation worker failed")?;

                if !confirmed {
                    self.current.insert(
                        identity,
                        Tracked {
                            relative: file.relative,
                            revision: file.revision,
                        },
                    );
                    continue;
                }

                let kind = if file.kind == crate::media_metadata::MediaKind::Video {
                    "video"
                } else {
                    "image"
                };
                let changed = sqlx::query(concat!(
                    "UPDATE media SET relative_path=?,path_key=?,extension=?,",
                    "stage_search_key=?,updated_at=? ",
                    "WHERE source_id=? AND path_key=? AND file_revision=? AND kind=? ",
                    "AND NOT EXISTS(SELECT 1 FROM media destination WHERE destination.source_id=? ",
                    "AND destination.path_key=?)",
                    " AND NOT EXISTS(SELECT 1 FROM file_operations pending WHERE pending.media_id=media.id)",
                    " AND NOT EXISTS(SELECT 1 FROM media_replacements pending WHERE pending.finished=0 ",
                    "AND (pending.main_id=media.id OR pending.stage_id=media.id))",
                ))
                .bind(&file.relative)
                .bind(&file.relative)
                .bind(&file.extension)
                .bind(name_key(&file.path.to_string_lossy().replace('\\', "/")))
                .bind(super::now())
                .bind(source)
                .bind(&old.relative)
                .bind(&old.revision)
                .bind(kind)
                .bind(source)
                .bind(&file.relative)
                .execute(pool)
                .await
                .map_err(|error| error.to_string())?
                .rows_affected();

                if changed != 0 {
                    tracing::info!(
                        source_id = source,
                        "External media rename adopted; catalog identity preserved"
                    );
                }
            }

            self.current.insert(
                identity,
                Tracked {
                    relative: file.relative,
                    revision: file.revision,
                },
            );
        }

        Ok(())
    }
}

#[cfg(all(test, windows))]
mod tests;
