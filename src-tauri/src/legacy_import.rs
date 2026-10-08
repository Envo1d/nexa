//! Explicit, read-only legacy import. Files and legacy settings are never changed.
use crate::database::Database;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sqlx::{ConnectOptions, Connection, Row, Sqlite, SqliteConnection, Transaction};
use std::{
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use unicode_normalization::UnicodeNormalization;

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

/// Version 1: canonical decomposition, full default Unicode case folding, NFC.
/// Preserve punctuation/accents/internal spacing; trim surrounding whitespace.
fn name_key(value: &str) -> String {
    let decomposed: String = value.trim().nfd().collect();
    caseless::default_case_fold_str(&decomposed).nfc().collect()
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
    database: tauri::State<'_, Database>,
    state: tauri::State<'_, ImportState>,
) -> Result<ImportReport, String> {
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
    Ok(sqlx::query_scalar("INSERT INTO sources(kind, root_path, path_key, is_active, created_at) VALUES (?, ?, ?, 0, ?) RETURNING id")
        .bind(kind).bind(root).bind(root).bind(now).fetch_one(&mut **tx).await?)
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
            "INSERT INTO artists(name, name_key) VALUES (?, ?) ON CONFLICT(name_key) DO UPDATE SET name_key = excluded.name_key RETURNING id"
        }
        Dictionary::Copyright => {
            "INSERT INTO copyrights(name, name_key) VALUES (?, ?) ON CONFLICT(name_key) DO UPDATE SET name_key = excluded.name_key RETURNING id"
        }
        Dictionary::Tag => {
            "INSERT INTO tags(name, name_key) VALUES (?, ?) ON CONFLICT(name_key) DO UPDATE SET name_key = excluded.name_key RETURNING id"
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
            "SELECT rowid AS legacy_id, path, media_type, modified, last_seen_scan, '' AS copyright, '' AS artist, '' AS characters, '' AS tags FROM staging WHERE rowid >= ? ORDER BY rowid LIMIT 500"
        }
        (true, 1) => {
            "SELECT rowid AS legacy_id, path, media_type, NULL AS modified, NULL AS last_seen_scan, category AS copyright, author AS artist, '' AS characters, '' AS tags FROM media WHERE rowid >= ? ORDER BY rowid LIMIT 500"
        }
        (true, 2..=4) => {
            "SELECT rowid AS legacy_id, path, media_type, modified, NULL AS last_seen_scan, category AS copyright, author AS artist, '' AS characters, '' AS tags FROM media WHERE rowid >= ? ORDER BY rowid LIMIT 500"
        }
        (true, 5..=6) => {
            "SELECT rowid AS legacy_id, path, media_type, modified, last_seen_scan, category AS copyright, author AS artist, '' AS characters, '' AS tags FROM media WHERE rowid >= ? ORDER BY rowid LIMIT 500"
        }
        (true, _) => {
            "SELECT rowid AS legacy_id, path, media_type, modified, last_seen_scan, copyright, artist, characters, tags FROM media WHERE rowid >= ? ORDER BY rowid LIMIT 500"
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
            let media_id: i64 = sqlx::query_scalar("INSERT INTO media(source_id, artist_id, relative_path, original_filename, path_key, kind, extension, availability, file_modified_at, last_seen_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id")
                .bind(source_id).bind(artist_id).bind(&relative).bind(original).bind(&relative).bind(kind)
                .bind(extension).bind(availability).bind(modified).bind(seen).bind(now).bind(now)
                .fetch_one(&mut **destination).await?;
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
                    let character_id: i64 = sqlx::query_scalar("INSERT INTO characters(copyright_id, name, name_key) VALUES (?, ?, ?) ON CONFLICT(copyright_id, name_key) DO UPDATE SET name_key = excluded.name_key RETURNING id")
                        .bind(copyright_id).bind(&name).bind(name_key(&name)).fetch_one(&mut **destination).await?;
                    sqlx::query("INSERT INTO media_characters(media_id, character_id, copyright_id) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
                        .bind(media_id).bind(character_id).bind(copyright_id).execute(&mut **destination).await?;
                }
            }
            let tags: String = row.try_get("tags")?;
            for name in tags.split('|') {
                if let Some(tag_id) = dictionary_id(destination, Dictionary::Tag, name).await? {
                    sqlx::query("INSERT INTO media_tags(media_id, tag_id) VALUES (?, ?) ON CONFLICT DO NOTHING")
                        .bind(media_id).bind(tag_id).execute(&mut **destination).await?;
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
mod tests {
    use super::*;
    use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

    struct Fixture {
        _directory: tempfile::TempDir,
        paths: LegacyPaths,
        library: PathBuf,
        stage: PathBuf,
        old: SqlitePool,
        new: Database,
    }

    impl Fixture {
        async fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let library = directory.path().join("Медиа");
            let stage = directory.path().join("Входящие");
            std::fs::create_dir_all(&library).unwrap();
            std::fs::create_dir_all(&stage).unwrap();
            let paths = LegacyPaths {
                database: directory.path().join("old.db"),
                settings: directory.path().join("settings.json"),
            };
            std::fs::write(
                &paths.settings,
                serde_json::to_vec(&serde_json::json!({
                    "library_path": library, "staging_path": stage,
                    "database_path": "unused-field", "folder_mapping": {"copyright_depth": 0}
                }))
                .unwrap(),
            )
            .unwrap();
            let old = SqlitePool::connect_with(
                SqliteConnectOptions::new()
                    .filename(&paths.database)
                    .create_if_missing(true)
                    .collation("NATURALSORT", |left, right| left.cmp(right)),
            )
            .await
            .unwrap();
            sqlx::raw_sql("CREATE TABLE schema_version(version INTEGER NOT NULL);
                INSERT INTO schema_version VALUES(10);
                CREATE TABLE media(path TEXT PRIMARY KEY, name TEXT NOT NULL,
                    copyright TEXT NOT NULL, artist TEXT NOT NULL, media_type TEXT NOT NULL,
                    modified INTEGER DEFAULT 0, last_seen_scan INTEGER DEFAULT 0,
                    characters TEXT NOT NULL DEFAULT '', tags TEXT NOT NULL DEFAULT '');
                CREATE INDEX idx_media_name ON media(name COLLATE NATURALSORT);
                CREATE TABLE staging(path TEXT PRIMARY KEY, name TEXT NOT NULL,
                    media_type TEXT NOT NULL, modified INTEGER DEFAULT 0, last_seen_scan INTEGER DEFAULT 0);
                CREATE INDEX idx_staging_name ON staging(name COLLATE NATURALSORT);
                CREATE VIRTUAL TABLE media_fts USING fts5(path, name, copyright, artist, characters, tags);
                CREATE TRIGGER trg_media_ai AFTER INSERT ON media BEGIN
                    INSERT INTO media_fts(rowid, path, name, copyright, artist, characters, tags)
                    VALUES(new.rowid, new.path, new.name, new.copyright, new.artist, new.characters, new.tags);
                END;")
                .execute(&old).await.unwrap();
            let new = Database::open(&directory.path().join("new/vault.db"))
                .await
                .unwrap();
            Self {
                _directory: directory,
                paths,
                library,
                stage,
                old,
                new,
            }
        }

        async fn media(&self, filename: &str, copyright: &str, characters: &str, tags: &str) {
            sqlx::query("INSERT INTO media(path,name,copyright,artist,media_type,modified,last_seen_scan,characters,tags) VALUES(?,?,?,'Artist','Image',123,123000,?,?)")
                .bind(self.library.join(filename).to_str().unwrap()).bind(filename)
                .bind(copyright).bind(characters).bind(tags).execute(&self.old).await.unwrap();
        }

        async fn count(&self, statement: &'static str) -> i64 {
            sqlx::query_scalar(statement)
                .fetch_one(self.new.pool())
                .await
                .unwrap()
        }

        async fn close(self) {
            self.old.close().await;
            self.new.close().await;
        }
    }

    #[test]
    fn normalizes_unicode_tag_identity_and_keeps_accents_distinct() {
        assert_eq!(name_key(" STRASSE "), name_key("Straße"));
        assert_eq!(name_key("CAFÉ"), name_key("Cafe\u{301}"));
        assert_eq!(name_key("Σ"), name_key("ς"));
        assert_ne!(name_key("cafe"), name_key("café"));
    }

    #[tokio::test]
    async fn imports_qualified_characters_unique_tags_stage_and_preserves_source() {
        let fixture = Fixture::new().await;
        fixture
            .media(
                "one.png",
                "World A",
                "Alice|ALICE",
                "Tag|tag| TAG |Straße|STRASSE|Cafe\u{301}|CAFÉ",
            )
            .await;
        fixture.media("two.png", "World B", "Alice", "TAG").await;
        std::fs::write(fixture.library.join("one.png"), b"unchanged media").unwrap();
        sqlx::query("INSERT INTO staging VALUES(?, 'incoming.mp4', 'Video', 456, 456000)")
            .bind(fixture.stage.join("incoming.mp4").to_str().unwrap())
            .execute(&fixture.old)
            .await
            .unwrap();
        sqlx::query("INSERT INTO tags(name,name_key) VALUES('Existing TAG','tag')")
            .execute(fixture.new.pool())
            .await
            .unwrap();
        // Close source writer so file bytes can be compared, including FTS/indexes.
        fixture.old.close().await;
        let source_before = std::fs::read(&fixture.paths.database).unwrap();
        let settings_before = std::fs::read(&fixture.paths.settings).unwrap();
        let result = import(&fixture.paths, &fixture.new).await.unwrap();
        assert_eq!(
            (result.library_added, result.stage_added, result.skipped),
            (2, 1, 0)
        );
        assert_eq!(fixture.count("SELECT count(*) FROM characters").await, 2);
        assert_eq!(fixture.count("SELECT count(*) FROM tags").await, 3);
        assert_eq!(fixture.count("SELECT count(*) FROM media_tags").await, 4);
        assert_eq!(
            fixture.count("SELECT count(*) FROM media_characters").await,
            2
        );
        assert_eq!(fixture.count("SELECT count(*) FROM media_characters mc JOIN characters c ON c.id=mc.character_id JOIN copyrights cp ON cp.id=c.copyright_id WHERE cp.id=mc.copyright_id AND c.name='Alice'").await, 2);
        assert_eq!(
            fixture
                .count("SELECT count(*) FROM media WHERE availability='missing'")
                .await,
            2
        );
        assert_eq!(
            fixture
                .count("SELECT count(*) FROM media WHERE file_modified_at IN (123000,456000)")
                .await,
            3
        );
        assert_eq!(
            fixture
                .count("SELECT count(*) FROM sources WHERE is_active=1")
                .await,
            0
        );
        assert_eq!(
            fixture
                .count("SELECT count(*) FROM pragma_foreign_key_check")
                .await,
            0
        );
        assert_eq!(
            std::fs::read(&fixture.paths.database).unwrap(),
            source_before
        );
        assert_eq!(
            std::fs::read(&fixture.paths.settings).unwrap(),
            settings_before
        );
        assert_eq!(
            std::fs::read(fixture.library.join("one.png")).unwrap(),
            b"unchanged media"
        );
        // Repeat import must preserve new annotations and stable media identities.
        sqlx::query("UPDATE media SET favorite=1")
            .execute(fixture.new.pool())
            .await
            .unwrap();
        let result = import(&fixture.paths, &fixture.new).await.unwrap();
        assert_eq!(
            (result.library_added, result.stage_added, result.skipped),
            (0, 0, 3)
        );
        assert_eq!(
            fixture
                .count("SELECT count(*) FROM media WHERE favorite=1")
                .await,
            3
        );
        assert_eq!(fixture.count("SELECT count(*) FROM sources").await, 2);
        assert_eq!(
            fixture
                .count("SELECT count(*) FROM tags WHERE name='Existing TAG'")
                .await,
            1
        );
        fixture.close().await;
    }

    #[tokio::test]
    async fn rolls_back_everything_when_character_copyright_is_missing() {
        let fixture = Fixture::new().await;
        fixture.media("one.png", "World", "Alice", "Tag").await;
        fixture.media("two.png", "", "Alice", "Tag").await;
        assert!(matches!(
            import(&fixture.paths, &fixture.new).await,
            Err(ImportError::Record { row: 2, .. })
        ));
        for statement in [
            "SELECT count(*) FROM media",
            "SELECT count(*) FROM sources",
            "SELECT count(*) FROM artists",
            "SELECT count(*) FROM copyrights",
            "SELECT count(*) FROM characters",
            "SELECT count(*) FROM tags",
        ] {
            assert_eq!(fixture.count(statement).await, 0);
        }
        fixture.close().await;
    }

    #[tokio::test]
    async fn rejects_escaping_paths_missing_roots_and_invalid_types_atomically() {
        let fixture = Fixture::new().await;
        fixture.media("one.png", "World", "Alice", "Tag").await;
        fixture
            .media("../escape.png", "World", "Alice", "Tag")
            .await;
        assert!(import(&fixture.paths, &fixture.new).await.is_err());
        assert_eq!(fixture.count("SELECT count(*) FROM media").await, 0);
        sqlx::query("DELETE FROM media WHERE rowid=2")
            .execute(&fixture.old)
            .await
            .unwrap();
        sqlx::query("UPDATE media SET media_type='Other'")
            .execute(&fixture.old)
            .await
            .unwrap();
        assert!(import(&fixture.paths, &fixture.new).await.is_err());
        sqlx::query("UPDATE media SET media_type='Image'")
            .execute(&fixture.old)
            .await
            .unwrap();
        std::fs::write(&fixture.paths.settings, b"{}").unwrap();
        assert!(import(&fixture.paths, &fixture.new).await.is_err());
        assert_eq!(fixture.count("SELECT count(*) FROM tags").await, 0);
        fixture.close().await;
    }

    #[tokio::test]
    async fn imports_batches_without_losing_records() {
        let fixture = Fixture::new().await;
        for index in 0..503 {
            fixture
                .media(&format!("{index}.png"), "World", "Alice", "Tag|TAG")
                .await;
        }
        let result = import(&fixture.paths, &fixture.new).await.unwrap();
        assert_eq!(result.library_added, 503);
        assert_eq!(fixture.count("SELECT count(*) FROM tags").await, 1);
        assert_eq!(fixture.count("SELECT count(*) FROM characters").await, 1);
        assert_eq!(fixture.count("SELECT count(*) FROM media_tags").await, 503);
        fixture.close().await;
    }

    #[tokio::test]
    async fn rejects_unknown_versions_corruption_and_missing_database() {
        let fixture = Fixture::new().await;
        sqlx::query("UPDATE schema_version SET version=11")
            .execute(&fixture.old)
            .await
            .unwrap();
        assert!(matches!(
            import(&fixture.paths, &fixture.new).await,
            Err(ImportError::Schema)
        ));
        sqlx::query("UPDATE schema_version SET version=10; INSERT INTO schema_version VALUES(10)")
            .execute(&fixture.old)
            .await
            .unwrap();
        assert!(matches!(
            import(&fixture.paths, &fixture.new).await,
            Err(ImportError::Schema)
        ));
        fixture.old.close().await;
        std::fs::write(&fixture.paths.database, b"corrupt").unwrap();
        assert!(import(&fixture.paths, &fixture.new).await.is_err());
        assert_eq!(std::fs::read(&fixture.paths.database).unwrap(), b"corrupt");
        std::fs::remove_file(&fixture.paths.database).unwrap();
        assert!(matches!(
            import(&fixture.paths, &fixture.new).await,
            Err(ImportError::Missing)
        ));
        assert!(!fixture.paths.database.exists());
        fixture.close().await;
    }

    #[tokio::test]
    async fn supports_early_schema_and_reuses_existing_active_source() {
        let fixture = Fixture::new().await;
        sqlx::raw_sql(
            "DROP TRIGGER trg_media_ai; DROP TABLE media; DROP TABLE staging;
            UPDATE schema_version SET version=1;
            CREATE TABLE media(path TEXT PRIMARY KEY, name TEXT NOT NULL, category TEXT NOT NULL,
                author TEXT NOT NULL, media_type TEXT NOT NULL);",
        )
        .execute(&fixture.old)
        .await
        .unwrap();
        sqlx::query("INSERT INTO media VALUES(?, 'old.png', 'World', 'Artist', 'Image')")
            .bind(fixture.library.join("old.png").to_str().unwrap())
            .execute(&fixture.old)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO sources(kind,root_path,path_key,created_at) VALUES('library',?,?,1)",
        )
        .bind(fixture.library.to_str().unwrap())
        .bind(fixture.library.to_str().unwrap())
        .execute(fixture.new.pool())
        .await
        .unwrap();
        let result = import(&fixture.paths, &fixture.new).await.unwrap();
        assert_eq!(result.library_added, 1);
        assert_eq!(
            fixture
                .count("SELECT count(*) FROM sources WHERE is_active=1")
                .await,
            1
        );
        assert_eq!(
            fixture
                .count("SELECT count(*) FROM media WHERE file_modified_at IS NULL")
                .await,
            1
        );
        fixture.close().await;
    }

    #[tokio::test]
    async fn stage_failure_rolls_back_library_and_preserves_existing_new_data() {
        let fixture = Fixture::new().await;
        fixture.media("one.png", "World", "Alice", "Tag").await;
        sqlx::query("INSERT INTO tags(name,name_key) VALUES('Keep','keep')")
            .execute(fixture.new.pool())
            .await
            .unwrap();
        sqlx::query("INSERT INTO staging VALUES(?, 'bad.mp4', 'Video', 1, 1)")
            .bind(fixture.library.join("outside-stage.mp4").to_str().unwrap())
            .execute(&fixture.old)
            .await
            .unwrap();
        assert!(import(&fixture.paths, &fixture.new).await.is_err());
        assert_eq!(fixture.count("SELECT count(*) FROM media").await, 0);
        assert_eq!(
            fixture
                .count("SELECT count(*) FROM tags WHERE name='Keep'")
                .await,
            1
        );
        assert_eq!(fixture.count("SELECT count(*) FROM tags").await, 1);
        fixture.close().await;
    }

    #[tokio::test]
    async fn preserves_both_extreme_rowids() {
        let fixture = Fixture::new().await;
        fixture.media("one.png", "World", "Alice", "Tag").await;
        fixture.media("two.png", "World", "Alice", "Tag").await;
        sqlx::query("UPDATE media SET rowid=? WHERE name='one.png'")
            .bind(i64::MIN)
            .execute(&fixture.old)
            .await
            .unwrap();
        sqlx::query("UPDATE media SET rowid=? WHERE name='two.png'")
            .bind(i64::MAX)
            .execute(&fixture.old)
            .await
            .unwrap();
        assert_eq!(
            import(&fixture.paths, &fixture.new)
                .await
                .unwrap()
                .library_added,
            2
        );
        fixture.close().await;
    }
}
