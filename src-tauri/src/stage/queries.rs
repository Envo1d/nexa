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
    pub(super) relative_path: String,
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
#[serde(rename_all = "camelCase")]
pub struct StagePage {
    pub(super) files: Vec<StageFile>,
    pub(crate) total: i64,
    pub(super) queue_total: i64,
    pub(super) type_counts: StageTypeCounts,
}

#[derive(Serialize)]
pub(super) struct StageTypeCounts {
    all: i64,
    image: i64,
    video: i64,
}

// Match the normalized full path; instr treats SQL wildcard characters literally
// and matches the empty query too. Filename prefixes retain relevance priority.
pub(super) const PATH_SEARCH: &str = " AND instr(m.stage_search_key,?)>0";

pub(super) const FILENAME_RANK: &str = concat!(
    " ORDER BY CASE WHEN instr(substr(m.stage_search_key,",
    "length(rtrim(m.stage_search_key,replace(m.stage_search_key,'/','')))+1),?)=1 ",
    "THEN 0 ELSE 1 END,",
);

pub(super) fn sort_order(sort: &str) -> Result<&'static str, String> {
    match sort {
        "newest" => Ok("m.file_modified_at DESC,m.id ASC"),
        "older" => Ok("m.file_modified_at ASC,m.id ASC"),
        "name-asc" => Ok("m.relative_path COLLATE NEXA_FILENAME ASC,m.id ASC"),
        "name-desc" => Ok("m.relative_path COLLATE NEXA_FILENAME DESC,m.id ASC"),
        _ => Err("Invalid Stage sorting mode".into()),
    }
}

pub(super) fn kind_filter(kind: &str) -> Result<&'static str, String> {
    match kind {
        "all" => Ok(""),
        "image" => Ok(" AND m.kind='image'"),
        "video" => Ok(" AND m.kind='video'"),
        _ => Err("Invalid Stage media kind".into()),
    }
}

pub(crate) async fn browse(
    pool: &SqlitePool,
    root: Option<String>,
    offset: i64,
    search: &str,
    sort: &str,
    kind: &str,
) -> Result<StagePage, String> {
    let order = sort_order(sort)?;
    let kind_sql = kind_filter(kind)?;

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

    let counts = sqlx::query(concat!(
        "WITH eligible AS (SELECT m.kind,instr(m.stage_search_key,?)>0 AS matches ",
        "FROM media m JOIN sources s ON s.id=m.source_id WHERE ",
        "((s.kind='stage' AND s.path_key=? AND m.availability='present' AND ",
        "m.library_assignment IS NULL) OR m.needs_distribution=1)) ",
        "SELECT count(*) AS queue_total,coalesce(sum(matches),0) AS all_count,",
        "coalesce(sum(matches AND kind='image'),0) AS image_count,",
        "coalesce(sum(matches AND kind='video'),0) AS video_count FROM eligible",
    ))
    .bind(&search)
    .bind(&root)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    let queue_total = counts.get("queue_total");
    let type_counts = StageTypeCounts {
        all: counts.get("all_count"),
        image: counts.get("image_count"),
        video: counts.get("video_count"),
    };
    let total = match kind {
        "image" => type_counts.image,
        "video" => type_counts.video,
        _ => type_counts.all,
    };

    let mut query = QueryBuilder::<Sqlite>::new(concat!(
        "SELECT m.*,s.root_path,v.duration_ms,v.fps_num,v.fps_den,v.codec,v.bitrate_bps FROM ",
        "media m JOIN sources s ON s.id=m.source_id LEFT JOIN video_metadata v ON ",
        "v.media_id=m.id WHERE ((s.kind='stage' AND s.path_key=? AND ",
        "m.availability='present' AND m.library_assignment IS NULL) OR m.needs_distribution=1) ",
    ));

    query
        .push(PATH_SEARCH)
        .push(kind_sql)
        .push(FILENAME_RANK)
        .push(order)
        .push(" LIMIT 500 OFFSET ?");

    let rows = query
        .build()
        .bind(root)
        .bind(&search)
        .bind(&search)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let files = rows.into_iter().map(stage_file).collect();
    Ok(StagePage {
        files,
        total,
        queue_total,
        type_counts,
    })
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(offset, sort, kind), err)]
pub async fn get_stage_files(
    offset: i64,
    search: String,
    sort: String,
    kind: String,
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
        search_match = "filename_prefix_then_path_substring",
        "Loading sorted Stage page"
    );

    let page = browse(database.pool(), root, offset, &search, &sort, &kind).await?;

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
        relative_path: row.get("relative_path"),
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
