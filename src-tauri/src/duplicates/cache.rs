use std::path::Path;

use sqlx::SqlitePool;

use super::fingerprints;
use crate::media_paths::checked_action_path;

pub(super) const VERSION: i64 = 1;

#[derive(Clone, sqlx::FromRow)]
pub(super) struct Fingerprint {
    pub media_id: i64,
    pub revision: String,
    pub kind: String,
    pub algorithm_version: i64,
    pub sha256: String,
    pub visual: Option<String>,
    pub visual_retry: bool,
}

pub(super) async fn get(pool: &SqlitePool, id: i64) -> Result<Option<Fingerprint>, String> {
    sqlx::query_as(concat!(
        "SELECT media_id,revision,kind,algorithm_version,sha256,visual,visual_retry ",
        "FROM duplicate_fingerprints WHERE media_id=?",
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())
}

pub(super) fn resolve(
    id: i64,
    root: &Path,
    relative: &str,
    expected: Option<&str>,
    kind: &str,
    cached: Option<Fingerprint>,
) -> Result<(Fingerprint, bool), String> {
    let current = fingerprints::current_revision(root, relative, expected)?;

    if let Some(mut cached) = cached {
        let reusable = cached.algorithm_version == VERSION
            && cached.kind == kind
            && cached.revision == current;

        if reusable {
            if cached.visual_retry && kind == "image" {
                let path = checked_action_path(root, relative)?;

                (cached.visual, cached.visual_retry) = fingerprints::visual(&path);
            }

            if fingerprints::current_revision(root, relative, expected)? != current {
                return Err("File changed during cached fingerprint validation".into());
            }

            return Ok((cached, true));
        }
    }

    let (sha256, visual, revision, visual_retry) =
        fingerprints::read(root, relative, expected, kind == "image")?;

    Ok((
        Fingerprint {
            media_id: id,
            revision,
            kind: kind.into(),
            algorithm_version: VERSION,
            sha256,
            visual,
            visual_retry,
        },
        false,
    ))
}

pub(super) async fn persist(
    pool: &SqlitePool,
    fingerprints: &[Fingerprint],
    invalid: &[i64],
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;

    for id in invalid {
        sqlx::query("DELETE FROM duplicate_fingerprints WHERE media_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }

    for fingerprint in fingerprints {
        sqlx::query(concat!(
            "INSERT INTO duplicate_fingerprints(media_id,revision,kind,algorithm_version,sha256,visual,visual_retry,updated_at) ",
            "VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(media_id) DO UPDATE SET revision=excluded.revision,",
            "kind=excluded.kind,algorithm_version=excluded.algorithm_version,sha256=excluded.sha256,",
            "visual=excluded.visual,visual_retry=excluded.visual_retry,updated_at=excluded.updated_at",
        ))
        .bind(fingerprint.media_id)
        .bind(&fingerprint.revision)
        .bind(&fingerprint.kind)
        .bind(fingerprint.algorithm_version)
        .bind(&fingerprint.sha256)
        .bind(&fingerprint.visual)
        .bind(fingerprint.visual_retry)
        .bind(crate::stage::now())
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }

    tx.commit().await.map_err(|error| error.to_string())
}
