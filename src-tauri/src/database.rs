//! Native SQLite foundation. Domain repositories use the pool and transactions;
//! arbitrary SQL is never exposed to the webview.
use sqlx::{
    ConnectOptions, Sqlite, SqlitePool, Transaction,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{path::Path, time::Duration};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("Database path must be absolute and have a filename")]
    InvalidPath,
    #[error("Cannot create database directory: {0}")]
    Directory(#[source] std::io::Error),
    #[error("Cannot open SQLite database: {0}")]
    Connect(#[source] sqlx::Error),
    #[error("Cannot inspect database schema: {0}")]
    Inspect(#[source] sqlx::Error),
    #[error("Existing database has an unrecognized schema; refusing to initialize it")]
    UnmanagedSchema,
    #[error("Cannot apply database migrations: {0}")]
    Migration(#[source] sqlx::migrate::MigrateError),
}

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create/open the isolated database and validate/apply embedded migrations.
    /// Startup owns reporting returned failures, avoiding duplicate error events.
    #[tracing::instrument(skip_all, fields(database_path = %path.display()))]
    pub async fn open(path: &Path) -> Result<Self, DatabaseError> {
        Self::open_with_migrator(path, &MIGRATOR).await
    }

    async fn open_with_migrator(path: &Path, migrator: &Migrator) -> Result<Self, DatabaseError> {
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(DatabaseError::InvalidPath);
        }
        let parent = path.parent().ok_or(DatabaseError::InvalidPath)?;
        tracing::info!("Opening SQLite database");
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(DatabaseError::Directory)?;
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5))
            // SQL and bound values can contain personal media metadata.
            .disable_statement_logging();
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(options)
            .await
            .map_err(DatabaseError::Connect)?;

        if let Err(error) = Self::initialize(&pool, migrator).await {
            pool.close().await;
            return Err(error);
        }
        tracing::info!(max_connections = 4, "SQLite database ready");
        Ok(Self { pool })
    }

    async fn initialize(pool: &SqlitePool, migrator: &Migrator) -> Result<(), DatabaseError> {
        let (tables, tracked): (i64, i64) = sqlx::query_as(
            "SELECT count(*), coalesce(sum(name = '_sqlx_migrations'), 0)
             FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_one(pool)
        .await
        .map_err(DatabaseError::Inspect)?;
        if tables > 0 && tracked == 0 {
            return Err(DatabaseError::UnmanagedSchema);
        }
        tracing::info!(
            embedded_migrations = migrator.iter().count(),
            "Validating and applying database migrations"
        );
        migrator.run(pool).await.map_err(DatabaseError::Migration)?;
        let version: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(version), 0) FROM _sqlx_migrations WHERE success = 1",
        )
        .fetch_one(pool)
        .await
        .map_err(DatabaseError::Inspect)?;
        tracing::info!(schema_version = version, "Database migrations completed");
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Dropping an uncommitted transaction rolls it back through SQLx.
    pub async fn begin(&self) -> Result<Transaction<'static, Sqlite>, sqlx::Error> {
        self.pool.begin().await
    }

    #[tracing::instrument(skip_all)]
    pub async fn close(&self) {
        tracing::info!("Closing SQLite database");
        self.pool.close().await;
        tracing::info!("SQLite database closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    async fn test_database() -> (tempfile::TempDir, Database) {
        let root = tempfile::tempdir().unwrap();
        let database = Database::open(&root.path().join("database/vault.db"))
            .await
            .unwrap();
        (root, database)
    }

    async fn seed(pool: &SqlitePool) {
        sqlx::raw_sql(
            "INSERT INTO sources(id, kind, root_path, path_key, created_at)
                 VALUES (1, 'library', '/library', '/library', 100);
             INSERT INTO copyrights(id, name, name_key) VALUES (1, 'World A', 'world a'), (2, 'World B', 'world b');
             INSERT INTO artists(id, name, name_key) VALUES (1, 'Artist A', 'artist a'), (2, 'Artist B', 'artist b');
             INSERT INTO tags(id, name, name_key) VALUES (1, 'Tag', 'tag');
             INSERT INTO characters(id, copyright_id, name, name_key)
                 VALUES (1, 1, 'Alice', 'alice'), (2, 2, 'Alice', 'alice');
             INSERT INTO media(id, source_id, artist_id, relative_path, original_filename, path_key, kind,
                               extension, last_seen_at, created_at, updated_at)
                 VALUES (1, 1, 1, 'image.png', 'image.png', 'image.png', 'image', 'png', 100, 100, 100),
                        (2, 1, 1, 'video.mp4', 'video.mp4', 'video.mp4', 'video', 'mp4', 100, 100, 100);
             INSERT INTO media_copyrights(media_id, copyright_id) VALUES (1, 1), (2, 1);
             INSERT INTO media_tags(media_id, tag_id) VALUES (1, 1);
             INSERT INTO media_characters(media_id, character_id, copyright_id) VALUES (1, 1, 1);
             INSERT INTO video_metadata(media_id, title, duration_ms, fps_num, fps_den)
                 VALUES (2, 'Video', 0, 30000, 1001);",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    async fn count(pool: &SqlitePool, statement: &'static str) -> i64 {
        sqlx::query_scalar(statement).fetch_one(pool).await.unwrap()
    }

    fn migration_directory(root: &Path) -> std::path::PathBuf {
        let directory = root.join("migrations");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(
            directory.join("0001_init.sql"),
            include_str!("../migrations/0001_init.sql"),
        )
        .unwrap();
        std::fs::write(
            directory.join("0002_redistribution.sql"),
            include_str!("../migrations/0002_redistribution.sql"),
        )
        .unwrap();
        std::fs::write(
            directory.join("0003_stage_scan.sql"),
            include_str!("../migrations/0003_stage_scan.sql"),
        )
        .unwrap();
        directory
    }

    #[tokio::test]
    async fn upgrades_version_one_without_changing_media_identity_or_metadata() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("vault.db");
        let directory = root.path().join("version_one");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(
            directory.join("0001_init.sql"),
            include_str!("../migrations/0001_init.sql"),
        )
        .unwrap();
        let migrator = Migrator::new(directory.as_path()).await.unwrap();
        let database = Database::open_with_migrator(&path, &migrator)
            .await
            .unwrap();
        seed(database.pool()).await;
        database.close().await;
        let database = Database::open(&path).await.unwrap();
        assert_eq!(
            count(
                database.pool(),
                "SELECT count(*) FROM media WHERE needs_distribution = 0 AND artist_id = 1"
            )
            .await,
            2
        );
        assert_eq!(
            count(database.pool(), "SELECT count(*) FROM media_characters").await,
            1
        );
        assert_eq!(
            count(database.pool(), "SELECT count(*) FROM media_tags").await,
            1
        );
        let filename: String =
            sqlx::query_scalar("SELECT original_filename FROM media WHERE id = 1")
                .fetch_one(database.pool())
                .await
                .unwrap();
        assert_eq!(filename, "image.png");
        database.close().await;
    }

    #[tokio::test]
    async fn initializes_ten_tables_and_preserves_data_on_reopen() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("данные 日本/database/vault.db");
        let database = Database::open(&path).await.unwrap();
        assert!(path.is_file());
        assert_eq!(
            count(database.pool(), "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations'").await,
            10
        );
        assert_eq!(
            count(
                database.pool(),
                "SELECT count(*) FROM _sqlx_migrations WHERE version = 1 AND success = 1"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                database.pool(),
                "SELECT count(*) FROM pragma_table_info('media_tags') WHERE name = 'position'"
            )
            .await,
            0
        );
        seed(database.pool()).await;
        sqlx::query("UPDATE media SET favorite = 1, availability = 'missing' WHERE id = 1")
            .execute(database.pool())
            .await
            .unwrap();
        database.close().await;
        assert!(database.pool().is_closed());

        let database = Database::open(&path).await.unwrap();
        assert_eq!(
            count(database.pool(), "SELECT count(*) FROM _sqlx_migrations").await,
            3
        );
        assert_eq!(
            count(
                database.pool(),
                "SELECT count(*) FROM media WHERE favorite = 1 AND availability = 'missing'"
            )
            .await,
            1
        );
        assert_eq!(
            count(database.pool(), "SELECT count(*) FROM media_characters").await,
            1
        );
        assert_eq!(
            count(
                database.pool(),
                "SELECT count(*) FROM pragma_foreign_key_check"
            )
            .await,
            0
        );
        database.close().await;
    }

    #[tokio::test]
    async fn configures_every_pool_connection() {
        let (_root, database) = test_database().await;
        let mut connections = Vec::new();
        for _ in 0..4 {
            connections.push(database.pool().acquire().await.unwrap());
        }
        for connection in &mut connections {
            let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
                .fetch_one(&mut **connection)
                .await
                .unwrap();
            assert_eq!(mode, "wal");
            for (pragma, expected) in [
                ("PRAGMA foreign_keys", 1_i64),
                ("PRAGMA synchronous", 2),
                ("PRAGMA busy_timeout", 5000),
            ] {
                let actual: i64 = sqlx::query_scalar(pragma)
                    .fetch_one(&mut **connection)
                    .await
                    .unwrap();
                assert_eq!(actual, expected);
            }
        }
        drop(connections);
        database.close().await;
    }

    #[tokio::test]
    async fn enforces_qualified_characters_and_cascades_removed_file_copyright() {
        let (_root, database) = test_database().await;
        seed(database.pool()).await;
        // Wrong copyright for this character, despite a selected file copyright.
        assert!(
            sqlx::query("INSERT INTO media_characters VALUES (2, 2, 1)")
                .execute(database.pool())
                .await
                .is_err()
        );
        // Correct character copyright, but not yet selected on this file.
        assert!(
            sqlx::query("INSERT INTO media_characters VALUES (2, 2, 2)")
                .execute(database.pool())
                .await
                .is_err()
        );
        sqlx::query("INSERT INTO media_copyrights VALUES (2, 2)")
            .execute(database.pool())
            .await
            .unwrap();
        sqlx::query("INSERT INTO media_characters VALUES (2, 2, 2)")
            .execute(database.pool())
            .await
            .unwrap();
        sqlx::query("DELETE FROM media_copyrights WHERE media_id = 2 AND copyright_id = 2")
            .execute(database.pool())
            .await
            .unwrap();
        assert_eq!(
            count(
                database.pool(),
                "SELECT count(*) FROM media_characters WHERE media_id = 2"
            )
            .await,
            0
        );
        assert_eq!(
            count(database.pool(), "SELECT count(*) FROM characters").await,
            2
        );
        database.close().await;
    }

    #[tokio::test]
    async fn permits_no_characters_and_single_artist_and_preserves_original_filename() {
        let (_root, database) = test_database().await;
        seed(database.pool()).await;
        sqlx::query("DELETE FROM media_characters WHERE media_id = 1")
            .execute(database.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE media SET artist_id = 2, relative_path = 'renamed.png', path_key = 'renamed.png' WHERE id = 1")
            .execute(database.pool()).await.unwrap();
        let row = sqlx::query("SELECT artist_id, original_filename FROM media WHERE id = 1")
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(row.get::<i64, _>("artist_id"), 2);
        assert_eq!(row.get::<String, _>("original_filename"), "image.png");
        assert_eq!(
            count(
                database.pool(),
                "SELECT count(*) FROM media_characters WHERE media_id = 1"
            )
            .await,
            0
        );
        // Discovery allows an unknown artist and two locations with identical metadata.
        sqlx::query("INSERT INTO media(source_id, relative_path, original_filename, path_key, kind, extension, last_seen_at, created_at, updated_at) VALUES (1, 'copy.png', 'image.png', 'copy.png', 'image', 'png', 100, 100, 100)")
            .execute(database.pool()).await.unwrap();
        assert!(
            sqlx::query("UPDATE media SET artist_id = 999 WHERE id = 1")
                .execute(database.pool())
                .await
                .is_err()
        );
        database.close().await;
    }

    #[tokio::test]
    async fn rejects_duplicate_links_paths_and_invalid_values() {
        let (_root, database) = test_database().await;
        seed(database.pool()).await;
        for statement in [
            "INSERT INTO media_tags VALUES (1, 1)",
            "INSERT INTO media_copyrights VALUES (1, 1)",
            "INSERT INTO media_characters VALUES (1, 1, 1)",
            "UPDATE media SET path_key = 'image.png' WHERE id = 2",
            "UPDATE media SET favorite = 2 WHERE id = 1",
            "UPDATE media SET size_bytes = -1 WHERE id = 1",
            "UPDATE media SET width = 0 WHERE id = 1",
            "UPDATE media SET kind = 'audio' WHERE id = 1",
            "UPDATE media SET metadata_state = 'unknown' WHERE id = 1",
            "UPDATE media SET original_filename = '../escape.png' WHERE id = 1",
            "UPDATE tags SET name = ' ' WHERE id = 1",
            "INSERT INTO tags(name, name_key) VALUES ('TAG', 'tag')",
            "INSERT INTO characters(copyright_id, name, name_key) VALUES (1, 'ALICE', 'alice')",
            "INSERT INTO sources(kind, root_path, path_key, created_at) VALUES ('library', '/other', '/other', 100)",
        ] {
            assert!(
                sqlx::query(statement)
                    .execute(database.pool())
                    .await
                    .is_err(),
                "accepted {statement}"
            );
        }
        database.close().await;
    }

    #[tokio::test]
    async fn restricts_parent_deletion_and_removes_only_file_dependents() {
        let (_root, database) = test_database().await;
        seed(database.pool()).await;
        for statement in [
            "DELETE FROM sources WHERE id = 1",
            "DELETE FROM artists WHERE id = 1",
            "DELETE FROM copyrights WHERE id = 1",
            "DELETE FROM characters WHERE id = 1",
            "DELETE FROM tags WHERE id = 1",
            "UPDATE characters SET copyright_id = 2 WHERE id = 1",
        ] {
            assert!(
                sqlx::query(statement)
                    .execute(database.pool())
                    .await
                    .is_err()
            );
        }
        sqlx::query("UPDATE sources SET is_active = 0 WHERE id = 1")
            .execute(database.pool())
            .await
            .unwrap();
        assert_eq!(
            count(database.pool(), "SELECT count(*) FROM media").await,
            2
        );
        sqlx::query("DELETE FROM media WHERE id IN (1, 2)")
            .execute(database.pool())
            .await
            .unwrap();
        for statement in [
            "SELECT count(*) FROM media_tags",
            "SELECT count(*) FROM media_copyrights",
            "SELECT count(*) FROM media_characters",
            "SELECT count(*) FROM video_metadata",
        ] {
            assert_eq!(count(database.pool(), statement).await, 0);
        }
        assert_eq!(
            count(database.pool(), "SELECT count(*) FROM artists").await,
            2
        );
        assert_eq!(count(database.pool(), "SELECT count(*) FROM tags").await, 1);
        database.close().await;
    }

    #[tokio::test]
    async fn enforces_video_only_metadata_and_paired_fps() {
        let (_root, database) = test_database().await;
        seed(database.pool()).await;
        for statement in [
            "INSERT INTO video_metadata(media_id) VALUES (1)",
            "UPDATE video_metadata SET media_id = 1 WHERE media_id = 2",
            "UPDATE media SET kind = 'image' WHERE id = 2",
            "UPDATE video_metadata SET fps_den = NULL WHERE media_id = 2",
            "UPDATE video_metadata SET fps_den = 0 WHERE media_id = 2",
            "UPDATE video_metadata SET duration_ms = -1 WHERE media_id = 2",
            "UPDATE video_metadata SET title = '' WHERE media_id = 2",
        ] {
            assert!(
                sqlx::query(statement)
                    .execute(database.pool())
                    .await
                    .is_err()
            );
        }
        sqlx::query("UPDATE video_metadata SET fps_num = NULL, fps_den = NULL, title = NULL WHERE media_id = 2")
            .execute(database.pool()).await.unwrap();
        assert_eq!(
            count(
                database.pool(),
                "SELECT count(*) FROM video_metadata WHERE duration_ms = 0 AND title IS NULL"
            )
            .await,
            1
        );
        database.close().await;
    }

    #[tokio::test]
    async fn commits_and_rolls_back_transactions() {
        let (_root, database) = test_database().await;
        let mut transaction = database.begin().await.unwrap();
        sqlx::query("INSERT INTO tags(name, name_key) VALUES ('Keep', 'keep')")
            .execute(&mut *transaction)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        let mut transaction = database.begin().await.unwrap();
        sqlx::query("INSERT INTO tags(name, name_key) VALUES ('Discard', 'discard')")
            .execute(&mut *transaction)
            .await
            .unwrap();
        transaction.rollback().await.unwrap();
        assert_eq!(count(database.pool(), "SELECT count(*) FROM tags").await, 1);
        let mut transaction = database.begin().await.unwrap();
        sqlx::query("INSERT INTO tags(name, name_key) VALUES ('Dropped', 'dropped')")
            .execute(&mut *transaction)
            .await
            .unwrap();
        drop(transaction);
        assert_eq!(count(database.pool(), "SELECT count(*) FROM tags").await, 1);
        database.close().await;
    }

    #[tokio::test]
    async fn applies_pending_migrations_and_rejects_missing_history() {
        let (root, database) = test_database().await;
        let path = root.path().join("database/vault.db");
        seed(database.pool()).await;
        database.close().await;
        let directory = migration_directory(root.path());
        std::fs::write(
            directory.join("0004_upgrade.sql"),
            "CREATE TABLE upgrade_marker(id INTEGER PRIMARY KEY) STRICT;",
        )
        .unwrap();
        let migrator = Migrator::new(directory.as_path()).await.unwrap();
        let database = Database::open_with_migrator(&path, &migrator)
            .await
            .unwrap();
        assert_eq!(
            count(
                database.pool(),
                "SELECT count(*) FROM _sqlx_migrations WHERE success = 1"
            )
            .await,
            4
        );
        assert_eq!(
            count(database.pool(), "SELECT count(*) FROM media").await,
            2
        );
        database.close().await;
        assert!(matches!(
            Database::open(&path).await,
            Err(DatabaseError::Migration(_))
        ));
    }

    #[tokio::test]
    async fn failed_migration_rolls_back_and_preserves_existing_records() {
        let (root, database) = test_database().await;
        let path = root.path().join("database/vault.db");
        seed(database.pool()).await;
        database.close().await;
        let directory = migration_directory(root.path());
        std::fs::write(
            directory.join("0004_broken.sql"),
            "CREATE TABLE rollback_marker(id INTEGER PRIMARY KEY); DELETE FROM media; INVALID SQL;",
        )
        .unwrap();
        let migrator = Migrator::new(directory.as_path()).await.unwrap();
        assert!(matches!(
            Database::open_with_migrator(&path, &migrator).await,
            Err(DatabaseError::Migration(_))
        ));
        let database = Database::open(&path).await.unwrap();
        assert_eq!(
            count(database.pool(), "SELECT count(*) FROM media").await,
            2
        );
        assert_eq!(
            count(database.pool(), "SELECT count(*) FROM _sqlx_migrations").await,
            3
        );
        assert_eq!(
            count(
                database.pool(),
                "SELECT count(*) FROM sqlite_schema WHERE name = 'rollback_marker'"
            )
            .await,
            0
        );
        database.close().await;
    }

    #[tokio::test]
    async fn rejects_modified_applied_migration_without_resetting_data() {
        let (root, database) = test_database().await;
        let path = root.path().join("database/vault.db");
        seed(database.pool()).await;
        database.close().await;
        let directory = migration_directory(root.path());
        std::fs::write(
            directory.join("0001_init.sql"),
            "CREATE TABLE replacement(id INTEGER PRIMARY KEY);",
        )
        .unwrap();
        let migrator = Migrator::new(directory.as_path()).await.unwrap();
        assert!(matches!(
            Database::open_with_migrator(&path, &migrator).await,
            Err(DatabaseError::Migration(_))
        ));
        let database = Database::open(&path).await.unwrap();
        assert_eq!(
            count(database.pool(), "SELECT count(*) FROM media").await,
            2
        );
        database.close().await;
    }

    #[tokio::test]
    async fn rejects_corrupt_unmanaged_and_unwritable_databases() {
        let root = tempfile::tempdir().unwrap();
        let corrupt = root.path().join("corrupt.db");
        let bytes = b"this is not a SQLite database";
        std::fs::write(&corrupt, bytes).unwrap();
        assert!(Database::open(&corrupt).await.is_err());
        assert_eq!(std::fs::read(&corrupt).unwrap(), bytes);

        let unmanaged = root.path().join("unmanaged.db");
        let options = SqliteConnectOptions::new()
            .filename(&unmanaged)
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        sqlx::raw_sql("CREATE TABLE unrelated(value TEXT); INSERT INTO unrelated VALUES ('keep');")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        assert!(matches!(
            Database::open(&unmanaged).await,
            Err(DatabaseError::UnmanagedSchema)
        ));
        let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&unmanaged))
            .await
            .unwrap();
        assert_eq!(
            count(&pool, "SELECT count(*) FROM unrelated WHERE value = 'keep'").await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM sqlite_schema WHERE name = '_sqlx_migrations'"
            )
            .await,
            0
        );
        pool.close().await;

        let blocker = root.path().join("file-not-directory");
        std::fs::write(&blocker, b"keep").unwrap();
        assert!(matches!(
            Database::open(&blocker.join("vault.db")).await,
            Err(DatabaseError::Directory(_))
        ));
        assert_eq!(std::fs::read(&blocker).unwrap(), b"keep");
        assert!(matches!(
            Database::open(Path::new("relative.db")).await,
            Err(DatabaseError::InvalidPath)
        ));
    }
}
