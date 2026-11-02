use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
};

use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::{media_paths::checked_action_path, stage};

#[derive(Default, Serialize)]
pub(crate) struct Verification {
    pub(super) checked: u64,
    pub(super) missing: u64,
    pub(super) unavailable: u64,
    #[serde(skip)]
    pub(super) roots: BTreeSet<String>,
}

pub(super) async fn verify(pool: &SqlitePool) -> Result<Verification, String> {
    let mut cursor: Option<i64> = None;
    let mut report = Verification::default();

    loop {
        let rows = sqlx::query(concat!(
            "SELECT m.id,m.relative_path,m.file_revision,s.root_path FROM media m ",
            "JOIN sources s ON s.id=m.source_id WHERE (? IS NULL OR m.id>?) ORDER BY m.id LIMIT 128",
        ))
        .bind(cursor)
        .bind(cursor)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;

        if rows.is_empty() {
            break;
        }

        cursor = Some(rows.last().unwrap().get("id"));
        let probes = tauri::async_runtime::spawn_blocking(move || {
            let mut roots = HashMap::new();
            rows.into_iter()
                .map(|row| {
                    let root = PathBuf::from(row.get::<String, _>("root_path"));
                    let relative: String = row.get("relative_path");
                    let readable = *roots
                        .entry(root.clone())
                        .or_insert_with(|| root.is_dir() && std::fs::read_dir(&root).is_ok());
                    let state = if !readable {
                        None
                    } else {
                        match std::fs::symlink_metadata(root.join(&relative)) {
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                Some((false, false))
                            }
                            Ok(_) => checked_action_path(&root, &relative)
                                .ok()
                                .and_then(|path| stage::inspect(path, &root).ok().flatten())
                                .map(|file| {
                                    (
                                        true,
                                        Some(file.revision)
                                            != row.get::<Option<String>, _>("file_revision"),
                                    )
                                }),
                            Err(_) => None,
                        }
                    };
                    let unavailable_root = (!readable).then(|| root.to_string_lossy().into_owned());
                    (row.get::<i64, _>("id"), state, unavailable_root)
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|_| "Health verification worker failed")?;

        let mut tx = pool.begin().await.map_err(|error| error.to_string())?;

        for (id, state, root) in probes {
            if let Some(root) = root {
                report.roots.insert(root);
            }
            report.checked += 1;
            let Some((present, changed)) = state else {
                report.unavailable += 1;
                continue;
            };
            if !present {
                report.missing += 1;
            }

            sqlx::query(concat!(
                "UPDATE media SET availability=CASE WHEN ? THEN availability ELSE 'missing' END,",
                "metadata_state=CASE WHEN ? THEN 'pending' ",
                "ELSE metadata_state END,updated_at=? WHERE id=? ",
                "AND NOT EXISTS(SELECT 1 FROM file_operations o WHERE o.media_id=media.id) ",
                "AND NOT EXISTS(SELECT 1 FROM media_replacements r WHERE r.finished=0 ",
                "AND (r.main_id=media.id OR r.stage_id=media.id))",
            ))
            .bind(present)
            .bind(changed)
            .bind(stage::now())
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
        }

        tx.commit().await.map_err(|error| error.to_string())?;
    }

    if report.unavailable != 0 {
        tracing::warn!(
            unavailable = report.unavailable,
            "Health check preserved unverified files and unavailable roots"
        );
    }

    Ok(report)
}
