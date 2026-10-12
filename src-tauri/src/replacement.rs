//! Recoverable content adoption: keep Main identity and never overwrite media bytes.
mod queries;
mod workflow;
pub use queries::{
    __cmd__get_replacement_candidates, __cmd__get_replacement_source,
    __tauri_command_name_get_replacement_candidates, __tauri_command_name_get_replacement_source,
    get_replacement_candidates, get_replacement_source,
};
pub(crate) use workflow::recover;
pub use workflow::{
    __cmd__replace_library_file, __tauri_command_name_replace_library_file, replace_library_file,
};

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, Row, SqlitePool, sqlite::SqliteRow};

use crate::{config::AppConfig, stage};

const SELECT: &str = concat!(
    "SELECT m.*,s.root_path,s.kind source_kind,v.duration_ms,v.fps_num,v.fps_den,v.codec,",
    "v.bitrate_bps,v.title,a.name artist_name FROM media m JOIN sources s ON ",
    "s.id=m.source_id LEFT JOIN video_metadata v ON v.media_id=m.id LEFT JOIN artists a ON ",
    "a.id=m.artist_id",
);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    #[serde(flatten)]
    file: stage::StageFile,
    token: String,
    artist: Option<String>,
    video_title: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    file: Candidate,
    sync_with_disk: bool,
    has_annotations: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidates {
    files: Vec<Candidate>,
    next: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    main_id: String,
    stage_id: String,
    main_token: String,
    stage_token: String,
    sync_with_disk: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    old_file_deleted: bool,
}

fn id(value: &str) -> Result<i64, String> {
    value
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or("Invalid media ID".into())
}

fn token(row: &SqliteRow) -> String {
    serde_json::to_string(&(
        row.get::<i64, _>("id"),
        row.get::<i64, _>("source_id"),
        row.get::<String, _>("root_path"),
        row.get::<String, _>("relative_path"),
        row.get::<Option<String>, _>("file_revision"),
        row.get::<String, _>("kind"),
    ))
    .expect("Serializable identity")
}

fn candidate(row: SqliteRow) -> Candidate {
    Candidate {
        token: token(&row),
        artist: row.get("artist_name"),
        video_title: row.get("title"),
        file: stage::stage_file(row),
    }
}

async fn record(pool: &SqlitePool, value: i64) -> Result<SqliteRow, String> {
    // Only the constant SELECT fragment is interpolated; IDs remain bound values.
    sqlx::query(AssertSqlSafe(format!("{SELECT} WHERE m.id=?")))
        .bind(value)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Media is unavailable".into())
}

fn root(settings: &AppConfig) -> Result<String, String> {
    settings
        .library_path
        .as_ref()
        .map(|p| {
            p.components()
                .collect::<PathBuf>()
                .to_string_lossy()
                .into_owned()
        })
        .ok_or("Configure Main Library first".into())
}

fn is_main(row: &SqliteRow, settings: &AppConfig) -> bool {
    let Ok(library) = root(settings) else {
        return false;
    };

    let physical: String = row.get("root_path");

    let assigned = row
        .get::<Option<String>, _>("library_assignment")
        .as_deref()
        == Some(&library);

    let in_stage = settings.staging_path.as_ref().is_some_and(|p| {
        Path::new(&physical)
            .join(row.get::<String, _>("relative_path"))
            .starts_with(p)
    });
    row.get::<String, _>("availability") == "present"
        && row.get::<i64, _>("needs_distribution") == 0
        && (assigned
            || row.get::<String, _>("source_kind") == "library" && physical == library && !in_stage)
}

fn is_stage(row: &SqliteRow, settings: &AppConfig) -> bool {
    row.get::<String, _>("availability") == "present"
        && (row.get::<i64, _>("needs_distribution") != 0
            || row.get::<String, _>("source_kind") == "stage"
                && row.get::<Option<String>, _>("library_assignment").is_none()
                && settings
                    .staging_path
                    .as_ref()
                    .is_some_and(|p| p == Path::new(&row.get::<String, _>("root_path"))))
}

#[cfg(test)]
mod tests;
