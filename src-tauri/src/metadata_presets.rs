//! Reusable annotation snapshots; saving a preset never changes media or taxonomy.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, SqlitePool};
use unicode_normalization::UnicodeNormalization;

use crate::{database::Database, naming::name_key};

#[derive(Clone, Deserialize, Serialize)]
pub struct Character {
    name: String,
    copyright: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Metadata {
    artist: String,
    copyrights: Vec<String>,
    characters: Vec<Character>,
    tags: Vec<String>,
}

#[derive(Serialize)]
pub struct Preset {
    id: String,
    name: String,
    metadata: Metadata,
}

#[derive(FromRow)]
struct StoredPreset {
    id: i64,
    name: String,
    metadata: String,
}

fn clean(value: &str) -> Result<String, String> {
    let value: String = value.trim().nfc().collect();

    if value.is_empty() || value.chars().count() > 256 || value.chars().any(char::is_control) {
        return Err(
            "Use a nonempty name of at most 256 characters without control characters.".into(),
        );
    }

    Ok(value)
}

fn clean_names(values: Vec<String>) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for value in values {
        let value = clean(&value)?;

        if seen.insert(name_key(&value)) {
            result.push(value);
        }
    }

    Ok(result)
}

fn validate(mut metadata: Metadata) -> Result<Metadata, String> {
    if metadata.copyrights.len() > 500
        || metadata.characters.len() > 500
        || metadata.tags.len() > 500
    {
        return Err("A preset supports at most 500 entries per field.".into());
    }

    metadata.artist = clean(&metadata.artist)?;
    metadata.copyrights = clean_names(metadata.copyrights)?;
    metadata.tags = clean_names(metadata.tags)?;

    let mut seen = HashSet::new();
    let mut characters = Vec::new();

    for character in metadata.characters {
        let name = clean(&character.name)?;
        let copyright = clean(&character.copyright)?;
        let key = name_key(&copyright);
        let copyright = metadata
            .copyrights
            .iter()
            .find(|value| name_key(value) == key)
            .ok_or("Every character must belong to a selected copyright.")?
            .clone();

        if seen.insert((key, name_key(&name))) {
            characters.push(Character { name, copyright });
        }
    }

    metadata.characters = characters;
    Ok(metadata)
}

async fn list(pool: &SqlitePool) -> Result<Vec<Preset>, String> {
    let rows = sqlx::query_as::<_, StoredPreset>(concat!(
        "SELECT id, name, metadata FROM metadata_presets ",
        "ORDER BY name_key, id",
    ))
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    rows.into_iter()
        .map(|row| {
            Ok(Preset {
                id: row.id.to_string(),
                name: row.name,
                metadata: serde_json::from_str(&row.metadata).map_err(|error| error.to_string())?,
            })
        })
        .collect()
}

async fn save(
    pool: &SqlitePool,
    id: Option<i64>,
    name: &str,
    metadata: Metadata,
) -> Result<Preset, String> {
    let name = clean(name)?;
    let metadata = validate(metadata)?;
    let json = serde_json::to_string(&metadata).map_err(|error| error.to_string())?;
    let key = name_key(&name);

    let result = if let Some(id) = id {
        sqlx::query(concat!(
            "UPDATE metadata_presets SET name=?, name_key=?, metadata=? ",
            "WHERE id=? RETURNING id",
        ))
        .bind(&name)
        .bind(key)
        .bind(json)
        .bind(id)
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query(concat!(
            "INSERT INTO metadata_presets(name, name_key, metadata) ",
            "VALUES (?, ?, ?) RETURNING id",
        ))
        .bind(&name)
        .bind(key)
        .bind(json)
        .fetch_optional(pool)
        .await
    };

    let row = result
        .map_err(|error| {
            if error
                .as_database_error()
                .is_some_and(|error| error.is_unique_violation())
            {
                "A preset with this name already exists.".to_string()
            } else {
                error.to_string()
            }
        })?
        .ok_or("This preset no longer exists. Reload the presets.")?;

    Ok(Preset {
        id: row.get::<i64, _>("id").to_string(),
        name,
        metadata,
    })
}

async fn remove(pool: &SqlitePool, id: i64) -> Result<(), String> {
    let result = sqlx::query("DELETE FROM metadata_presets WHERE id=?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;

    if result.rows_affected() == 0 {
        return Err("This preset no longer exists. Reload the presets.".into());
    }

    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub async fn get_metadata_presets(
    database: tauri::State<'_, Database>,
) -> Result<Vec<Preset>, String> {
    list(database.pool()).await
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(update = id.is_some()), err)]
pub async fn save_metadata_preset(
    database: tauri::State<'_, Database>,
    id: Option<String>,
    name: String,
    metadata: Metadata,
) -> Result<Preset, String> {
    tracing::info!("Saving metadata preset");

    let id = id
        .map(|id| id.parse::<i64>().map_err(|_| "Invalid preset ID."))
        .transpose()?;
    let preset = save(database.pool(), id, &name, metadata).await?;

    tracing::info!("Metadata preset saved");
    Ok(preset)
}

#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub async fn delete_metadata_preset(
    database: tauri::State<'_, Database>,
    id: String,
) -> Result<(), String> {
    tracing::info!("Deleting metadata preset");
    let id = id.parse::<i64>().map_err(|_| "Invalid preset ID.")?;

    remove(database.pool(), id).await?;

    tracing::info!("Metadata preset deleted");
    Ok(())
}

#[cfg(test)]
mod tests;
