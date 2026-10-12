//! library: queries responsibility.

use std::path::PathBuf;

use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use super::{Character, Count, File, Filters, Page, Suggestion, configured_root};
use crate::{config::ConfigStore, database::Database, naming::name_key, stage};

fn exclude_stage(
    q: &mut QueryBuilder<Sqlite>,
    root: &Option<String>,
    stage_prefix: &Option<String>,
) {
    if let Some(prefix) = stage_prefix {
        q.push(" AND (m.library_assignment=")
            .push_bind(root)
            .push(" OR substr(replace(s.root_path || '/' || m.relative_path, char(92), '/'),1,")
            .push_bind(prefix.chars().count() as i64)
            .push(") COLLATE NOCASE != ")
            .push_bind(prefix)
            .push(")");
    }
}

fn base(
    root: &Option<String>,
    favorites: bool,
    prefix: &str,
    stage_prefix: &Option<String>,
    joins: &str,
) -> QueryBuilder<Sqlite> {
    let mut q = QueryBuilder::new(prefix);
    q.push(" FROM media m JOIN sources s ON s.id=m.source_id LEFT JOIN video_metadata v ON v.media_id=m.id LEFT JOIN artists a ON a.id=m.artist_id");
    q.push(joins);
    q.push(" WHERE (m.library_assignment=");
    q.push_bind(root)
        .push(" OR (s.kind='library' AND s.path_key=")
        .push_bind(root)
        .push("))")
        .push(" AND m.availability='present' AND m.needs_distribution=0");

    if favorites {
        q.push(" AND m.favorite=1");
    }
    exclude_stage(&mut q, root, stage_prefix);
    q
}

pub(super) fn filtered(root: &Option<String>, f: &Filters, prefix: &str) -> QueryBuilder<Sqlite> {
    let mut q = base(root, f.scope == "favorites", prefix, &f.stage_prefix, "");

    if f.kind != "all" {
        q.push(" AND m.kind=").push_bind(&f.kind);
    }

    if f.tagged {
        q.push(" AND EXISTS(SELECT 1 FROM media_tags WHERE media_id=m.id)");
    }
    apply_metadata_search(&mut q, &f.search);
    q
}

pub(crate) fn apply_metadata_search(q: &mut QueryBuilder<Sqlite>, search: &str) {
    for value in search.split(';').map(str::trim).filter(|v| !v.is_empty()) {
        let key = name_key(value);
        q.push(" AND (EXISTS(SELECT 1 FROM artists d WHERE d.id=m.artist_id AND d.name_key=").push_bind(key.clone())
            .push(concat!(
                ") OR EXISTS(SELECT 1 FROM media_copyrights l JOIN copyrights d ON d.id=l.copyright_id ",
                "WHERE l.media_id=m.id AND d.name_key=",
            )).push_bind(key.clone())
            .push(concat!(
                ") OR EXISTS(SELECT 1 FROM media_characters l JOIN characters d ON d.id=l.character_id ",
                "WHERE l.media_id=m.id AND d.name_key=",
            )).push_bind(key.clone())
            .push(concat!(
                ") OR EXISTS(SELECT 1 FROM media_tags l JOIN tags d ON d.id=l.tag_id WHERE ",
                "l.media_id=m.id AND d.name_key=",
            )).push_bind(key).push("))");
    }
}

pub(crate) async fn browse(
    pool: &SqlitePool,
    root: Option<String>,
    f: Filters,
) -> Result<Page, String> {
    if f.offset < 0
        || !matches!(f.scope.as_str(), "gallery" | "favorites")
        || !matches!(f.kind.as_str(), "all" | "image" | "video")
        || !matches!(
            f.sort.as_str(),
            "newest" | "older" | "name-asc" | "name-desc"
        )
    {
        return Err("Invalid Library filters".into());
    }

    let mut counts = base(
        &root,
        f.scope == "favorites",
        "SELECT count(*),coalesce(sum(m.kind='video'),0),coalesce(sum(m.kind='image'),0)",
        &f.stage_prefix,
        "",
    );

    let row = counts
        .build()
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    let type_counts = [row.get(0), row.get(1), row.get(2)];

    let total = count_filtered(pool, &root, &f).await?;

    let mut q = select_filtered(&root, &f);
    q.push(match f.sort.as_str() {
        "older" => " ORDER BY m.file_modified_at ASC,m.id ASC",
        "name-asc" => " ORDER BY m.relative_path COLLATE NEXA_FILENAME ASC,m.id ASC",
        "name-desc" => " ORDER BY m.relative_path COLLATE NEXA_FILENAME DESC,m.id ASC",
        _ => " ORDER BY m.file_modified_at DESC,m.id ASC",
    });
    q.push(" LIMIT 500 OFFSET ").push_bind(f.offset);

    let rows = q.build().fetch_all(pool).await.map_err(|e| e.to_string())?;

    let mut files = Vec::with_capacity(rows.len());

    for row in rows {
        files.push(File {
            favorite: row.get::<i64, _>("favorite") != 0,
            artist: row
                .get::<Option<String>, _>("artist_name")
                .unwrap_or_default(),
            video_title: row.get("title"),
            technical: stage::stage_file(row),
            copyrights: Vec::new(),
            character_copyrights: Vec::new(),
            tags: Vec::new(),
        });
    }

    for (table, dictionary, foreign) in [
        ("media_copyrights", "copyrights", "copyright_id"),
        ("media_tags", "tags", "tag_id"),
        ("media_characters", "characters", "character_id"),
    ] {
        if files.is_empty() {
            break;
        }

        let mut q = QueryBuilder::<Sqlite>::new("SELECT l.media_id,d.name");

        if table == "media_characters" {
            q.push(",c.name copyright");
        }
        q.push(" FROM ")
            .push(table)
            .push(" l JOIN ")
            .push(dictionary)
            .push(" d ON d.id=l.")
            .push(foreign);

        if table == "media_characters" {
            q.push(" JOIN copyrights c ON c.id=d.copyright_id");
        }
        q.push(" WHERE l.media_id IN (");

        let mut separated = q.separated(",");

        for file in &files {
            separated.push_bind(file.technical.id.parse::<i64>().map_err(|_| "Invalid ID")?);
        }
        separated.push_unseparated(") ORDER BY d.name_key,d.id");

        for row in q.build().fetch_all(pool).await.map_err(|e| e.to_string())? {
            let id = row.get::<i64, _>("media_id").to_string();

            if let Some(file) = files.iter_mut().find(|file| file.technical.id == id) {
                let name = row.get("name");

                match table {
                    "media_tags" => file.tags.push(name),
                    "media_characters" => file.character_copyrights.push(Character {
                        name,
                        copyright: row.get("copyright"),
                    }),
                    _ => file.copyrights.push(name),
                }
            }
        }
    }

    let mut tag_query = base(
        &root,
        f.scope == "favorites",
        "SELECT d.name,count(*) count",
        &f.stage_prefix,
        " JOIN media_tags l ON l.media_id=m.id JOIN tags d ON d.id=l.tag_id",
    );
    tag_query.push(" GROUP BY d.id ORDER BY count DESC,d.name_key");

    let rows = tag_query
        .build()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let tags = rows
        .into_iter()
        .map(|r| Count {
            name: r.get("name"),
            count: r.get("count"),
        })
        .collect();

    let mut suggestions = Vec::new();

    for (table, category) in [
        ("copyrights", "Copyright"),
        ("artists", "Artist"),
        ("characters", "Character"),
        ("tags", "Tag"),
    ] {
        let mut q = QueryBuilder::<Sqlite>::new("SELECT name FROM ");
        q.push(table).push(" ORDER BY name_key");

        for row in q.build().fetch_all(pool).await.map_err(|e| e.to_string())? {
            suggestions.push(Suggestion {
                name: row.get("name"),
                category,
            });
        }
    }

    let characters = sqlx::query(concat!(
        "SELECT d.name,c.name copyright FROM characters d JOIN copyrights c ON ",
        "c.id=d.copyright_id ORDER BY d.name_key,c.name_key",
    ))
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|row| Character {
        name: row.get("name"),
        copyright: row.get("copyright"),
    })
    .collect();
    Ok(Page {
        files,
        total,
        type_counts,
        tags,
        suggestions,
        characters,
    })
}

fn select_filtered(root: &Option<String>, f: &Filters) -> QueryBuilder<Sqlite> {
    filtered(
        root,
        f,
        concat!(
            "SELECT m.*,s.root_path,v.duration_ms,v.fps_num,v.fps_den,v.codec,v.bitrate_bps,",
            "v.title,a.name artist_name",
        ),
    )
}

async fn count_filtered(
    pool: &SqlitePool,
    root: &Option<String>,
    f: &Filters,
) -> Result<i64, String> {
    filtered(root, f, "SELECT count(*)")
        .build_query_scalar()
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_library_files(
    mut filters: Filters,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<Page, String> {
    filters.stage_prefix = config.snapshot()?.settings.staging_path.map(|p| {
        format!(
            "{}/",
            p.components()
                .collect::<PathBuf>()
                .to_string_lossy()
                .replace('\\', "/")
                .trim_end_matches('/')
        )
    });
    browse(database.pool(), configured_root(&config)?, filters).await
}
