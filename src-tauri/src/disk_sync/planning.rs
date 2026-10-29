//! disk_sync: planning responsibility.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use sqlx::{Row, SqlitePool};

use super::component;
use crate::{config::AppConfig, naming::name_key};

pub(super) struct Target {
    pub(super) id: i64,
    pub(super) folder: PathBuf,
    pub(super) stem: String,
    pub(super) extension: String,
    pub(super) current_root: String,
    pub(super) current_relative: String,
    pub(super) position: Option<i64>,
    incoming_position: Option<i64>,
    pub(super) kind: String,
    pub(super) revision: Option<String>,
}

fn current_position(target: &Target, root: &str) -> Option<u64> {
    let current = Path::new(&target.current_relative);

    let same_destination = name_key(&target.current_root) == name_key(root)
        && name_key(&current.parent()?.to_string_lossy())
            == name_key(&target.folder.to_string_lossy());

    if !same_destination {
        return None;
    }

    let current_stem = name_key(current.file_stem()?.to_str()?);

    let stem = name_key(&target.stem);

    if current_stem == stem {
        return Some(0);
    }

    let suffix = current_stem.strip_prefix(&format!("{stem} - "))?;

    let canonical_index = !suffix.starts_with('0')
        && !suffix.is_empty()
        && suffix.bytes().all(|byte| byte.is_ascii_digit());

    if !canonical_index {
        return None;
    }

    suffix.parse().ok()
}

pub(super) async fn grouped_targets(
    pool: &SqlitePool,
    root: &str,
    settings: &AppConfig,
) -> Result<BTreeMap<String, Vec<Target>>, String> {
    let mut groups: BTreeMap<String, Vec<Target>> = BTreeMap::new();

    let incoming_key = super::distribution_order_key(root)?;

    let mut cursor = 0;

    loop {
        let rows = sqlx::query(concat!(
            "SELECT m.id,m.kind,m.extension,m.original_filename,m.relative_path,m.file_revision,",
            "m.naming_order_key,m.naming_order,s.root_path,a.name artist,v.title FROM media m JOIN ",
            "sources s ON s.id=m.source_id JOIN artists a ON a.id=m.artist_id LEFT JOIN ",
            "video_metadata v ON v.media_id=m.id WHERE m.id>? AND m.availability='present' AND ",
            "m.needs_distribution=0 AND (m.library_assignment=? OR (s.kind='library' AND ",
            "s.path_key=?)) ORDER BY m.id LIMIT 128",
        ))
.bind(cursor)
.bind(root)
.bind(root)
.fetch_all(pool)
.await
.map_err(|e| e.to_string())?;

        if rows.is_empty() {
            break;
        }

        for row in rows {
            let id: i64 = row.get("id");
            cursor = id;

            let characters: Vec<String> = sqlx::query_scalar(concat!(
                "SELECT c.name FROM media_characters l JOIN characters c ON c.id=l.character_id WHERE ",
                "l.media_id=? ORDER BY c.name_key,c.id",
            ))
.bind(id)
.fetch_all(pool)
.await
.map_err(|e| e.to_string())?;

            let copyright: Option<String> = sqlx::query_scalar(concat!(
                "SELECT c.name FROM media_copyrights l JOIN copyrights c ON c.id=l.copyright_id WHERE ",
                "l.media_id=? ORDER BY c.name_key,c.id LIMIT 1",
            ))
.bind(id)
.fetch_optional(pool)
.await
.map_err(|e| e.to_string())?;

            let artist: String = row.get("artist");

            let is_video = row.get::<String, _>("kind") == "video";

            let original: String = row.get("original_filename");

            let fallback = Path::new(&original)
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Invalid original filename")?;

            let base = if is_video {
                let title = row
                    .get::<Option<String>, _>("title")
                    .filter(|s| !s.trim().is_empty());

                let Some(title) = title else {
                    continue;
                };

                if characters.is_empty() {
                    title
                } else {
                    format!(
                        "{} - {title}",
                        characters.join(&settings.character_separator)
                    )
                }
            } else if characters.is_empty() {
                fallback.into()
            } else {
                characters.join(&settings.character_separator)
            };

            let stem = component(&format!("{base} [{artist}]"))?;

            let extension = component(&row.get::<String, _>("extension"))?.to_lowercase();

            let mut folder = PathBuf::new();

            if settings.copyright_directory
                && let Some(copyright) = copyright
            {
                folder.push(component(&copyright)?);
            }

            if settings.artist_directory {
                folder.push(component(&artist)?);
            }

            if is_video && !settings.video_subfolder.trim().is_empty() {
                folder.push(component(&settings.video_subfolder)?);
            } else if extension == "gif" && !settings.gif_subfolder.trim().is_empty() {
                folder.push(component(&settings.gif_subfolder)?);

                tracing::debug!(
                    media_id = id,
                    "GIF destination uses configured animation subfolder"
                );
            }

            let key = name_key(&folder.join(&stem).to_string_lossy());

            let key = serde_json::to_string(&(root, key, "stem")).map_err(|e| e.to_string())?;
            // Accept saved extension-specific positions until the next explicit save.
            let legacy_key =
                name_key(&folder.join(format!("{stem}.{extension}")).to_string_lossy());

            let legacy_key =
                serde_json::to_string(&(root, legacy_key)).map_err(|e| e.to_string())?;

            let saved_key = row.get::<Option<String>, _>("naming_order_key");

            let position = if saved_key.as_deref() == Some(key.as_str()) {
                row.get::<Option<i64>, _>("naming_order")
            } else if saved_key.as_deref() == Some(legacy_key.as_str()) {
                tracing::debug!(media_id = id, "Using legacy extension-specific file order");
                row.get::<Option<i64>, _>("naming_order")
            } else {
                None
            };

            let incoming_position = if saved_key.as_deref() == Some(incoming_key.as_str()) {
                row.get::<Option<i64>, _>("naming_order")
            } else {
                None
            };

            groups.entry(key).or_default().push(Target {
                id,
                folder,
                stem,
                extension,
                current_root: row.get("root_path"),
                current_relative: row.get("relative_path"),
                position,
                incoming_position,
                kind: row.get("kind"),
                revision: row.get("file_revision"),
            });
        }
    }

    for group in groups.values_mut() {
        group.sort_by_cached_key(|target| {
            if let Some(position) = target.position {
                (0, position, 0, target.id)
            } else if let Some(position) = current_position(target, root) {
                (1, 0, position, target.id)
            } else {
                (
                    2,
                    target.incoming_position.unwrap_or(i64::MAX),
                    0,
                    target.id,
                )
            }
        });

        tracing::debug!(
            files = group.len(),
            "Naming group ordered by saved ranks, existing filenames and distribution order with ID fallback"
        );
    }
    Ok(groups)
}

pub(super) async fn targets(
    pool: &SqlitePool,
    root: &str,
    settings: &AppConfig,
) -> Result<Vec<(i64, String, String, String)>, String> {
    let groups = grouped_targets(pool, root, settings).await?;

    let mut result = Vec::new();

    for group in groups.into_values() {
        let count = group.len();

        for (index, target) in group.into_iter().enumerate() {
            let suffix = if count > 1 {
                format!(" - {}", index + 1)
            } else {
                String::new()
            };

            let filename = component(&format!("{}{suffix}.{}", target.stem, target.extension))?;
            result.push((
                target.id,
                target.folder.join(filename).to_string_lossy().into_owned(),
                target.current_root,
                target.current_relative,
            ));
        }
    }
    Ok(result)
}
