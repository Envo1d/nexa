use std::collections::HashMap;

use serde_json::{Value, json};

use super::snapshots::{Record, Snapshot};

fn index<'a>(snapshot: &'a Snapshot, table: &str) -> HashMap<i64, &'a Record> {
    snapshot.tables[table]
        .iter()
        .filter_map(|row| row["id"].as_i64().map(|id| (id, row)))
        .collect()
}

fn names(snapshot: &Snapshot, table: &str) -> Vec<Value> {
    snapshot.tables[table]
        .iter()
        .map(|row| row["name"].clone())
        .collect()
}

pub(super) fn export(snapshot: &Snapshot) -> Result<Value, String> {
    let sources = index(snapshot, "sources");
    let artists = index(snapshot, "artists");
    let copyrights = index(snapshot, "copyrights");
    let characters = index(snapshot, "characters");
    let tags = index(snapshot, "tags");
    let videos: HashMap<_, _> = snapshot.tables["video_metadata"]
        .iter()
        .filter_map(|row| row["media_id"].as_i64().map(|id| (id, row)))
        .collect();

    let qualified = |row: &Record| -> Result<Value, String> {
        let parent = row["copyright_id"]
            .as_i64()
            .and_then(|id| copyrights.get(&id))
            .ok_or("Invalid character copyright")?;
        Ok(json!({ "name": row["name"], "copyright": parent["name"] }))
    };

    // Build link indexes once, including for large libraries.
    let mut links: HashMap<(&str, i64), Vec<Value>> = HashMap::new();

    for (table, key, dictionary) in [
        ("media_copyrights", "copyright_id", &copyrights),
        ("media_tags", "tag_id", &tags),
        ("media_characters", "character_id", &characters),
    ] {
        for row in &snapshot.tables[table] {
            let media_id = row["media_id"].as_i64().ok_or("Invalid media link")?;
            let entry = row[key]
                .as_i64()
                .and_then(|id| dictionary.get(&id))
                .ok_or("Invalid taxonomy link")?;
            let value = if table == "media_characters" {
                qualified(entry)?
            } else {
                entry["name"].clone()
            };
            links.entry((table, media_id)).or_default().push(value);
        }
    }

    let mut files = Vec::new();

    for row in &snapshot.tables["media"] {
        let id = row["id"].as_i64().ok_or("Invalid media ID")?;
        let source = row["source_id"]
            .as_i64()
            .and_then(|id| sources.get(&id))
            .ok_or("Invalid media source")?;
        let artist = row["artist_id"]
            .as_i64()
            .and_then(|id| artists.get(&id))
            .map(|row| &row["name"]);

        files.push(json!({
            "sourceRoot": source["root_path"], "sourceKind": source["kind"],
            "relativePath": row["relative_path"], "originalFilename": row["original_filename"],
            "kind": row["kind"], "availability": row["availability"],
            "libraryAssignment": row["library_assignment"],
            "needsDistribution": row["needs_distribution"] == 1,
            "artist": artist,
            "copyrights": links.get(&("media_copyrights", id)).cloned().unwrap_or_default(),
            "characters": links.get(&("media_characters", id)).cloned().unwrap_or_default(),
            "tags": links.get(&("media_tags", id)).cloned().unwrap_or_default(),
            "favorite": row["favorite"] == 1,
            "videoTitle": videos.get(&id).map(|row| &row["title"]),
            "namingOrderKey": row["naming_order_key"], "namingOrder": row["naming_order"],
        }));
    }

    let characters = snapshot.tables["characters"]
        .iter()
        .map(qualified)
        .collect::<Result<Vec<_>, _>>()?;
    let presets = snapshot.tables["metadata_presets"]
        .iter()
        .map(|row| {
            let metadata: Value =
                serde_json::from_str(row["metadata"].as_str().ok_or("Invalid preset")?)
                    .map_err(|error| error.to_string())?;
            Ok(json!({ "name": row["name"], "metadata": metadata }))
        })
        .collect::<Result<Vec<Value>, String>>()?;

    Ok(json!({
        "format": "nexa-portable-metadata", "version": 1, "createdAt": snapshot.created_at,
        "sources": snapshot.tables["sources"].iter().map(|row| json!({
            "root": row["root_path"], "kind": row["kind"], "active": row["is_active"] == 1,
        })).collect::<Vec<_>>(),
        "taxonomy": { "artists": names(snapshot, "artists"), "copyrights": names(snapshot, "copyrights"),
            "characters": characters, "tags": names(snapshot, "tags") },
        "presets": presets, "files": files,
    }))
}
