//! Replacement source and target queries; no filesystem mutation.

use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use super::{Candidates, SELECT_BODY, Source, candidate, id, is_main, is_stage, record, root};
use crate::{
    config::{AppConfig, ConfigStore},
    database::Database,
};

#[tauri::command]
pub async fn get_replacement_source(
    id: String,
    preferred_main_id: Option<String>,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<Source, String> {
    let id = self::id(&id)?;

    let settings = config.snapshot()?.settings;
    root(&settings)?;

    let row = record(database.pool(), id).await?;

    if !is_stage(&row, &settings) {
        return Err("Select a present Stage file".into());
    }

    let has_annotations: bool = sqlx::query_scalar(concat!(
        "SELECT artist_id IS NOT NULL OR favorite=1 OR EXISTS(SELECT 1 FROM media_tags WHERE ",
        "media_id=m.id) OR EXISTS(SELECT 1 FROM media_copyrights WHERE media_id=m.id) OR ",
        "EXISTS(SELECT 1 FROM media_characters WHERE media_id=m.id) OR EXISTS(SELECT 1 FROM ",
        "video_metadata WHERE media_id=m.id AND title IS NOT NULL) FROM media m WHERE id=?",
    ))
    .bind(id)
    .fetch_one(database.pool())
    .await
    .map_err(|e| e.to_string())?;

    let preferred = if let Some(preferred_id) = preferred_main_id {
        let target = record(database.pool(), self::id(&preferred_id)?).await?;

        if !is_main(&target, &settings)
            || target.get::<String, _>("kind") != row.get::<String, _>("kind")
        {
            return Err("Replacement target changed; restart duplicate search".into());
        }

        Some(candidate(target))
    } else {
        None
    };

    Ok(Source {
        preferred,
        file: candidate(row),
        sync_with_disk: settings.sync_structure,
        has_annotations,
    })
}

// The independent picker searches the full Main database, not the current view/page.
pub(super) async fn candidates(
    pool: &SqlitePool,
    settings: &AppConfig,
    kind: &str,
    search: &str,
    after: i64,
) -> Result<Candidates, String> {
    if !matches!(kind, "image" | "video") || after == i64::MIN || search.len() > 512 {
        return Err("Invalid replacement search".into());
    }

    let library = root(settings)?;

    let stage_prefix = settings.staging_path.as_ref().map(|p| {
        format!(
            "{}/",
            p.to_string_lossy().replace('\\', "/").trim_end_matches('/')
        )
    });

    // Cursor sign records the relevance group; magnitude remains the media ID.
    let cursor_rank = i64::from(after < 0);
    let cursor_id = after.abs();

    let mut query = QueryBuilder::<Sqlite>::new("WITH matches AS (SELECT ");
    crate::library::metadata_rank(&mut query, search);
    query.push(" AS search_rank,").push(SELECT_BODY);
    query
        .push(" WHERE m.id>0 AND m.kind=")
        .push_bind(kind)
        .push(" AND m.availability='present' AND m.needs_distribution=0 AND (m.library_assignment=")
        .push_bind(&library)
        .push(" OR (s.kind='library' AND s.path_key=")
        .push_bind(&library)
        .push("))");

    if let Some(prefix) = &stage_prefix {
        query
            .push(" AND (m.library_assignment=")
            .push_bind(&library)
            .push(" OR substr(replace(s.root_path || '/' || m.relative_path,char(92),'/'),1,")
            .push_bind(prefix.chars().count() as i64)
            .push(") COLLATE NOCASE != ")
            .push_bind(prefix)
            .push(")");
    }
    crate::library::apply_metadata_search(&mut query, search);
    query
        .push(") SELECT * FROM matches WHERE search_rank>")
        .push_bind(cursor_rank)
        .push(" OR (search_rank=")
        .push_bind(cursor_rank)
        .push(" AND id>")
        .push_bind(cursor_id)
        .push(") ORDER BY search_rank,id LIMIT 51");

    tracing::debug!(
        kind,
        after,
        search_match = "metadata_prefix_then_substring",
        "Loading replacement candidates using Main metadata filters"
    );

    let mut rows = query
        .build()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let more = rows.len() > 50;
    rows.truncate(50);

    let next = if more {
        rows.last().map(|row| {
            let id = row.get::<i64, _>("id");
            let cursor = if row.get::<i64, _>("search_rank") == 0 {
                id
            } else {
                -id
            };

            cursor.to_string()
        })
    } else {
        None
    };
    Ok(Candidates {
        files: rows.into_iter().map(candidate).collect(),
        next,
    })
}

#[tauri::command]
pub async fn get_replacement_candidates(
    kind: String,
    search: String,
    after: Option<String>,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<Candidates, String> {
    candidates(
        database.pool(),
        &config.snapshot()?.settings,
        &kind,
        &search,
        after.as_deref().map(parse_cursor).transpose()?.unwrap_or(0),
    )
    .await
}

pub(super) fn parse_cursor(value: &str) -> Result<i64, String> {
    value
        .parse::<i64>()
        .ok()
        .filter(|id| *id != 0 && *id != i64::MIN)
        .ok_or_else(|| "Invalid replacement cursor".into())
}
