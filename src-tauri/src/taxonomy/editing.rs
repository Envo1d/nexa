use sqlx::SqlitePool;
use unicode_normalization::UnicodeNormalization;

use crate::naming::name_key;

pub(super) async fn update_character(
    pool: &SqlitePool,
    id: i64,
    name: &str,
    copyright_id: i64,
) -> Result<u64, String> {
    let name: String = name.trim().nfc().collect();

    if name.is_empty() || name.chars().any(char::is_control) {
        return Err("Enter a nonempty name without control characters.".into());
    }

    update(pool, id, &name, copyright_id)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .is_some_and(|e| e.is_unique_violation())
            {
                "This character already exists in the copyright.".to_string()
            } else if matches!(error, sqlx::Error::RowNotFound) {
                "The character or copyright no longer exists. Refresh the list.".to_string()
            } else {
                error.to_string()
            }
        })
}

async fn update(
    pool: &SqlitePool,
    id: i64,
    name: &str,
    copyright_id: i64,
) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

    // Validate both identities before touching any associations.
    let old_copyright: i64 = sqlx::query_scalar("SELECT copyright_id FROM characters WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;

    sqlx::query_scalar::<_, i64>("SELECT id FROM copyrights WHERE id = ?")
        .bind(copyright_id)
        .fetch_one(&mut *tx)
        .await?;

    let media_ids: Vec<i64> = if old_copyright != copyright_id {
        sqlx::query_scalar("DELETE FROM media_characters WHERE character_id = ? RETURNING media_id")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?
    } else {
        Vec::new()
    };

    sqlx::query("UPDATE characters SET name = ?, name_key = ?, copyright_id = ? WHERE id = ?")
        .bind(name)
        .bind(name_key(name))
        .bind(copyright_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;

    // Restore qualified links only after the parent identity has changed.
    let mut removed_copyrights = 0;

    for media_id in &media_ids {
        sqlx::query("INSERT OR IGNORE INTO media_copyrights(media_id, copyright_id) VALUES (?, ?)")
            .bind(media_id)
            .bind(copyright_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(concat!(
            "INSERT INTO media_characters(media_id, character_id, copyright_id) ",
            "VALUES (?, ?, ?)",
        ))
        .bind(media_id)
        .bind(id)
        .bind(copyright_id)
        .execute(&mut *tx)
        .await?;

        // Cleanup is local to this file and the character's previous copyright.
        removed_copyrights += sqlx::query(concat!(
            "DELETE FROM media_copyrights WHERE media_id = ? AND copyright_id = ? ",
            "AND NOT EXISTS (SELECT 1 FROM media_characters ",
            "WHERE media_id = ? AND copyright_id = ?)",
        ))
        .bind(media_id)
        .bind(old_copyright)
        .bind(media_id)
        .bind(old_copyright)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }

    let affected = sqlx::query(concat!(
        "UPDATE media SET updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER) ",
        "WHERE id IN (SELECT media_id FROM media_characters WHERE character_id = ?)",
    ))
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;
    tracing::debug!(
        removed_copyright_links = removed_copyrights,
        "Removed previous copyright links without remaining characters"
    );

    Ok(affected)
}
