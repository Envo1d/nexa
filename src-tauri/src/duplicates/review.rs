use sqlx::SqlitePool;

use crate::database::Database;

pub(super) fn canonical_pair(left: String, right: String) -> Result<(String, String), String> {
    let valid = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };

    if !valid(&left) || !valid(&right) || left == right {
        return Err("Choose two different valid content hashes".into());
    }

    Ok(if left < right {
        (left, right)
    } else {
        (right, left)
    })
}

pub(super) async fn set(
    pool: &SqlitePool,
    left: String,
    right: String,
    rejected: bool,
) -> Result<(), String> {
    let (left, right) = canonical_pair(left, right)?;

    if rejected {
        sqlx::query(concat!(
            "INSERT INTO duplicate_rejections(left_hash,right_hash,created_at) VALUES(?,?,?) ",
            "ON CONFLICT(left_hash,right_hash) DO NOTHING",
        ))
        .bind(left)
        .bind(right)
        .bind(crate::stage::now())
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    } else {
        sqlx::query("DELETE FROM duplicate_rejections WHERE left_hash=? AND right_hash=?")
            .bind(left)
            .bind(right)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(rejected))]
pub async fn set_duplicate_rejection(
    left_hash: String,
    right_hash: String,
    rejected: bool,
    database: tauri::State<'_, Database>,
) -> Result<(), String> {
    tracing::info!("Duplicate similarity review save started");

    let result = set(database.pool(), left_hash, right_hash, rejected).await;

    match &result {
        Ok(()) => tracing::info!("Duplicate similarity review saved"),
        Err(error) => tracing::error!(reason = %error, "Duplicate similarity review save failed"),
    }

    result
}
