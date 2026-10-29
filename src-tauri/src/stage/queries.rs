//! stage: queries responsibility.

use std::path::{Path, PathBuf};

use serde::Serialize;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use crate::{config::ConfigStore, database::Database};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageFile {
    pub(crate) id: String,
    pub(super) path: String,
    pub(super) filename: String,
    pub(crate) kind: String,
    pub(super) extension: String,
    pub(super) size: Option<i64>,
    pub(crate) width: Option<i64>,
    pub(crate) height: Option<i64>,
    pub(super) modified_at: Option<i64>,
    pub(super) created_at: Option<i64>,
    pub(super) duration: Option<i64>,
    pub(super) fps: Option<f64>,
    pub(super) codec: Option<String>,
    pub(super) bitrate: Option<f64>,
    pub(super) native_redistribution: bool,
    pub(crate) revision: Option<String>,
    pub(super) availability: String,
}

#[derive(Serialize)]
pub struct StagePage {
    pub(super) files: Vec<StageFile>,
    pub(crate) total: i64,
}

// The key is a normalized full path. Trimming its non-slash characters leaves
// the parent directory including the final slash; search only the remaining name.
// instr treats SQL wildcard characters literally and matches the empty query too.
pub(super) const FILENAME_SEARCH: &str = concat!(
    " AND instr(substr(m.stage_search_key,",
    "length(rtrim(m.stage_search_key,replace(m.stage_search_key,'/','')))+1),?)=1",
);

pub(super) fn sort_order(sort: &str) -> Result<&'static str, String> {
    match sort {
        "newest" => Ok(" ORDER BY m.file_modified_at DESC,m.id ASC"),
        "older" => Ok(" ORDER BY m.file_modified_at ASC,m.id ASC"),
        "name-asc" => Ok(" ORDER BY m.relative_path COLLATE NEXA_FILENAME ASC,m.id ASC"),
        "name-desc" => Ok(" ORDER BY m.relative_path COLLATE NEXA_FILENAME DESC,m.id ASC"),
        _ => Err("Invalid Stage sorting mode".into()),
    }
}

pub(crate) async fn browse(
    pool: &SqlitePool,
    root: Option<String>,
    offset: i64,
    search: &str,
    sort: &str,
) -> Result<StagePage, String> {
    let order = sort_order(sort)?;

    if offset < 0 {
        return Err("Invalid Stage offset".into());
    }

    // Imported/redistribution records need searchable path keys, without filesystem reads.
    loop {
        let rows = sqlx::query(concat!(
            "SELECT m.id,s.root_path,m.relative_path FROM media m JOIN sources s ON ",
            "m.source_id=s.id WHERE m.stage_search_key IS NULL AND (s.kind IN ('stage','library') ",
            "OR m.needs_distribution=1) LIMIT 500",
        ))
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        if rows.is_empty() {
            break;
        }

        let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

        for row in rows {
            let path = Path::new(&row.get::<String, _>("root_path"))
                .join(row.get::<String, _>("relative_path"));

            let key = crate::naming::name_key(&path.to_string_lossy().replace('\\', "/"));
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

    let search = crate::naming::name_key(&search.trim().replace('\\', "/"));

    let mut count_query = QueryBuilder::<Sqlite>::new(concat!(
        "SELECT count(*) FROM media m JOIN sources s ON s.id=m.source_id WHERE ",
        "((s.kind='stage' AND s.path_key=? AND m.availability='present' AND ",
        "m.library_assignment IS NULL) OR m.needs_distribution=1)",
    ));
    count_query.push(FILENAME_SEARCH);

    let total = count_query
        .build_query_scalar()
        .bind(&root)
        .bind(&search)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut query = QueryBuilder::<Sqlite>::new(concat!(
        "SELECT m.*,s.root_path,v.duration_ms,v.fps_num,v.fps_den,v.codec,v.bitrate_bps FROM ",
        "media m JOIN sources s ON s.id=m.source_id LEFT JOIN video_metadata v ON ",
        "v.media_id=m.id WHERE ((s.kind='stage' AND s.path_key=? AND ",
        "m.availability='present' AND m.library_assignment IS NULL) OR m.needs_distribution=1) ",
    ));

    query
        .push(FILENAME_SEARCH)
        .push(order)
        .push(" LIMIT 500 OFFSET ?");

    let rows = query
        .build()
        .bind(root)
        .bind(search)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let files = rows.into_iter().map(stage_file).collect();
    Ok(StagePage { files, total })
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(offset, sort), err)]
pub async fn get_stage_files(
    offset: i64,
    search: String,
    sort: String,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<StagePage, String> {
    let root = config.snapshot()?.settings.staging_path.map(|path| {
        path.components()
            .collect::<PathBuf>()
            .to_string_lossy()
            .into_owned()
    });

    tracing::debug!(
        search_match = "filename_prefix",
        "Loading sorted Stage page"
    );

    let page = browse(database.pool(), root, offset, &search, &sort).await?;

    tracing::debug!(
        count = page.files.len(),
        total = page.total,
        "Stage page loaded"
    );
    Ok(page)
}

pub(crate) fn stage_file(row: sqlx::sqlite::SqliteRow) -> StageFile {
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
