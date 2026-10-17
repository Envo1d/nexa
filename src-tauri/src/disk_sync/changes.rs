//! Bounded database-only invalidation before expensive planning and filesystem validation.

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::config::AppConfig;

pub(super) struct Inputs {
    settings: AppConfig,
    fingerprint: [u8; 32],
}

impl PartialEq for Inputs {
    fn eq(&self, other: &Self) -> bool {
        self.settings.same_disk_structure(&other.settings) && self.fingerprint == other.fingerprint
    }
}

impl Inputs {
    pub(super) fn requires_sync(
        current: &Option<Self>,
        previous: &Option<Self>,
        forced: bool,
    ) -> bool {
        forced || (current.is_some() && current != previous)
    }

    pub(super) async fn capture(
        pool: &SqlitePool,
        settings: &AppConfig,
    ) -> Result<Option<Self>, String> {
        if !settings.sync_structure {
            return Ok(None);
        }

        let root = settings.library_path.as_ref().map(|path| {
            path.components()
                .collect::<std::path::PathBuf>()
                .to_string_lossy()
                .into_owned()
        });

        let mut hash = Sha256::new();

        let mut cursor = 0;

        loop {
            // Match planning eligibility. Exclude scan markers, revisions, technical fields,
            // favorites and tags: none of them determine a destination or group membership.
            // JSON arrays preserve field boundaries even when names contain punctuation.
            let rows: Vec<(i64, String)> = sqlx::query_as(concat!(
                "SELECT m.id,json_array(m.id,m.kind,m.extension,",
                "CASE WHEN m.kind<>'video' AND NOT EXISTS ",
                "(SELECT 1 FROM media_characters WHERE media_id=m.id) ",
                "THEN m.original_filename END,",
                "s.root_path,m.relative_path,a.name,",
                "CASE WHEN m.kind='video' THEN v.title END,m.naming_order_key,m.naming_order,",
                "(SELECT json_group_array(name) FROM (SELECT c.name FROM media_characters l ",
                "JOIN characters c ON c.id=l.character_id WHERE l.media_id=m.id ",
                "ORDER BY c.name_key,c.id)),",
                "CASE WHEN ? THEN (SELECT c.name FROM media_copyrights l ",
                "JOIN copyrights c ON c.id=l.copyright_id WHERE l.media_id=m.id ",
                "ORDER BY c.name_key,c.id LIMIT 1) END) ",
                "FROM media m JOIN sources s ON s.id=m.source_id ",
                "JOIN artists a ON a.id=m.artist_id ",
                "LEFT JOIN video_metadata v ON v.media_id=m.id ",
                "WHERE m.id>? AND m.availability='present' AND m.needs_distribution=0 ",
                "AND (m.library_assignment=? OR (s.kind='library' AND s.path_key=?)) ",
                "AND (m.kind<>'video' OR trim(coalesce(v.title,''))<>'') ",
                "ORDER BY m.id LIMIT 256",
            ))
            .bind(settings.copyright_directory)
            .bind(cursor)
            .bind(&root)
            .bind(&root)
            .fetch_all(pool)
            .await
            .map_err(|error| error.to_string())?;

            if rows.is_empty() {
                break;
            }

            for (id, row) in rows {
                cursor = id;
                hash.update((row.len() as u64).to_le_bytes());
                hash.update(row.as_bytes());
            }
        }

        Ok(Some(Self {
            settings: settings.clone(),
            fingerprint: hash.finalize().into(),
        }))
    }
}
