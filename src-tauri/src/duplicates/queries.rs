use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use super::File;
use crate::stage;

pub(super) const SELECT: &str = concat!(
    "SELECT m.*,s.root_path,s.kind source_kind,v.duration_ms,v.fps_num,v.fps_den,",
    "v.codec,v.bitrate_bps,v.title,a.name artist_name,",
    "(SELECT json_group_array(name) FROM (SELECT d.name FROM media_copyrights l ",
    "JOIN copyrights d ON d.id=l.copyright_id WHERE l.media_id=m.id ORDER BY d.name_key)) copyrights,",
    "(SELECT json_group_array(name) FROM (SELECT d.name || ' (' || c.name || ')' name FROM media_characters l ",
    "JOIN characters d ON d.id=l.character_id JOIN copyrights c ON c.id=d.copyright_id ",
    "WHERE l.media_id=m.id ORDER BY d.name_key)) characters,",
    "(SELECT json_group_array(name) FROM (SELECT d.name FROM media_tags l ",
    "JOIN tags d ON d.id=l.tag_id WHERE l.media_id=m.id ORDER BY d.name_key)) tags ",
    "FROM media m JOIN sources s ON s.id=m.source_id LEFT JOIN video_metadata v ON v.media_id=m.id ",
    "LEFT JOIN artists a ON a.id=m.artist_id",
);

pub(super) fn file(row: SqliteRow, main: bool) -> Result<File, String> {
    Ok(File {
        main,
        artist: row
            .get::<Option<String>, _>("artist_name")
            .unwrap_or_default(),
        copyrights: serde_json::from_str(&row.get::<String, _>("copyrights"))
            .map_err(|error| error.to_string())?,
        characters: serde_json::from_str(&row.get::<String, _>("characters"))
            .map_err(|error| error.to_string())?,
        tags: serde_json::from_str(&row.get::<String, _>("tags"))
            .map_err(|error| error.to_string())?,
        video_title: row.get("title"),
        technical: stage::stage_file(row),
        sha256: String::new(),
        visual: None,
    })
}
