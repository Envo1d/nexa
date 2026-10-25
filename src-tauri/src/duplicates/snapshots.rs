use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, Row, SqlitePool};

use super::{File, cache, fingerprints, queries};
use crate::{
    config::{AppConfig, ConfigStore},
    database::Database,
    replacement,
    stage::StageState,
};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub(super) total: usize,
    pub(super) processed: usize,
    pub(super) checked: usize,
    pub(super) skipped: usize,
    pub(super) visual_unavailable: usize,
    pub(super) reused: usize,
    pub(super) hashed: usize,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pair {
    pub(super) left_id: String,
    pub(super) right_id: String,
    pub(super) left_hash: String,
    pub(super) right_hash: String,
    pub(super) exact: bool,
    pub(super) distance: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub(super) scope: String,
    pub(super) generation: i64,
    pub(super) status: String,
    pub(super) report: Report,
    pub(super) pairs: Vec<Pair>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSearch {
    updated_at: i64,
    status: String,
    report: Report,
}

#[derive(Serialize)]
pub struct Loaded {
    snapshot: Option<SavedSearch>,
    files: Vec<File>,
    pairs: Vec<Pair>,
    rejections: Vec<(String, String)>,
    stale: usize,
}

pub(super) fn scope(settings: &AppConfig) -> Result<String, String> {
    serde_json::to_string(&(&settings.library_path, &settings.staging_path))
        .map_err(|error| error.to_string())
}

pub(super) async fn begin(pool: &SqlitePool, scope: &str, total: usize) -> Result<i64, String> {
    let report = serde_json::to_string(&Report {
        total,
        ..Report::default()
    })
    .map_err(|error| error.to_string())?;

    sqlx::query_scalar(concat!(
        "INSERT INTO duplicate_searches(scope,generation,algorithm_version,updated_at,status,report_json,pairs_json) ",
        "VALUES(?,1,?,?,'partial',?,'[]') ON CONFLICT(scope) DO UPDATE SET generation=generation+1,",
        "algorithm_version=excluded.algorithm_version,updated_at=excluded.updated_at,status='partial',",
        "report_json=excluded.report_json RETURNING generation",
    ))
    .bind(scope)
    .bind(cache::VERSION)
    .bind(crate::stage::now())
    .bind(report)
    .fetch_one(pool)
    .await
    .map_err(|error| error.to_string())
}

pub(super) async fn save(pool: &SqlitePool, checkpoint: Checkpoint) -> Result<(), String> {
    let valid_status = matches!(
        checkpoint.status.as_str(),
        "partial" | "complete" | "stopped" | "limited" | "failed"
    );

    if !valid_status || checkpoint.generation <= 0 || checkpoint.pairs.len() > 1000 {
        return Err("Invalid duplicate search checkpoint".into());
    }

    for pair in &checkpoint.pairs {
        let valid_ids = [&pair.left_id, &pair.right_id]
            .iter()
            .all(|id| id.parse::<i64>().is_ok_and(|id| id > 0));

        let valid_hashes = [&pair.left_hash, &pair.right_hash].iter().all(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        });

        if !valid_ids
            || !valid_hashes
            || pair.left_id == pair.right_id
            || pair.distance > 8
            || pair.exact && pair.distance != 0
            || pair.exact != (pair.left_hash == pair.right_hash)
        {
            return Err("Invalid saved duplicate pair".into());
        }
    }

    let result = sqlx::query(concat!(
        "UPDATE duplicate_searches SET updated_at=?,status=?,report_json=?,pairs_json=? ",
        "WHERE scope=? AND generation=? AND algorithm_version=?",
    ))
    .bind(crate::stage::now())
    .bind(checkpoint.status)
    .bind(serde_json::to_string(&checkpoint.report).map_err(|error| error.to_string())?)
    .bind(serde_json::to_string(&checkpoint.pairs).map_err(|error| error.to_string())?)
    .bind(checkpoint.scope)
    .bind(checkpoint.generation)
    .bind(cache::VERSION)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;

    if result.rows_affected() == 0 {
        return Err("A newer duplicate search replaced this session".into());
    }

    Ok(())
}

pub(super) async fn load(database: &Database, settings: &AppConfig) -> Result<Loaded, String> {
    let rejections: Vec<(String, String)> =
        sqlx::query_as("SELECT left_hash,right_hash FROM duplicate_rejections")
            .fetch_all(database.pool())
            .await
            .map_err(|error| error.to_string())?;

    let row = sqlx::query(concat!(
        "SELECT updated_at,status,report_json,pairs_json FROM duplicate_searches ",
        "WHERE scope=? AND algorithm_version=?",
    ))
    .bind(scope(settings)?)
    .bind(cache::VERSION)
    .fetch_optional(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    let Some(row) = row else {
        return Ok(Loaded {
            snapshot: None,
            files: Vec::new(),
            pairs: Vec::new(),
            rejections,
            stale: 0,
        });
    };

    let snapshot = SavedSearch {
        updated_at: row.get("updated_at"),
        status: row.get("status"),
        report: serde_json::from_str(&row.get::<String, _>("report_json"))
            .map_err(|error| error.to_string())?,
    };

    let pairs: Vec<Pair> = serde_json::from_str(&row.get::<String, _>("pairs_json"))
        .map_err(|error| error.to_string())?;

    if pairs.len() > 1000 {
        return Err("Saved duplicate search exceeds its result limit".into());
    }

    let ids: HashSet<_> = pairs
        .iter()
        .flat_map(|pair| [&pair.left_id, &pair.right_id])
        .cloned()
        .collect();

    let mut work = Vec::new();

    for id in ids {
        let numeric_id = id.parse::<i64>().map_err(|_| "Invalid saved media ID")?;

        let row = sqlx::query(AssertSqlSafe(format!(
            "{} WHERE m.id=? AND m.availability='present'",
            queries::SELECT
        )))
        .bind(numeric_id)
        .fetch_optional(database.pool())
        .await
        .map_err(|error| error.to_string())?;

        let Some(row) = row else {
            continue;
        };

        let main = replacement::is_main(&row, settings);

        if !main && !replacement::is_stage(&row, settings) {
            continue;
        }

        let Some(cached) = cache::get(database.pool(), numeric_id).await? else {
            continue;
        };

        if cached.algorithm_version != cache::VERSION || cached.kind != row.get::<String, _>("kind")
        {
            continue;
        }

        let root = PathBuf::from(row.get::<String, _>("root_path"));

        let relative: String = row.get("relative_path");

        let revision: Option<String> = row.get("file_revision");

        work.push((
            id,
            queries::file(row, main)?,
            root,
            relative,
            revision,
            cached,
        ));
    }

    let files = tauri::async_runtime::spawn_blocking(move || {
        let mut files = HashMap::new();

        for (id, mut file, root, relative, revision, cached) in work {
            let valid = fingerprints::current_revision(&root, &relative, revision.as_deref())
                .is_ok_and(|revision| revision == cached.revision);

            if valid {
                file.sha256 = cached.sha256;
                file.visual = cached.visual;
                file.technical.revision = Some(cached.revision);
                files.insert(id, file);
            }
        }

        files
    })
    .await
    .map_err(|error| error.to_string())?;

    let original_count = pairs.len();

    let pairs = pairs
        .into_iter()
        .filter(|pair| {
            let (Some(left), Some(right)) = (files.get(&pair.left_id), files.get(&pair.right_id))
            else {
                return false;
            };

            let hashes_match = pair.left_hash == left.sha256 && pair.right_hash == right.sha256;

            let rejected = rejections.iter().any(|(a, b)| {
                (a == &left.sha256 && b == &right.sha256)
                    || (b == &left.sha256 && a == &right.sha256)
            });

            hashes_match
                && valid_comparison(pair, left, right)
                && pair.exact == (left.sha256 == right.sha256)
                && (pair.exact || !rejected)
        })
        .collect::<Vec<_>>();

    Ok(Loaded {
        stale: original_count - pairs.len(),
        snapshot: Some(snapshot),
        files: files.into_values().collect(),
        pairs,
        rejections,
    })
}

fn valid_comparison(pair: &Pair, left: &File, right: &File) -> bool {
    if pair.exact {
        return pair.distance == 0;
    }

    if left.technical.kind != "image" || right.technical.kind != "image" {
        return false;
    }

    let dimensions = left
        .technical
        .width
        .zip(left.technical.height)
        .zip(right.technical.width.zip(right.technical.height));

    let Some(((left_width, left_height), (right_width, right_height))) = dimensions else {
        return false;
    };

    let ratio = left_width as f64 / left_height as f64 / (right_width as f64 / right_height as f64);

    let hashes = left
        .visual
        .as_deref()
        .zip(right.visual.as_deref())
        .and_then(|(left, right)| {
            Some((
                u64::from_str_radix(left, 16).ok()?,
                u64::from_str_radix(right, 16).ok()?,
            ))
        });

    let Some((left, right)) = hashes else {
        return false;
    };

    let distance = (left ^ right).count_ones();

    (0.85..=1.0 / 0.85).contains(&ratio) && distance <= 8 && distance == pair.distance
}

#[tauri::command]
#[tracing::instrument(skip_all)]
pub async fn load_duplicate_search(
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
    state: tauri::State<'_, StageState>,
) -> Result<Loaded, String> {
    let _guard = state
        .try_operation()
        .ok_or("Another media operation is running; retry when it finishes")?;

    tracing::info!("Saved duplicate search load started");

    let result = load(&database, &config.snapshot()?.settings).await;

    match &result {
        Ok(saved) => tracing::info!(
            pairs = saved.pairs.len(),
            stale = saved.stale,
            "Saved duplicate search loaded"
        ),
        Err(error) => tracing::error!(reason = %error, "Saved duplicate search load failed"),
    }

    result
}

#[tauri::command]
#[tracing::instrument(skip_all)]
pub async fn save_duplicate_search(
    checkpoint: Checkpoint,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<(), String> {
    if checkpoint.scope != scope(&config.snapshot()?.settings)? {
        return Err("Configured folders changed; restart duplicate search".into());
    }

    let result = save(database.pool(), checkpoint).await;

    match &result {
        Ok(()) => tracing::debug!("Duplicate search checkpoint saved"),
        Err(error) => tracing::error!(reason = %error, "Duplicate search checkpoint failed"),
    }

    result
}
