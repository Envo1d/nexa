//! Logical Main membership and transactional annotations; disk work is queued separately.
use crate::{config::ConfigStore, database::Database, legacy_import::name_key, stage};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use std::path::PathBuf;
use unicode_normalization::UnicodeNormalization;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filters {
    scope: String,
    search: String,
    kind: String,
    sort: String,
    tagged: bool,
    offset: i64,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct Character {
    name: String,
    copyright: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    #[serde(flatten)]
    technical: stage::StageFile,
    favorite: bool,
    copyrights: Vec<String>,
    artist: String,
    character_copyrights: Vec<Character>,
    tags: Vec<String>,
    video_title: Option<String>,
}
#[derive(Serialize)]
pub struct Count {
    name: String,
    count: i64,
}
#[derive(Serialize)]
pub struct Suggestion {
    name: String,
    category: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    files: Vec<File>,
    total: i64,
    type_counts: [i64; 3],
    tags: Vec<Count>,
    suggestions: Vec<Suggestion>,
    characters: Vec<Character>,
}
fn base(root: &Option<String>, favorites: bool, prefix: &str) -> QueryBuilder<Sqlite> {
    let mut q = QueryBuilder::new(prefix);
    q.push(" FROM media m JOIN sources s ON s.id=m.source_id LEFT JOIN video_metadata v ON v.media_id=m.id LEFT JOIN artists a ON a.id=m.artist_id WHERE (m.library_assignment=");
    q.push_bind(root)
        .push(" OR (s.kind='library' AND s.path_key=")
        .push_bind(root)
        .push("))")
        .push(" AND m.availability='present' AND m.needs_distribution=0");
    if favorites {
        q.push(" AND m.favorite=1");
    }
    q
}
fn filtered(root: &Option<String>, f: &Filters, prefix: &str) -> QueryBuilder<Sqlite> {
    let mut q = base(root, f.scope == "favorites", prefix);
    if f.kind != "all" {
        q.push(" AND m.kind=").push_bind(&f.kind);
    }
    if f.tagged {
        q.push(" AND EXISTS(SELECT 1 FROM media_tags WHERE media_id=m.id)");
    }
    for value in f.search.split(';').map(str::trim).filter(|v| !v.is_empty()) {
        let key = name_key(value);
        q.push(" AND (EXISTS(SELECT 1 FROM artists d WHERE d.id=m.artist_id AND d.name_key=").push_bind(key.clone())
            .push(") OR EXISTS(SELECT 1 FROM media_copyrights l JOIN copyrights d ON d.id=l.copyright_id WHERE l.media_id=m.id AND d.name_key=").push_bind(key.clone())
            .push(") OR EXISTS(SELECT 1 FROM media_characters l JOIN characters d ON d.id=l.character_id WHERE l.media_id=m.id AND d.name_key=").push_bind(key.clone())
            .push(") OR EXISTS(SELECT 1 FROM media_tags l JOIN tags d ON d.id=l.tag_id WHERE l.media_id=m.id AND d.name_key=").push_bind(key).push("))");
    }
    q
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
    let mut counts = QueryBuilder::<Sqlite>::new(
        "SELECT count(*),coalesce(sum(m.kind='video'),0),coalesce(sum(m.kind='image'),0)",
    );
    counts
        .push(" FROM media m JOIN sources s ON s.id=m.source_id WHERE (m.library_assignment=")
        .push_bind(&root)
        .push(" OR (s.kind='library' AND s.path_key=")
        .push_bind(&root)
        .push(")) AND m.availability='present' AND m.needs_distribution=0");
    if f.scope == "favorites" {
        counts.push(" AND m.favorite=1");
    }
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
    let rows = sqlx::query("SELECT d.name,count(*) count FROM tags d JOIN media_tags l ON l.tag_id=d.id JOIN media m ON m.id=l.media_id JOIN sources s ON s.id=m.source_id WHERE (m.library_assignment=? OR (s.kind='library' AND s.path_key=?)) AND m.availability='present' AND m.needs_distribution=0 AND (?=0 OR m.favorite=1) GROUP BY d.id ORDER BY count DESC,d.name_key")
        .bind(&root).bind(&root).bind(f.scope == "favorites").fetch_all(pool).await.map_err(|e| e.to_string())?;
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
    let characters = sqlx::query("SELECT d.name,c.name copyright FROM characters d JOIN copyrights c ON c.id=d.copyright_id ORDER BY d.name_key,c.name_key")
        .fetch_all(pool).await.map_err(|e| e.to_string())?.into_iter().map(|row| Character {name: row.get("name"), copyright: row.get("copyright")}).collect();
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
        "SELECT m.*,s.root_path,v.duration_ms,v.fps_num,v.fps_den,v.codec,v.bitrate_bps,v.title,a.name artist_name",
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
    filters: Filters,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<Page, String> {
    browse(database.pool(), configured_root(&config)?, filters).await
}
fn configured_root(config: &ConfigStore) -> Result<Option<String>, String> {
    Ok(config.snapshot()?.settings.library_path.map(|p| {
        p.components()
            .collect::<PathBuf>()
            .to_string_lossy()
            .into_owned()
    }))
}
async fn ensure_library(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    id: i64,
    root: &Option<String>,
) -> Result<String, String> {
    sqlx::query_scalar("SELECT m.kind FROM media m JOIN sources s ON s.id=m.source_id WHERE m.id=? AND (m.library_assignment=? OR (s.kind='library' AND s.path_key=?)) AND m.needs_distribution=0 AND m.availability='present'")
        .bind(id).bind(root).bind(root).fetch_optional(&mut **tx).await.map_err(|e| e.to_string())?.ok_or("File is outside the current Library".into())
}
fn clean(value: &str) -> Result<String, String> {
    let value: String = value.trim().nfc().collect();
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err("Metadata names must be nonempty without control characters".into());
    }
    Ok(value)
}
async fn dictionary(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    table: &str,
    value: &str,
) -> Result<i64, String> {
    let name = clean(value)?;
    let mut q = QueryBuilder::<Sqlite>::new("INSERT INTO ");
    q.push(table)
        .push("(name,name_key) VALUES(")
        .push_bind(&name)
        .push(",")
        .push_bind(name_key(&name))
        .push(") ON CONFLICT(name_key) DO UPDATE SET name_key=excluded.name_key RETURNING id");
    q.build_query_scalar()
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| e.to_string())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    copyrights: Vec<String>,
    artists: Vec<String>,
    characters: Vec<Character>,
    tags: Vec<String>,
    video_title: String,
}
async fn write_tags(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    id: i64,
    tags: &[String],
) -> Result<(), String> {
    sqlx::query("DELETE FROM media_tags WHERE media_id=?")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    for tag in tags {
        let tag = dictionary(tx, "tags", tag).await?;
        sqlx::query("INSERT OR IGNORE INTO media_tags VALUES(?,?)")
            .bind(id)
            .bind(tag)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
async fn save(
    pool: &SqlitePool,
    root: Option<String>,
    ids: Vec<String>,
    draft: Draft,
) -> Result<(), String> {
    save_mode(pool, root, None, ids, draft).await
}
async fn save_mode(
    pool: &SqlitePool,
    root: Option<String>,
    stage_root: Option<Option<String>>,
    ids: Vec<String>,
    draft: Draft,
) -> Result<(), String> {
    if ids.is_empty() || ids.len() > 500 || draft.artists.len() != 1 {
        return Err("Choose exactly one artist and 1–500 files".into());
    }
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let artist = dictionary(&mut tx, "artists", &draft.artists[0]).await?;
    for raw in ids {
        let id = raw.parse::<i64>().map_err(|_| "Invalid media ID")?;
        let kind: String = if let Some(stage_root) = &stage_root {
            sqlx::query_scalar("SELECT m.kind FROM media m JOIN sources s ON s.id=m.source_id WHERE m.id=? AND m.availability='present' AND (m.needs_distribution=1 OR (s.kind='stage' AND s.path_key=? AND m.library_assignment IS NULL))")
                .bind(id).bind(stage_root).fetch_optional(&mut *tx).await.map_err(|e| e.to_string())?.ok_or("File is outside Stage")?
        } else {
            ensure_library(&mut tx, id, &root).await?
        };
        if kind == "video" && draft.video_title.trim().is_empty() {
            return Err("Video title is required".into());
        }
        sqlx::query("DELETE FROM media_copyrights WHERE media_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        let mut copyrights = std::collections::HashMap::new();
        for name in &draft.copyrights {
            let copyright = dictionary(&mut tx, "copyrights", name).await?;
            copyrights.insert(name_key(name.trim()), copyright);
            sqlx::query("INSERT OR IGNORE INTO media_copyrights VALUES(?,?)")
                .bind(id)
                .bind(copyright)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }
        for character in &draft.characters {
            let copyright = *copyrights
                .get(&name_key(character.copyright.trim()))
                .ok_or("Select the character's copyright")?;
            let name = clean(&character.name)?;
            let character: i64 = sqlx::query_scalar("INSERT INTO characters(copyright_id,name,name_key) VALUES(?,?,?) ON CONFLICT(copyright_id,name_key) DO UPDATE SET name_key=excluded.name_key RETURNING id").bind(copyright).bind(&name).bind(name_key(&name)).fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
            sqlx::query("INSERT OR IGNORE INTO media_characters VALUES(?,?,?)")
                .bind(id)
                .bind(character)
                .bind(copyright)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }
        write_tags(&mut tx, id, &draft.tags).await?;
        sqlx::query("UPDATE media SET artist_id=?,library_assignment=?,needs_distribution=0,updated_at=CAST(strftime('%s','now') AS INTEGER)*1000 WHERE id=?").bind(artist).bind(&root).bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        if kind == "video" {
            sqlx::query("INSERT INTO video_metadata(media_id,title) VALUES(?,?) ON CONFLICT(media_id) DO UPDATE SET title=excluded.title").bind(id).bind(draft.video_title.trim()).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
    }
    tx.commit().await.map_err(|e| e.to_string())
}
#[tauri::command]
#[tracing::instrument(skip_all, fields(count=ids.len()), err)]
pub async fn save_library_metadata(
    app: tauri::AppHandle,
    state: tauri::State<'_, stage::StageState>,
    ids: Vec<String>,
    draft: Draft,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<(), String> {
    let _guard = state.operation().await;
    crate::disk_sync::recover(database.pool()).await?;
    tracing::info!("Library metadata save started");
    save(database.pool(), configured_root(&config)?, ids, draft).await?;
    crate::disk_sync::request(&app);
    tracing::info!("Library metadata save completed");
    Ok(())
}
#[tauri::command]
#[tracing::instrument(skip_all, fields(count=ids.len()), err)]
pub async fn distribute_files(
    app: tauri::AppHandle,
    ids: Vec<String>,
    draft: Draft,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
    state: tauri::State<'_, stage::StageState>,
) -> Result<(), String> {
    let _guard = state.operation().await;
    crate::disk_sync::recover(database.pool()).await?;
    let settings = config.snapshot()?.settings;
    let root = configured_root(&config)?.ok_or("Choose a Library directory before distribution")?;
    let staging = settings.staging_path.map(|p| {
        p.components()
            .collect::<PathBuf>()
            .to_string_lossy()
            .into_owned()
    });
    tracing::info!("Distribution metadata save started");
    save_mode(database.pool(), Some(root), Some(staging), ids, draft).await?;
    crate::disk_sync::request(&app);
    tracing::info!("Distribution saved; physical synchronization queued when enabled");
    Ok(())
}
#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub async fn save_library_tags(
    id: String,
    tags: Vec<String>,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<(), String> {
    let id = id.parse::<i64>().map_err(|_| "Invalid media ID")?;
    let mut tx = database.pool().begin().await.map_err(|e| e.to_string())?;
    ensure_library(&mut tx, id, &configured_root(&config)?).await?;
    write_tags(&mut tx, id, &tags).await?;
    sqlx::query(
        "UPDATE media SET updated_at=CAST(strftime('%s','now') AS INTEGER)*1000 WHERE id=?",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    tracing::info!("Library tags saved");
    Ok(())
}
#[tauri::command]
#[tracing::instrument(skip_all, err)]
pub async fn set_library_favorite(
    id: String,
    favorite: bool,
    database: tauri::State<'_, Database>,
    config: tauri::State<'_, ConfigStore>,
) -> Result<(), String> {
    let id = id.parse::<i64>().map_err(|_| "Invalid media ID")?;
    let mut tx = database.pool().begin().await.map_err(|e| e.to_string())?;
    ensure_library(&mut tx, id, &configured_root(&config)?).await?;
    sqlx::query("UPDATE media SET favorite=?,updated_at=CAST(strftime('%s','now') AS INTEGER)*1000 WHERE id=?").bind(favorite).bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    tracing::debug!("Library favorite saved");
    Ok(())
}

/// Natural, Unicode-insensitive filename ordering; directory prefixes do not affect names.
pub(crate) fn compare_names(a: &str, b: &str) -> std::cmp::Ordering {
    let a = name_key(a.rsplit(['/', '\\']).next().unwrap_or(a));
    let b = name_key(b.rsplit(['/', '\\']).next().unwrap_or(b));
    let mut a = a.as_str();
    let mut b = b.as_str();
    while !a.is_empty() && !b.is_empty() {
        let ac = a.chars().next().unwrap();
        let bc = b.chars().next().unwrap();
        if ac.is_ascii_digit() && bc.is_ascii_digit() {
            let an = a.bytes().take_while(u8::is_ascii_digit).count();
            let bn = b.bytes().take_while(u8::is_ascii_digit).count();
            let av = a[..an].trim_start_matches('0');
            let bv = b[..bn].trim_start_matches('0');
            let order = av.len().cmp(&bv.len()).then_with(|| av.cmp(bv));
            if !order.is_eq() {
                return order;
            }
            a = &a[an..];
            b = &b[bn..];
        } else {
            let order = ac.cmp(&bc);
            if !order.is_eq() {
                return order;
            }
            a = &a[ac.len_utf8()..];
            b = &b[bc.len_utf8()..];
        }
    }
    a.len().cmp(&b.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn filters() -> Filters {
        Filters {
            scope: "gallery".into(),
            kind: "all".into(),
            sort: "newest".into(),
            ..Default::default()
        }
    }
    async fn fixture() -> (tempfile::TempDir, Database, String) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp
            .path()
            .join("Library 日本")
            .to_string_lossy()
            .into_owned();
        let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
        sqlx::query(
            "INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(1,'library',?,?,1)",
        )
        .bind(&root)
        .bind(&root)
        .execute(db.pool())
        .await
        .unwrap();
        let mut tx = db.pool().begin().await.unwrap();
        for id in 1..=503i64 {
            let path = format!("nested/Image {id}.png");
            sqlx::query("INSERT INTO media(id,source_id,relative_path,original_filename,path_key,kind,extension,last_seen_at,created_at,updated_at,file_modified_at,favorite) VALUES(?,1,?,?,?,'image','png',1,1,1,?,?)")
                .bind(id).bind(&path).bind(format!("Image {id}.png")).bind(&path).bind(id).bind(id==2).execute(&mut *tx).await.unwrap();
        }
        sqlx::query("UPDATE media SET availability='missing' WHERE id=502")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE media SET needs_distribution=1 WHERE id=503")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        (temp, db, root)
    }
    fn draft() -> Draft {
        Draft {
            copyrights: vec!["Series".into(), "Other".into()],
            artists: vec!["Straße".into()],
            characters: vec![
                Character {
                    name: "Alice".into(),
                    copyright: "Series".into(),
                },
                Character {
                    name: "Alice".into(),
                    copyright: "Other".into(),
                },
            ],
            tags: vec!["Cafe\u{301}".into(), "CAFÉ".into()],
            video_title: String::new(),
        }
    }
    #[tokio::test]
    async fn pages_filter_the_entire_library_and_exclude_missing_queued_and_other_roots() {
        let (_temp, db, root) = fixture().await;
        let page = browse(db.pool(), Some(root.clone()), filters())
            .await
            .unwrap();
        assert_eq!(page.total, 501);
        assert_eq!(page.files.len(), 500);
        assert_eq!(page.type_counts, [501, 0, 501]);
        let mut f = filters();
        f.offset = 500;
        let next = browse(db.pool(), Some(root.clone()), f).await.unwrap();
        assert_eq!(next.files.len(), 1);
        assert_eq!(next.files[0].technical.id, "1");
        let mut f = filters();
        f.sort = "name-asc".into();
        let names = browse(db.pool(), Some(root.clone()), f).await.unwrap();
        assert_eq!(names.files[0].technical.id, "1");
        assert_eq!(names.files[1].technical.id, "2");
        let mut f = filters();
        f.scope = "favorites".into();
        let favorites = browse(db.pool(), Some(root.clone()), f).await.unwrap();
        assert_eq!(favorites.total, 1);
        assert!(favorites.files[0].favorite);
        assert_eq!(favorites.type_counts, [1, 0, 1]);
        assert_eq!(
            browse(db.pool(), Some("other root".into()), filters())
                .await
                .unwrap()
                .total,
            0
        );
        save(db.pool(), Some(root.clone()), vec!["1".into()], draft())
            .await
            .unwrap();
        let mut f = filters();
        f.search = "STRASSE; CAFÉ; Alice".into();
        f.tagged = true;
        let found = browse(db.pool(), Some(root.clone()), f).await.unwrap();
        assert_eq!(found.total, 1);
        assert_eq!(found.files[0].technical.id, "1");
        assert_eq!(found.files[0].character_copyrights.len(), 2);
        assert_eq!(found.files[0].tags, vec!["Café"]);
        assert_eq!(found.tags.len(), 1);
        assert_eq!(found.tags[0].count, 1);
        assert!(found.suggestions.iter().any(|entry| entry.name == "Straße"));
        let mut f = filters();
        f.search = "' OR 1=1 --".into();
        assert_eq!(browse(db.pool(), Some(root), f).await.unwrap().total, 0);
        db.close().await;
    }
    #[tokio::test]
    async fn metadata_saves_are_atomic_qualified_and_preserve_favorites_and_video_details() {
        let (temp, db, root) = fixture().await;
        save(db.pool(), Some(root.clone()), vec!["2".into()], draft())
            .await
            .unwrap();
        let mut bad = draft();
        bad.artists = vec!["Transient".into()];
        bad.characters[0].copyright = "Not selected".into();
        assert!(
            save(db.pool(), Some(root.clone()), vec!["2".into()], bad)
                .await
                .is_err()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM artists WHERE name='Transient'")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            0
        );
        assert!(
            save(
                db.pool(),
                Some(root.clone()),
                vec!["2".into(), "503".into()],
                draft()
            )
            .await
            .is_err()
        );
        sqlx::query("UPDATE media SET kind='video',extension='mp4' WHERE id=2")
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("INSERT INTO video_metadata(media_id,title,duration_ms,codec) VALUES(2,'Original title',1234,'H.264')").execute(db.pool()).await.unwrap();
        assert!(
            save(db.pool(), Some(root.clone()), vec!["2".into()], draft())
                .await
                .is_err()
        );
        let mut changed = draft();
        changed.characters.clear();
        changed.video_title = "New title".into();
        save(db.pool(), Some(root.clone()), vec!["2".into()], changed)
            .await
            .unwrap();
        let row=sqlx::query("SELECT m.favorite,v.title,v.duration_ms,v.codec,m.original_filename,m.relative_path FROM media m JOIN video_metadata v ON v.media_id=m.id WHERE m.id=2").fetch_one(db.pool()).await.unwrap();
        assert_eq!(row.get::<i64, _>("favorite"), 1);
        assert_eq!(row.get::<String, _>("title"), "New title");
        assert_eq!(row.get::<i64, _>("duration_ms"), 1234);
        assert_eq!(row.get::<String, _>("codec"), "H.264");
        assert_eq!(row.get::<String, _>("original_filename"), "Image 2.png");
        assert_eq!(row.get::<String, _>("relative_path"), "nested/Image 2.png");
        db.close().await;
        let reopened = Database::open(&temp.path().join("vault.db")).await.unwrap();
        let mut f = filters();
        f.scope = "favorites".into();
        let page = browse(reopened.pool(), Some(root), f).await.unwrap();
        assert_eq!(page.files[0].video_title.as_deref(), Some("New title"));
        assert!(page.files[0].character_copyrights.is_empty());
        reopened.close().await;
    }
    #[test]
    fn filename_sort_is_natural_unicode_insensitive_and_ignores_folders() {
        assert!(compare_names("Z/Image 2.png", "A/Image 10.png").is_lt());
        assert!(
            compare_names(
                "Image 99999999999999999999.png",
                "Image 100000000000000000000.png"
            )
            .is_lt()
        );
        assert!(compare_names("Straße 02.png", "STRASSE 2.png").is_eq());
    }

    #[tokio::test]
    async fn distribution_saves_logical_membership_without_moving_and_supports_main_edits() {
        let (temp, db, root) = fixture().await;
        let staging = temp.path().join("Stage");
        std::fs::create_dir(&staging).unwrap();
        let original = staging.join("original.png");
        std::fs::write(&original, b"untouched staged content").unwrap();
        let staging = staging.to_string_lossy().into_owned();
        sqlx::query(
            "INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(99,'stage',?,?,1)",
        )
        .bind(&staging)
        .bind(&staging)
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query("INSERT INTO media(id,source_id,relative_path,path_key,original_filename,kind,extension,last_seen_at,created_at,updated_at) VALUES(1000,99,'original.png','original.png','original.png','image','png',1,1,1)").execute(db.pool()).await.unwrap();
        let mut changed = draft();
        changed.copyrights = vec!["Zebra".into(), "Alpha".into()];
        changed.characters = vec![
            Character {
                name: "Zed".into(),
                copyright: "Zebra".into(),
            },
            Character {
                name: "Alice".into(),
                copyright: "Alpha".into(),
            },
        ];
        changed.tags = vec!["Zed".into(), "Alpha".into()];
        save_mode(
            db.pool(),
            Some(root.clone()),
            Some(Some(staging.clone())),
            vec!["1000".into()],
            changed,
        )
        .await
        .unwrap();
        let mut query = filters();
        query.search = "Straße".into();
        let page = browse(db.pool(), Some(root.clone()), query).await.unwrap();
        let file = page
            .files
            .iter()
            .find(|f| f.technical.id == "1000")
            .unwrap();
        assert_eq!(file.copyrights, ["Alpha", "Zebra"]);
        assert_eq!(file.tags, ["Alpha", "Zed"]);
        assert_eq!(file.character_copyrights[0].name, "Alice");
        assert_eq!(file.character_copyrights[1].name, "Zed");
        assert_eq!(
            serde_json::to_value(&file.technical).unwrap()["filename"],
            "original.png"
        );
        sqlx::query("UPDATE media SET stage_search_key='' WHERE id IN (503,1000)")
            .execute(db.pool())
            .await
            .unwrap();
        let staged = stage::browse(db.pool(), Some(staging), None, "")
            .await
            .unwrap();
        assert_eq!(staged.total, 1); // Only the existing redistribution record remains.
        let mut edit = draft();
        edit.characters.clear();
        save(db.pool(), Some(root), vec!["1000".into()], edit)
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(original).unwrap(),
            b"untouched staged content"
        );
        let row = sqlx::query(
            "SELECT source_id,relative_path,original_filename FROM media WHERE id=1000",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("source_id"), 99);
        assert_eq!(row.get::<String, _>("relative_path"), "original.png");
        assert_eq!(row.get::<String, _>("original_filename"), "original.png");
        db.close().await;
    }
}
