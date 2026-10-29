use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::config::AppConfig;

// Dependency order, fixed by this format version. Runtime caches are disposable.
const TABLES: &[&str] = &[
    "sources",
    "copyrights",
    "artists",
    "tags",
    "characters",
    "media",
    "video_metadata",
    "media_copyrights",
    "media_tags",
    "media_characters",
    "metadata_presets",
    "media_replacements",
    "duplicate_rejections",
];

const MAX_BYTES: u64 = 256 * 1024 * 1024;

pub(super) type Record = Map<String, Value>;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    format: String,
    version: u32,
    schema: i64,
    pub(super) created_at: u64,
    pub(super) tables: BTreeMap<String, Vec<Record>>,
}

impl Snapshot {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.format != "nexa-catalog-backup" || self.version != 1 || self.schema != 10 {
            return Err("Unsupported catalog backup format or database version.".into());
        }

        if self.tables.len() != TABLES.len()
            || TABLES.iter().any(|table| !self.tables.contains_key(*table))
        {
            return Err("The backup has missing or unsupported tables.".into());
        }

        for row in &self.tables["sources"] {
            let root = row
                .get("root_path")
                .and_then(Value::as_str)
                .ok_or("Invalid source root")?;

            if !Path::new(root).is_absolute()
                || root.contains('\0')
                || row.get("path_key").and_then(Value::as_str) != Some(root)
            {
                return Err("Invalid source root in backup.".into());
            }
        }

        for row in &self.tables["media"] {
            let relative = row
                .get("relative_path")
                .and_then(Value::as_str)
                .ok_or("Invalid media path")?;
            let key = row
                .get("path_key")
                .and_then(Value::as_str)
                .ok_or("Invalid media path key")?;

            if !super::safe_relative(relative) || !super::safe_relative(key) {
                return Err("Invalid relative media path in backup.".into());
            }
        }

        if self.tables["media_replacements"]
            .iter()
            .any(|row| row.get("finished").and_then(Value::as_i64) != Some(1))
        {
            return Err("Backups containing pending file recovery cannot be restored.".into());
        }
        Ok(())
    }
}

pub(super) fn check_roots(backup: &Snapshot, settings: &AppConfig) -> Result<(), String> {
    for row in &backup.tables["sources"] {
        let active = row
            .get("is_active")
            .and_then(Value::as_i64)
            .ok_or("Invalid source activation")?;

        if active != 1 {
            continue;
        }

        let configured = match row.get("kind").and_then(Value::as_str) {
            Some("library") => settings.library_path.as_deref(),
            Some("stage") => settings.staging_path.as_deref(),
            _ => return Err("Invalid source kind".into()),
        };
        let root = row["root_path"].as_str().ok_or("Invalid source root")?;

        if configured != Some(Path::new(root)) {
            return Err(
                "Set and save the Library/Stage folders shown in the backup before restoring."
                    .into(),
            );
        }
    }
    Ok(())
}

async fn settled(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<(), String> {
    let pending: i64 = sqlx::query_scalar(concat!(
        "SELECT (SELECT count(*) FROM file_operations) + ",
        "(SELECT count(*) FROM media_replacements WHERE finished=0)",
    ))
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;

    if pending != 0 {
        return Err(
            "Finish pending disk recovery before backing up or restoring the catalog.".into(),
        );
    }
    Ok(())
}

pub(super) async fn capture(pool: &SqlitePool) -> Result<Snapshot, String> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let snapshot = capture_in(&mut tx).await?;
    tx.rollback().await.map_err(|error| error.to_string())?;
    Ok(snapshot)
}

async fn capture_in(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<Snapshot, String> {
    settled(tx).await?;

    let schema = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success=1")
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    let mut tables = BTreeMap::new();

    for table in TABLES {
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
                .bind(table)
                .fetch_all(&mut **tx)
                .await
                .map_err(|error| error.to_string())?;
        let mut query = QueryBuilder::<Sqlite>::new("SELECT json_object(");

        for (index, column) in columns.iter().enumerate() {
            if index > 0 {
                query.push(",");
            }
            // Identifiers come exclusively from our migrated local schema.
            query.push_bind(column).push(",\"").push(column).push("\"");
        }
        query.push(") FROM ").push(table).push(" ORDER BY rowid");

        let json: Vec<String> = query
            .build_query_scalar()
            .fetch_all(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;
        let rows = json
            .into_iter()
            .map(|row| serde_json::from_str(&row))
            .collect::<Result<Vec<Record>, _>>()
            .map_err(|error| error.to_string())?;
        tables.insert((*table).to_owned(), rows);
    }

    Ok(Snapshot {
        format: "nexa-catalog-backup".into(),
        version: 1,
        schema,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis() as u64,
        tables,
    })
}

pub(super) async fn restore(
    pool: &SqlitePool,
    backup: &Snapshot,
    expected: &Snapshot,
) -> Result<(), String> {
    backup.validate()?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|error| error.to_string())?;
    let current = capture_in(&mut tx).await?;

    if current.schema != backup.schema {
        return Err("Unsupported catalog backup format or database version.".into());
    }

    if current.tables != expected.tables {
        return Err(
            "The catalog changed while saving the safety backup. Retry restoration.".into(),
        );
    }

    for table in ["duplicate_searches", "duplicate_fingerprints"]
        .into_iter()
        .chain(TABLES.iter().rev().copied())
    {
        QueryBuilder::<Sqlite>::new("DELETE FROM ")
            .push(table)
            .build()
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }

    for table in TABLES {
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
                .bind(table)
                .fetch_all(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;

        for row in &backup.tables[*table] {
            if row.len() != columns.len() || columns.iter().any(|column| !row.contains_key(column))
            {
                return Err("Backup columns do not match the current catalog schema.".into());
            }

            let mut query = QueryBuilder::<Sqlite>::new("INSERT INTO ");
            query.push(table).push("(");

            for (index, column) in columns.iter().enumerate() {
                if index > 0 {
                    query.push(",");
                }
                query.push("\"").push(column).push("\"");
            }
            query.push(") VALUES(");

            for (index, column) in columns.iter().enumerate() {
                if index > 0 {
                    query.push(",");
                }

                match &row[column] {
                    Value::Null => {
                        query.push_bind(Option::<String>::None);
                    }
                    Value::String(value) => {
                        query.push_bind(value);
                    }
                    Value::Number(value) if value.is_i64() => {
                        query.push_bind(value.as_i64());
                    }
                    _ => return Err("Invalid backup cell type.".into()),
                }
            }
            query
                .push(")")
                .build()
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
        }
    }

    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;

    if !violations.is_empty() {
        return Err("Invalid backup relationships.".into());
    }
    tx.commit().await.map_err(|error| error.to_string())
}

pub(super) async fn read(path: PathBuf) -> Result<(Snapshot, String), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|error| error.to_string())?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;

        if bytes.len() as u64 > MAX_BYTES {
            return Err("Catalog files are limited to 256 MiB.".into());
        }
        let digest = super::digest_hex(&bytes);
        let snapshot =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid catalog backup JSON.")?;
        Ok((snapshot, digest))
    })
    .await
    .map_err(|error| error.to_string())?
}

pub(super) async fn write(path: PathBuf, bytes: Vec<u8>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !path.is_absolute() || bytes.len() as u64 > MAX_BYTES {
            return Err(
                "Choose an absolute destination; catalog files are limited to 256 MiB.".into(),
            );
        }
        let parent = path.parent().ok_or("Invalid destination")?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
        temporary
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        temporary.persist_noclobber(path).map_err(
            |_| "The destination already exists or cannot be written. Choose a new filename.",
        )?;
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}
