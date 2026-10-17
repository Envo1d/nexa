//! Explicit, read-only legacy import. Files and legacy settings are never changed.
use std::{
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sqlx::{ConnectOptions, Connection, Row, Sqlite, SqliteConnection, Transaction};
use unicode_normalization::UnicodeNormalization;

use crate::database::Database;
use crate::naming::name_key;

#[derive(Default)]
pub struct ImportState(tokio::sync::Mutex<()>);

#[derive(Debug, thiserror::Error)]
enum ImportError {
    #[error("Could not locate legacy Nexa storage")]
    Location,
    #[error("Legacy database was not found")]
    Missing,
    #[error(
        "Could not read legacy settings; restore the old settings.json with its Library/Stage roots"
    )]
    Settings,
    #[error("Legacy schema version must contain one supported version (1–10)")]
    Schema,
    #[error("Invalid {table} row {row}: {reason}. Import was not applied")]
    Record {
        table: &'static str,
        row: i64,
        reason: &'static str,
    },
    #[error("Legacy root conflicts with an existing source")]
    SourceConflict,
    #[error("Could not inspect a legacy media file; check directory access")]
    FileAccess,
    #[error("Could not read or write the import database: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("System clock is outside the supported timestamp range")]
    Clock,
}

#[derive(Deserialize, Default)]
struct LegacySettings {
    library_path: Option<PathBuf>,
    staging_path: Option<PathBuf>,
}

struct LegacyPaths {
    database: PathBuf,
    settings: PathBuf,
}

impl LegacyPaths {
    fn resolve() -> Result<Self, ImportError> {
        let dirs = ProjectDirs::from("com", "Envo1d", "Nexa").ok_or(ImportError::Location)?;
        Ok(Self {
            database: dirs.data_local_dir().join("db/vault.db"),
            settings: dirs.config_dir().join("settings.json"),
        })
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    library_added: u64,
    stage_added: u64,
    skipped: u64,
}

#[tauri::command]
#[tracing::instrument(skip_all)]
pub async fn find_legacy_database() -> Result<bool, String> {
    let paths = LegacyPaths::resolve().map_err(|e| e.to_string())?;

    match tokio::fs::metadata(&paths.database).await {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => {
            tracing::warn!("Could not inspect legacy database location");
            Err("Could not check the legacy database location".into())
        }
    }
}

#[tauri::command]
#[tracing::instrument(skip_all)]
pub async fn import_legacy_database(
    app: tauri::AppHandle,
    media: tauri::State<'_, crate::stage::StageState>,
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, ImportState>,
) -> Result<ImportReport, String> {
    let _media_guard = media.operation().await;
    crate::disk_sync::recover(database.pool()).await?;

    let _guard = state
        .0
        .try_lock()
        .map_err(|_| "An import is already running")?;

    tracing::info!("Legacy import started");

    let result = match LegacyPaths::resolve() {
        Ok(paths) => import(&paths, &database).await,
        Err(error) => Err(error),
    };

    match result {
        Ok(report) => {
            if report.library_added > 0 {
                crate::disk_sync::request(&app);
            }

            tracing::info!(
                library_added = report.library_added,
                stage_added = report.stage_added,
                skipped = report.skipped,
                "Legacy import completed"
            );
            Ok(report)
        }
        Err(error) => {
            tracing::error!(%error, "Legacy import failed; destination transaction rolled back");
            Err(error.to_string())
        }
    }
}

async fn import(paths: &LegacyPaths, database: &Database) -> Result<ImportReport, ImportError> {
    if !paths.database.is_file() {
        return Err(ImportError::Missing);
    }

    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&paths.database)
        .read_only(true)
        .create_if_missing(false)
        .busy_timeout(std::time::Duration::from_secs(5))
        .disable_statement_logging();

    let mut legacy = SqliteConnection::connect_with(&options).await?;

    let mut snapshot = legacy.begin().await?;

    let versions: Vec<i64> = sqlx::query_scalar("SELECT version FROM schema_version")
        .fetch_all(&mut *snapshot)
        .await?;

    let version = match versions.as_slice() {
        [version @ 1..=10] => *version,
        _ => return Err(ImportError::Schema),
    };

    let settings = match tokio::fs::read(&paths.settings).await {
        Ok(bytes) => {
            serde_json::from_slice::<LegacySettings>(&bytes).map_err(|_| ImportError::Settings)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => LegacySettings::default(),
        Err(_) => return Err(ImportError::Settings),
    };

    let now = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ImportError::Clock)?
            .as_millis(),
    )
    .map_err(|_| ImportError::Clock)?;
    // Obtain the writer reservation before any dictionary/source reads. Serializes
    // concurrent writes and avoids upgrading a stale deferred SQLite snapshot.
    let mut destination = database.pool().begin_with("BEGIN IMMEDIATE").await?;

    let mut report = ImportReport::default();
    import_table(
        &mut snapshot,
        &mut destination,
        "media",
        version,
        settings.library_path.as_deref(),
        now,
        &mut report,
    )
    .await?;

    if version >= 8 {
        import_table(
            &mut snapshot,
            &mut destination,
            "staging",
            version,
            settings.staging_path.as_deref(),
            now,
            &mut report,
        )
        .await?;
    }
    // Finish the source snapshot before committing the destination. No operation
    // that can fail remains after the destination commit.
    snapshot.rollback().await?;
    destination.commit().await?;
    Ok(report)
}

fn record_error(table: &'static str, row: i64, reason: &'static str) -> ImportError {
    ImportError::Record { table, row, reason }
}

fn valid_root(root: &Path) -> bool {
    root.is_absolute()
        && !root
            .components()
            .any(|part| matches!(part, Component::ParentDir))
}

fn relative_file(root: &Path, path: &str) -> Option<(String, String, String)> {
    if !valid_root(root) {
        return None;
    }

    let path = Path::new(path);

    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return None;
    }

    let relative = path.strip_prefix(root).ok()?;

    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }

    let filename = relative.file_name()?.to_str()?.to_owned();

    if filename.trim().is_empty() {
        return None;
    }

    let extension = relative.extension()?.to_str()?.to_lowercase();

    if extension.trim().is_empty() {
        return None;
    }
    Some((relative.to_str()?.to_owned(), filename, extension))
}

async fn source_id(
    tx: &mut Transaction<'_, Sqlite>,
    kind: &str,
    root: &Path,
    now: i64,
) -> Result<i64, ImportError> {
    let normalized: PathBuf = root.components().collect();

    let root = normalized.to_str().ok_or(ImportError::Settings)?;

    let existing = sqlx::query("SELECT id, kind FROM sources WHERE path_key = ?")
        .bind(root)
        .fetch_optional(&mut **tx)
        .await?;

    if let Some(row) = existing {
        if row.get::<String, _>("kind") != kind {
            return Err(ImportError::SourceConflict);
        }

        return Ok(row.get("id"));
    }
    // Imported roots are retained but not silently activated or written to JSON.
    Ok(sqlx::query_scalar(concat!(
        "INSERT INTO sources(kind, root_path, path_key, is_active, created_at) VALUES (?, ?, ?,",
        " 0, ?) RETURNING id",
    ))
    .bind(kind)
    .bind(root)
    .bind(root)
    .bind(now)
    .fetch_one(&mut **tx)
    .await?)
}

#[derive(Clone, Copy)]
enum Dictionary {
    Artist,
    Copyright,
    Tag,
}

async fn dictionary_id(
    tx: &mut Transaction<'_, Sqlite>,
    dictionary: Dictionary,
    value: &str,
) -> Result<Option<i64>, ImportError> {
    let name: String = value.trim().nfc().collect();

    if name.is_empty() {
        return Ok(None);
    }

    let statement = match dictionary {
        Dictionary::Artist => {
            concat!(
                "INSERT INTO artists(name, name_key) VALUES (?, ?) ON CONFLICT(name_key) DO UPDATE SET ",
                "name_key = excluded.name_key RETURNING id",
            )
        }
        Dictionary::Copyright => {
            concat!(
                "INSERT INTO copyrights(name, name_key) VALUES (?, ?) ON CONFLICT(name_key) DO UPDATE ",
                "SET name_key = excluded.name_key RETURNING id",
            )
        }
        Dictionary::Tag => {
            concat!(
                "INSERT INTO tags(name, name_key) VALUES (?, ?) ON CONFLICT(name_key) DO UPDATE SET ",
                "name_key = excluded.name_key RETURNING id",
            )
        }
    };
    Ok(Some(
        sqlx::query_scalar(statement)
            .bind(&name)
            .bind(name_key(&name))
            .fetch_one(&mut **tx)
            .await?,
    ))
}

async fn import_table(
    legacy: &mut Transaction<'_, Sqlite>,
    destination: &mut Transaction<'_, Sqlite>,
    table: &'static str,
    version: i64,
    root: Option<&Path>,
    now: i64,
    report: &mut ImportReport,
) -> Result<(), ImportError> {
    let library = table == "media";

    let statement = match (library, version) {
        (false, _) => {
            concat!(
                "SELECT rowid AS legacy_id, path, media_type, modified, last_seen_scan, '' AS ",
                "copyright, '' AS artist, '' AS characters, '' AS tags FROM staging WHERE rowid >= ? ",
                "ORDER BY rowid LIMIT 500",
            )
        }
        (true, 1) => {
            concat!(
                "SELECT rowid AS legacy_id, path, media_type, NULL AS modified, NULL AS last_seen_scan,",
                " category AS copyright, author AS artist, '' AS characters, '' AS tags FROM media ",
                "WHERE rowid >= ? ORDER BY rowid LIMIT 500",
            )
        }
        (true, 2..=4) => {
            concat!(
                "SELECT rowid AS legacy_id, path, media_type, modified, NULL AS last_seen_scan, ",
                "category AS copyright, author AS artist, '' AS characters, '' AS tags FROM media ",
                "WHERE rowid >= ? ORDER BY rowid LIMIT 500",
            )
        }
        (true, 5..=6) => {
            concat!(
                "SELECT rowid AS legacy_id, path, media_type, modified, last_seen_scan, category AS ",
                "copyright, author AS artist, '' AS characters, '' AS tags FROM media WHERE rowid >= ? ",
                "ORDER BY rowid LIMIT 500",
            )
        }
        (true, _) => {
            concat!(
                "SELECT rowid AS legacy_id, path, media_type, modified, last_seen_scan, copyright, ",
                "artist, characters, tags FROM media WHERE rowid >= ? ORDER BY rowid LIMIT 500",
            )
        }
    };

    let mut cursor = i64::MIN;

    let mut source = None;

    loop {
        let rows = sqlx::query(statement)
            .bind(cursor)
            .fetch_all(&mut **legacy)
            .await?;

        if rows.is_empty() {
            break;
        }

        for row in rows {
            let row_id: i64 = row.try_get("legacy_id")?;
            cursor = row_id;

            let root = root.filter(|root| valid_root(root)).ok_or_else(|| {
                record_error(
                    table,
                    row_id,
                    "legacy source root is missing or invalid in settings.json",
                )
            })?;

            let path: String = row.try_get("path")?;

            let (relative, original, extension) = relative_file(root, &path).ok_or_else(|| {
                record_error(
                    table,
                    row_id,
                    "path is not a valid file beneath its legacy root",
                )
            })?;

            let kind = match row.try_get::<String, _>("media_type")?.as_str() {
                "Image" => "image",
                "Video" => "video",
                _ => return Err(record_error(table, row_id, "unsupported media type")),
            };

            let source_id = match source {
                Some(id) => id,
                None => {
                    let id = source_id(
                        destination,
                        if library { "library" } else { "stage" },
                        root,
                        now,
                    )
                    .await?;
                    source = Some(id);
                    id
                }
            };

            let existing: Option<i64> =
                sqlx::query_scalar("SELECT id FROM media WHERE source_id = ? AND path_key = ?")
                    .bind(source_id)
                    .bind(&relative)
                    .fetch_optional(&mut **destination)
                    .await?;

            if existing.is_some() {
                report.skipped += 1;
                continue;
            }

            let characters: String = row.try_get("characters")?;

            let copyright: String = row.try_get("copyright")?;

            if characters.split('|').any(|name| !name.trim().is_empty())
                && copyright.trim().is_empty()
            {
                return Err(record_error(
                    table,
                    row_id,
                    "characters have no copyright; assignment would be ambiguous",
                ));
            }

            let availability = match tokio::fs::metadata(&path).await {
                Ok(meta) if meta.is_file() => "present",
                Ok(_) => "missing",
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => "missing",
                Err(_) => return Err(ImportError::FileAccess),
            };

            let modified = row
                .try_get::<Option<i64>, _>("modified")?
                .filter(|time| *time != 0)
                .map(|time| {
                    time.checked_mul(1000).ok_or_else(|| {
                        record_error(table, row_id, "modification timestamp overflow")
                    })
                })
                .transpose()?;

            let seen = row
                .try_get::<Option<i64>, _>("last_seen_scan")?
                .filter(|time| *time > 0)
                .unwrap_or(now);

            let artist: String = row.try_get("artist")?;

            let artist_id = dictionary_id(destination, Dictionary::Artist, &artist).await?;

            let media_id: i64 = sqlx::query_scalar(concat!(
                "INSERT INTO media(source_id, artist_id, relative_path, original_filename, path_key, ",
                "kind, extension, availability, file_modified_at, last_seen_at, created_at, ",
                "updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
            ))
                .bind(source_id)
                .bind(artist_id)
                .bind(&relative)
                .bind(original)
                .bind(&relative)
                .bind(kind)
                .bind(extension)
                .bind(availability)
                .bind(modified)
                .bind(seen)
                .bind(now)
                .bind(now)
                .fetch_one(&mut **destination)
                .await?;

            if let Some(copyright_id) =
                dictionary_id(destination, Dictionary::Copyright, &copyright).await?
            {
                sqlx::query("INSERT INTO media_copyrights(media_id, copyright_id) VALUES (?, ?)")
                    .bind(media_id)
                    .bind(copyright_id)
                    .execute(&mut **destination)
                    .await?;

                for name in characters
                    .split('|')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                {
                    let name: String = name.nfc().collect();

                    let character_id: i64 = sqlx::query_scalar(concat!(
                        "INSERT INTO characters(copyright_id, name, name_key) VALUES (?, ?, ?) ON ",
                        "CONFLICT(copyright_id, name_key) DO UPDATE SET name_key = excluded.name_key RETURNING ",
                        "id",
                    ))
                        .bind(copyright_id)
                        .bind(&name)
                        .bind(name_key(&name))
                        .fetch_one(&mut **destination)
                        .await?;
                    sqlx::query(concat!(
                        "INSERT INTO media_characters(media_id, character_id, copyright_id) VALUES (?, ?, ?) ",
                        "ON CONFLICT DO NOTHING",
                    ))
                        .bind(media_id)
                        .bind(character_id)
                        .bind(copyright_id)
                        .execute(&mut **destination)
                        .await?;
                }
            }

            let tags: String = row.try_get("tags")?;

            for name in tags.split('|') {
                if let Some(tag_id) = dictionary_id(destination, Dictionary::Tag, name).await? {
                    sqlx::query("INSERT INTO media_tags(media_id, tag_id) VALUES (?, ?) ON CONFLICT DO NOTHING")
                        .bind(media_id)
                        .bind(tag_id)
                        .execute(&mut **destination)
                        .await?;
                }
            }

            if library {
                report.library_added += 1;
            } else {
                report.stage_added += 1;
            }
        }

        tracing::debug!(
            table,
            imported = report.library_added + report.stage_added,
            skipped = report.skipped,
            "Legacy import batch processed"
        );

        match cursor.checked_add(1) {
            Some(next) => cursor = next,
            None => break,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
