use serde::Serialize;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use crate::stage;

const ISSUES: &str = concat!(
    "WITH issues(category,key,media_id,path,kind,reason,technical,collection) AS (",
    "SELECT 'missing','missing-'||m.id,cast(m.id AS TEXT),s.root_path||'/'||m.relative_path,",
    "m.kind,'missing',0,CASE WHEN m.needs_distribution=0 AND (m.library_assignment IS NOT NULL OR ",
    "s.kind='library') THEN 'library' ELSE 'stage' END ",
    "FROM media m JOIN sources s ON s.id=m.source_id WHERE m.availability='missing' UNION ALL ",
    "SELECT 'operations','move-'||o.media_id,cast(o.media_id AS TEXT),",
    "o.source_root||'/'||o.source_relative,'','move',0,'' FROM file_operations o UNION ALL ",
    "SELECT 'operations','replacement-'||r.id,cast(r.main_id AS TEXT),",
    "r.old_root||'/'||r.old_relative,'','replacement',0,'' FROM media_replacements r ",
    "WHERE r.finished=0 UNION ALL ",
    "SELECT 'operations','sync',NULL,'','',?,0,'' WHERE ? IS NOT NULL UNION ALL ",
    "SELECT 'previews','preview-'||m.id,cast(m.id AS TEXT),s.root_path||'/'||m.relative_path,",
    "m.kind,json_extract(p.value,'$.reason'),0,'' FROM json_each(?,'$.previews') p ",
    "JOIN media m ON m.id=json_extract(p.value,'$.id') JOIN sources s ON s.id=m.source_id ",
    "WHERE m.availability='present' AND m.file_revision IS json_extract(p.value,'$.revision') ",
    "AND s.root_path||'/'||m.relative_path=json_extract(p.value,'$.path') UNION ALL ",
    "SELECT 'operations','source-'||p.value,NULL,p.value,'','source',0,'' ",
    "FROM json_each(?,'$.roots') p UNION ALL ",
    "SELECT 'metadata','metadata-'||m.id,cast(m.id AS TEXT),s.root_path||'/'||m.relative_path,",
    "m.kind,CASE WHEN m.metadata_state<>'ready' OR m.width IS NULL OR m.height IS NULL ",
    "THEN 'technical' ELSE 'annotations' END,",
    "m.metadata_state<>'ready' OR m.width IS NULL OR m.height IS NULL,",
    "CASE WHEN m.needs_distribution=0 AND (m.library_assignment IS NOT NULL OR ",
    "s.kind='library') THEN 'library' ELSE 'stage' END ",
    "FROM media m JOIN sources s ON s.id=m.source_id LEFT JOIN video_metadata v ON v.media_id=m.id ",
    "WHERE m.availability='present' AND (m.metadata_state<>'ready' OR m.width IS NULL ",
    "OR m.height IS NULL OR (((s.kind='library' AND s.is_active=1) OR m.library_assignment ",
    "IS NOT NULL) AND m.needs_distribution=0 AND (m.artist_id IS NULL OR NOT EXISTS ",
    "(SELECT 1 FROM media_copyrights c WHERE c.media_id=m.id) OR (m.kind='video' AND ",
    "(v.title IS NULL OR trim(v.title)='')))))) ",
);

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Issue {
    key: String,
    media_id: Option<String>,
    path: String,
    kind: String,
    reason: String,
    technical: bool,
    collection: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthPage {
    counts: [i64; 4],
    total: i64,
    issues: Vec<Issue>,
}

pub(super) async fn browse(
    pool: &SqlitePool,
    category: &str,
    offset: i64,
    previews: &str,
    sync_error: Option<String>,
) -> Result<HealthPage, String> {
    let index = ["missing", "operations", "previews", "metadata"]
        .iter()
        .position(|value| *value == category)
        .ok_or("Invalid health category")?;

    if offset < 0 {
        return Err("Invalid health offset".into());
    }

    // One read transaction keeps counts and this bounded page on the same catalog snapshot.
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let mut count_query = query(previews, &sync_error);
    count_query.push("SELECT category,count(*) count FROM issues GROUP BY category");
    let rows = count_query
        .build()
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    let mut counts = [0; 4];

    for row in rows {
        let category: String = row.get("category");
        if let Some(index) = ["missing", "operations", "previews", "metadata"]
            .iter()
            .position(|value| *value == category)
        {
            counts[index] = row.get("count");
        }
    }

    let mut page_query = query(previews, &sync_error);
    page_query
        .push(
            "SELECT key,media_id,path,kind,reason,technical,collection FROM issues WHERE category=",
        )
        .push_bind(category)
        .push(" ORDER BY key LIMIT 100 OFFSET ")
        .push_bind(offset);
    let issues = page_query
        .build_query_as()
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;

    Ok(HealthPage {
        counts,
        total: counts[index],
        issues,
    })
}

fn query(previews: &str, sync_error: &Option<String>) -> QueryBuilder<Sqlite> {
    // Bind the runtime diagnostic parameters where the CTE declares them.
    let parts: Vec<_> = ISSUES.split('?').collect();
    let mut query = QueryBuilder::new(parts[0]);
    query
        .push_bind(sync_error)
        .push(parts[1])
        .push_bind(sync_error)
        .push(parts[2])
        .push_bind(previews)
        .push(parts[3])
        .push_bind(previews)
        .push(parts[4]);
    query
}

#[derive(Serialize, sqlx::FromRow)]
pub(crate) struct Character {
    name: String,
    copyright: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthMedia {
    #[serde(flatten)]
    file: stage::StageFile,
    favorite: bool,
    artist: Option<String>,
    copyrights: Vec<String>,
    character_copyrights: Vec<Character>,
    tags: Vec<String>,
    video_title: Option<String>,
}

pub(super) async fn media(pool: &SqlitePool, id: i64) -> Result<HealthMedia, String> {
    let row = sqlx::query(concat!(
        "SELECT m.*,s.root_path,v.duration_ms,v.fps_num,v.fps_den,v.codec,v.bitrate_bps,",
        "v.title,a.name artist_name FROM media m JOIN sources s ON s.id=m.source_id ",
        "LEFT JOIN video_metadata v ON v.media_id=m.id LEFT JOIN artists a ON a.id=m.artist_id WHERE m.id=?",
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or("Media unavailable")?;
    let favorite = row.get::<i64, _>("favorite") != 0;
    let artist = row.get("artist_name");
    let video_title = row.get("title");
    let copyrights = sqlx::query_scalar(concat!(
        "SELECT c.name FROM copyrights c JOIN media_copyrights l ON l.copyright_id=c.id ",
        "WHERE l.media_id=? ORDER BY c.name_key,c.id",
    ))
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let tags = sqlx::query_scalar(concat!(
        "SELECT t.name FROM tags t JOIN media_tags l ON l.tag_id=t.id ",
        "WHERE l.media_id=? ORDER BY t.name_key,t.id",
    ))
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let character_copyrights = sqlx::query_as(concat!(
        "SELECT c.name,p.name copyright FROM characters c JOIN copyrights p ON p.id=c.copyright_id ",
        "JOIN media_characters l ON l.character_id=c.id WHERE l.media_id=? ORDER BY c.name_key,c.id",
    ))
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    Ok(HealthMedia {
        file: stage::stage_file(row),
        favorite,
        artist,
        copyrights,
        character_copyrights,
        tags,
        video_title,
    })
}
