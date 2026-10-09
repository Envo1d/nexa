//! Taxonomy changes are database-only; this module never writes media files.
use crate::{database::Database, legacy_import::name_key};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Sqlite, SqlitePool};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Copyright,
    Artist,
    Tag,
}
impl Category {
    fn table(self) -> &'static str {
        match self {
            Self::Copyright => "copyrights",
            Self::Artist => "artists",
            Self::Tag => "tags",
        }
    }
    fn relation(self) -> &'static str {
        match self {
            Self::Copyright => {
                "m.id IN (SELECT media_id FROM media_copyrights WHERE copyright_id = d.id)"
            }
            Self::Artist => "m.artist_id = d.id",
            Self::Tag => "m.id IN (SELECT media_id FROM media_tags WHERE tag_id = d.id)",
        }
    }
}
#[derive(Serialize, FromRow)]
pub struct Entry {
    id: String,
    name: String,
    count: i64,
}
#[derive(Serialize, Default)]
pub struct Catalog {
    copyrights: Vec<Entry>,
    artists: Vec<Entry>,
    tags: Vec<Entry>,
}

async fn entries(pool: &SqlitePool, category: Category) -> Result<Vec<Entry>, sqlx::Error> {
    QueryBuilder::<Sqlite>::new(
        "SELECT CAST(d.id AS TEXT) id, d.name, (SELECT count(*) FROM media m WHERE ",
    )
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
    database: tauri::State<'_, Database>,
    category: Category,
    id: i64,
    name: String,
) -> Result<(), String> {
    tracing::info!("Renaming dictionary entry");
    rename(database.pool(), category, id, &name).await?;
    tracing::info!("Dictionary entry renamed; disk structure preserved");
    Ok(())
}
async fn remove(pool: &SqlitePool, category: Category, id: i64) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let queued = match category {
        Category::Copyright => {
            let count = sqlx::query("UPDATE media SET needs_distribution = 1, updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER) WHERE id IN (SELECT media_id FROM media_copyrights WHERE copyright_id = ?)")
                .bind(id).execute(&mut *tx).await?.rows_affected();
            // Deleting the copyright links cascades only the corresponding character links.
            sqlx::query("DELETE FROM media_copyrights WHERE copyright_id = ?").bind(id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM characters WHERE copyright_id = ?").bind(id).execute(&mut *tx).await?;
            count
        }
        Category::Artist => {
            sqlx::query("UPDATE media SET needs_distribution = 1, artist_id = NULL, updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER) WHERE artist_id = ?")
                .bind(id).execute(&mut *tx).await?.rows_affected()
        }
        Category::Tag => {
            sqlx::query("DELETE FROM media_tags WHERE tag_id = ?").bind(id).execute(&mut *tx).await?;
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
    database: tauri::State<'_, Database>,
    category: Category,
    id: i64,
) -> Result<u64, String> {
    tracing::info!("Deleting dictionary entry");
    let count = remove(database.pool(), category, id)
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!(
        affected_files = count,
        "Dictionary entry deleted; redistribution queued without disk changes"
    );
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
    sqlx::query_as("SELECT CAST(m.id AS TEXT) id, s.root_path root, m.relative_path, m.kind, m.extension, m.size_bytes size, m.width, m.height, m.file_modified_at modified_at, m.file_created_at created_at, v.duration_ms duration FROM media m JOIN sources s ON s.id = m.source_id LEFT JOIN video_metadata v ON v.media_id = m.id WHERE m.needs_distribution = 1 ORDER BY m.id")
        .fetch_all(database.pool()).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture() -> (tempfile::TempDir, Database) {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(&root.path().join("vault.db")).await.unwrap();
        std::fs::write(root.path().join("image.png"), b"unchanged media bytes").unwrap();
        sqlx::raw_sql("INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(1,'library','/library','/library',1); INSERT INTO copyrights(id,name,name_key) VALUES(1,'World','world'),(2,'Other','other'); INSERT INTO artists(id,name,name_key) VALUES(1,'Artist','artist'); INSERT INTO tags(id,name,name_key) VALUES(1,'Tag','tag'),(2,'Other','other'); INSERT INTO characters(id,copyright_id,name,name_key) VALUES(1,1,'Alice','alice'),(2,2,'Alice','alice'); INSERT INTO media(id,source_id,artist_id,relative_path,original_filename,path_key,kind,extension,last_seen_at,created_at,updated_at) VALUES(1,1,1,'image.png','image.png','image.png','image','png',1,1,1); INSERT INTO media_copyrights VALUES(1,1),(1,2); INSERT INTO media_characters VALUES(1,1,1),(1,2,2); INSERT INTO media_tags VALUES(1,1);")
            .execute(db.pool()).await.unwrap();
        (root, db)
    }
    #[tokio::test]
    async fn copyright_deletion_queues_without_changing_path_or_other_relations() {
        let (root, db) = fixture().await;
        assert_eq!(remove(db.pool(), Category::Copyright, 1).await.unwrap(), 1);
        let row: (String, i64, Option<i64>) =
            sqlx::query_as("SELECT relative_path,needs_distribution,artist_id FROM media")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(row, ("image.png".into(), 1, Some(1)));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM media_characters WHERE copyright_id = 2"
            )
            .fetch_one(db.pool())
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pragma_foreign_key_check")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            0
        );
        db.close().await;
        let reopened = Database::open(&root.path().join("vault.db")).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT needs_distribution FROM media")
                .fetch_one(reopened.pool())
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            std::fs::read(root.path().join("image.png")).unwrap(),
            b"unchanged media bytes"
        );
    }
    #[tokio::test]
    async fn artist_deletion_clears_attribution_and_tag_deletion_does_not_queue() {
        let (_root, db) = fixture().await;
        assert_eq!(remove(db.pool(), Category::Tag, 1).await.unwrap(), 0);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT needs_distribution FROM media")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            0
        );
        assert_eq!(remove(db.pool(), Category::Artist, 1).await.unwrap(), 1);
        assert_eq!(
            sqlx::query_scalar::<_, Option<i64>>("SELECT artist_id FROM media")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            None
        );
    }
    #[tokio::test]
    async fn rename_preserves_identity_and_rejects_unicode_duplicates_and_blank_names() {
        let (_root, db) = fixture().await;
        rename(db.pool(), Category::Tag, 1, " Straße ")
            .await
            .unwrap();
        assert!(
            rename(db.pool(), Category::Tag, 2, "STRASSE")
                .await
                .is_err()
        );
        assert!(rename(db.pool(), Category::Tag, 1, " \n ").await.is_err());
        assert!(
            rename(db.pool(), Category::Tag, 999, "Missing")
                .await
                .is_err()
        );
        assert_eq!(
            entries(db.pool(), Category::Tag)
                .await
                .unwrap()
                .iter()
                .find(|e| e.id == "1")
                .unwrap()
                .count,
            1
        );
    }
    #[tokio::test]
    async fn failed_delete_rolls_back_queue_and_links() {
        let (_root, db) = fixture().await;
        sqlx::raw_sql("CREATE TRIGGER reject_delete BEFORE DELETE ON copyrights BEGIN SELECT RAISE(ABORT,'blocked'); END;").execute(db.pool()).await.unwrap();
        assert!(remove(db.pool(), Category::Copyright, 1).await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT needs_distribution FROM media")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM media_characters")
                .fetch_one(db.pool())
                .await
                .unwrap(),
            2
        );
    }
}
