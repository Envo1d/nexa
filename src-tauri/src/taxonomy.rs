//! Taxonomy changes are database-only; this module never writes media files.
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Sqlite, SqlitePool};
use unicode_normalization::UnicodeNormalization;

use crate::{database::Database, naming::name_key};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Copyright,
    Artist,
    Tag,
    Character,
}

impl Category {
    fn table(self) -> &'static str {
        match self {
            Self::Copyright => "copyrights",
            Self::Artist => "artists",
            Self::Tag => "tags",
            Self::Character => "characters",
        }
    }

    fn relation(self) -> &'static str {
        match self {
            Self::Copyright => {
                "m.id IN (SELECT media_id FROM media_copyrights WHERE copyright_id = d.id)"
            }
            Self::Artist => "m.artist_id = d.id",
            Self::Character => {
                "m.id IN (SELECT media_id FROM media_characters WHERE character_id = d.id)"
            }
            Self::Tag => "m.id IN (SELECT media_id FROM media_tags WHERE tag_id = d.id)",
        }
    }
}

#[derive(Serialize, FromRow)]
pub struct Entry {
    id: String,
    name: String,
    count: i64,
    copyright: Option<String>,
}

#[derive(Serialize, Default)]
pub struct Catalog {
    copyrights: Vec<Entry>,
    artists: Vec<Entry>,
    tags: Vec<Entry>,
    characters: Vec<Entry>,
}

async fn entries(pool: &SqlitePool, category: Category) -> Result<Vec<Entry>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new("SELECT CAST(d.id AS TEXT) id, d.name, ");
    query.push(if matches!(category, Category::Character) {
        "(SELECT name FROM copyrights WHERE id = d.copyright_id)"
    } else {
        "NULL"
    });
    query
        .push(" copyright, (SELECT count(*) FROM media m WHERE ")
        .push(category.relation())
        .push(") count FROM ")
        .push(category.table())
        .push(" d ORDER BY d.name_key, d.id")
        .build_query_as()
        .fetch_all(pool)
        .await
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub async fn get_taxonomy(database: tauri::State<'_, Database>) -> Result<Catalog, String> {
    let pool = database.pool();
    Ok(Catalog {
        copyrights: entries(pool, Category::Copyright)
            .await
            .map_err(|e| e.to_string())?,
        artists: entries(pool, Category::Artist)
            .await
            .map_err(|e| e.to_string())?,
        characters: entries(pool, Category::Character)
            .await
            .map_err(|e| e.to_string())?,
        tags: entries(pool, Category::Tag)
            .await
            .map_err(|e| e.to_string())?,
    })
}

async fn rename(pool: &SqlitePool, category: Category, id: i64, name: &str) -> Result<(), String> {
    let name: String = name.trim().nfc().collect();

    if name.is_empty() || name.chars().any(char::is_control) {
        return Err("Enter a nonempty name without control characters.".into());
    }

    let result = QueryBuilder::<Sqlite>::new("UPDATE ")
        .push(category.table())
        .push(" SET name = ")
        .push_bind(&name)
        .push(", name_key = ")
        .push_bind(name_key(&name))
        .push(" WHERE id = ")
        .push_bind(id)
        .build()
        .execute(pool)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .is_some_and(|e| e.is_unique_violation())
            {
                "This name already exists in the category.".to_string()
            } else {
                error.to_string()
            }
        })?;

    if result.rows_affected() == 0 {
        return Err("This entry no longer exists. Refresh the list.".into());
    }
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(?category, id), err)]
pub async fn rename_taxonomy(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::stage::StageState>,
    database: tauri::State<'_, Database>,
    category: Category,
    id: i64,
    name: String,
) -> Result<(), String> {
    let _guard = state.operation().await;
    crate::disk_sync::recover(database.pool()).await?;

    tracing::info!("Renaming dictionary entry");
    rename(database.pool(), category, id, &name).await?;
    crate::disk_sync::request(&app);

    tracing::info!("Dictionary entry renamed; disk synchronization requested");
    Ok(())
}

async fn remove(pool: &SqlitePool, category: Category, id: i64) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

    let queued = match category {
        Category::Copyright => {
            let count = sqlx::query(concat!(
                "UPDATE media SET needs_distribution = 1, updated_at = CAST(unixepoch('subsec') * 1000 ",
                "AS INTEGER) WHERE id IN (SELECT media_id FROM media_copyrights WHERE copyright_id = ",
                "?)",
            ))
                .bind(id)
                .execute(&mut *tx)
                .await?.rows_affected();
            // Deleting the copyright links cascades only the corresponding character links.
            sqlx::query("DELETE FROM media_copyrights WHERE copyright_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM characters WHERE copyright_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            count
        }
        Category::Artist => sqlx::query(concat!(
            "UPDATE media SET needs_distribution = 1, artist_id = NULL, updated_at = ",
            "CAST(unixepoch('subsec') * 1000 AS INTEGER) WHERE artist_id = ?",
        ))
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected(),
        Category::Character => {
            sqlx::query("DELETE FROM media_characters WHERE character_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            0
        }
        Category::Tag => {
            sqlx::query("DELETE FROM media_tags WHERE tag_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            0
        }
    };
    QueryBuilder::<Sqlite>::new("DELETE FROM ")
        .push(category.table())
        .push(" WHERE id = ")
        .push_bind(id)
        .build()
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(queued)
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(?category, id), err)]
pub async fn delete_taxonomy(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::stage::StageState>,
    database: tauri::State<'_, Database>,
    category: Category,
    id: i64,
) -> Result<u64, String> {
    let _guard = state.operation().await;
    crate::disk_sync::recover(database.pool()).await?;

    tracing::info!("Deleting dictionary entry");

    let count = remove(database.pool(), category, id)
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!(
        affected_files = count,
        "Dictionary entry deleted; redistribution queued without disk changes"
    );
    crate::disk_sync::request(&app);
    Ok(count)
}

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RedistributionFile {
    id: String,
    root: String,
    relative_path: String,
    kind: String,
    extension: String,
    size: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    modified_at: Option<i64>,
    created_at: Option<i64>,
    duration: Option<i64>,
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub async fn get_redistribution_files(
    database: tauri::State<'_, Database>,
) -> Result<Vec<RedistributionFile>, String> {
    sqlx::query_as(concat!(
        "SELECT CAST(m.id AS TEXT) id, s.root_path root, m.relative_path, m.kind, m.extension, ",
        "m.size_bytes size, m.width, m.height, m.file_modified_at modified_at, ",
        "m.file_created_at created_at, v.duration_ms duration FROM media m JOIN sources s ON ",
        "s.id = m.source_id LEFT JOIN video_metadata v ON v.media_id = m.id WHERE ",
        "m.needs_distribution = 1 ORDER BY m.id",
    ))
    .fetch_all(database.pool())
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
