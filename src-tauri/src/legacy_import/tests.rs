use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

use super::*;

struct Fixture {
    _directory: tempfile::TempDir,
    paths: LegacyPaths,
    library: PathBuf,
    stage: PathBuf,
    old: SqlitePool,
    new: Database,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();

        let library = directory.path().join("Медиа");

        let stage = directory.path().join("Входящие");
        std::fs::create_dir_all(&library).unwrap();
        std::fs::create_dir_all(&stage).unwrap();

        let paths = LegacyPaths {
            database: directory.path().join("old.db"),
            settings: directory.path().join("settings.json"),
        };
        std::fs::write(
            &paths.settings,
            serde_json::to_vec(&serde_json::json!({
                "library_path": library, "staging_path": stage,
                "database_path": "unused-field", "folder_mapping": {"copyright_depth": 0}
            }))
            .unwrap(),
        )
        .unwrap();

        let old = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&paths.database)
                .create_if_missing(true)
                .collation("NATURALSORT", |left, right| left.cmp(right)),
        )
        .await
        .unwrap();
        sqlx::raw_sql("CREATE TABLE schema_version(version INTEGER NOT NULL);
                INSERT INTO schema_version VALUES(10);
                CREATE TABLE media(path TEXT PRIMARY KEY, name TEXT NOT NULL,
                    copyright TEXT NOT NULL, artist TEXT NOT NULL, media_type TEXT NOT NULL,
                    modified INTEGER DEFAULT 0, last_seen_scan INTEGER DEFAULT 0,
                    characters TEXT NOT NULL DEFAULT '', tags TEXT NOT NULL DEFAULT '');
                CREATE INDEX idx_media_name ON media(name COLLATE NATURALSORT);
                CREATE TABLE staging(path TEXT PRIMARY KEY, name TEXT NOT NULL,
                    media_type TEXT NOT NULL, modified INTEGER DEFAULT 0, last_seen_scan INTEGER DEFAULT 0);
                CREATE INDEX idx_staging_name ON staging(name COLLATE NATURALSORT);
                CREATE VIRTUAL TABLE media_fts USING fts5(path, name, copyright, artist, characters, tags);
                CREATE TRIGGER trg_media_ai AFTER INSERT ON media BEGIN
                    INSERT INTO media_fts(rowid, path, name, copyright, artist, characters, tags)
                    VALUES(new.rowid, new.path, new.name, new.copyright, new.artist, new.characters, new.tags);
                END;")
            .execute(&old)
            .await
            .unwrap();

        let new = Database::open(&directory.path().join("new/vault.db"))
            .await
            .unwrap();
        Self {
            _directory: directory,
            paths,
            library,
            stage,
            old,
            new,
        }
    }

    async fn media(&self, filename: &str, copyright: &str, characters: &str, tags: &str) {
        sqlx::query(concat!(
            "INSERT INTO media(path,name,copyright,artist,media_type,modified,last_seen_scan,",
            "characters,tags) VALUES(?,?,?,'Artist','Image',123,123000,?,?)",
        ))
        .bind(self.library.join(filename).to_str().unwrap())
        .bind(filename)
        .bind(copyright)
        .bind(characters)
        .bind(tags)
        .execute(&self.old)
        .await
        .unwrap();
    }

    async fn count(&self, statement: &'static str) -> i64 {
        sqlx::query_scalar(statement)
            .fetch_one(self.new.pool())
            .await
            .unwrap()
    }

    async fn close(self) {
        self.old.close().await;
        self.new.close().await;
    }
}

#[test]
fn normalizes_unicode_tag_identity_and_keeps_accents_distinct() {
    assert_eq!(name_key(" STRASSE "), name_key("Straße"));
    assert_eq!(name_key("CAFÉ"), name_key("Cafe\u{301}"));
    assert_eq!(name_key("Σ"), name_key("ς"));
    assert_ne!(name_key("cafe"), name_key("café"));
}

#[tokio::test]
async fn imports_qualified_characters_unique_tags_stage_and_preserves_source() {
    let fixture = Fixture::new().await;
    fixture
        .media(
            "one.png",
            "World A",
            "Alice|ALICE",
            "Tag|tag| TAG |Straße|STRASSE|Cafe\u{301}|CAFÉ",
        )
        .await;
    fixture.media("two.png", "World B", "Alice", "TAG").await;
    std::fs::write(fixture.library.join("one.png"), b"unchanged media").unwrap();
    sqlx::query("INSERT INTO staging VALUES(?, 'incoming.mp4', 'Video', 456, 456000)")
        .bind(fixture.stage.join("incoming.mp4").to_str().unwrap())
        .execute(&fixture.old)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tags(name,name_key) VALUES('Existing TAG','tag')")
        .execute(fixture.new.pool())
        .await
        .unwrap();
    // Close source writer so file bytes can be compared, including FTS/indexes.
    fixture.old.close().await;

    let source_before = std::fs::read(&fixture.paths.database).unwrap();

    let settings_before = std::fs::read(&fixture.paths.settings).unwrap();

    let result = import(&fixture.paths, &fixture.new).await.unwrap();
    assert_eq!(
        (result.library_added, result.stage_added, result.skipped),
        (2, 1, 0)
    );
    assert_eq!(fixture.count("SELECT count(*) FROM characters").await, 2);
    assert_eq!(fixture.count("SELECT count(*) FROM tags").await, 3);
    assert_eq!(fixture.count("SELECT count(*) FROM media_tags").await, 4);
    assert_eq!(
        fixture.count("SELECT count(*) FROM media_characters").await,
        2
    );
    assert_eq!(fixture.count(concat!(
        "SELECT count(*) FROM media_characters mc JOIN characters c ON c.id=mc.character_id ",
        "JOIN copyrights cp ON cp.id=c.copyright_id WHERE cp.id=mc.copyright_id AND ",
        "c.name='Alice'",
    ))
.await, 2);
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM media WHERE availability='missing'")
            .await,
        2
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM media WHERE file_modified_at IN (123000,456000)")
            .await,
        3
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM sources WHERE is_active=1")
            .await,
        0
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM pragma_foreign_key_check")
            .await,
        0
    );
    assert_eq!(
        std::fs::read(&fixture.paths.database).unwrap(),
        source_before
    );
    assert_eq!(
        std::fs::read(&fixture.paths.settings).unwrap(),
        settings_before
    );
    assert_eq!(
        std::fs::read(fixture.library.join("one.png")).unwrap(),
        b"unchanged media"
    );
    // Repeat import must preserve new annotations and stable media identities.
    sqlx::query("UPDATE media SET favorite=1")
        .execute(fixture.new.pool())
        .await
        .unwrap();

    let result = import(&fixture.paths, &fixture.new).await.unwrap();
    assert_eq!(
        (result.library_added, result.stage_added, result.skipped),
        (0, 0, 3)
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM media WHERE favorite=1")
            .await,
        3
    );
    assert_eq!(fixture.count("SELECT count(*) FROM sources").await, 2);
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM tags WHERE name='Existing TAG'")
            .await,
        1
    );
    fixture.close().await;
}

#[tokio::test]
async fn rolls_back_everything_when_character_copyright_is_missing() {
    let fixture = Fixture::new().await;
    fixture.media("one.png", "World", "Alice", "Tag").await;
    fixture.media("two.png", "", "Alice", "Tag").await;
    assert!(matches!(
        import(&fixture.paths, &fixture.new).await,
        Err(ImportError::Record { row: 2, .. })
    ));

    for statement in [
        "SELECT count(*) FROM media",
        "SELECT count(*) FROM sources",
        "SELECT count(*) FROM artists",
        "SELECT count(*) FROM copyrights",
        "SELECT count(*) FROM characters",
        "SELECT count(*) FROM tags",
    ] {
        assert_eq!(fixture.count(statement).await, 0);
    }
    fixture.close().await;
}

#[tokio::test]
async fn rejects_escaping_paths_missing_roots_and_invalid_types_atomically() {
    let fixture = Fixture::new().await;
    fixture.media("one.png", "World", "Alice", "Tag").await;
    fixture
        .media("../escape.png", "World", "Alice", "Tag")
        .await;
    assert!(import(&fixture.paths, &fixture.new).await.is_err());
    assert_eq!(fixture.count("SELECT count(*) FROM media").await, 0);
    sqlx::query("DELETE FROM media WHERE rowid=2")
        .execute(&fixture.old)
        .await
        .unwrap();
    sqlx::query("UPDATE media SET media_type='Other'")
        .execute(&fixture.old)
        .await
        .unwrap();
    assert!(import(&fixture.paths, &fixture.new).await.is_err());
    sqlx::query("UPDATE media SET media_type='Image'")
        .execute(&fixture.old)
        .await
        .unwrap();
    std::fs::write(&fixture.paths.settings, b"{}").unwrap();
    assert!(import(&fixture.paths, &fixture.new).await.is_err());
    assert_eq!(fixture.count("SELECT count(*) FROM tags").await, 0);
    fixture.close().await;
}

#[tokio::test]
async fn imports_batches_without_losing_records() {
    let fixture = Fixture::new().await;

    for index in 0..503 {
        fixture
            .media(&format!("{index}.png"), "World", "Alice", "Tag|TAG")
            .await;
    }

    let result = import(&fixture.paths, &fixture.new).await.unwrap();
    assert_eq!(result.library_added, 503);
    assert_eq!(fixture.count("SELECT count(*) FROM tags").await, 1);
    assert_eq!(fixture.count("SELECT count(*) FROM characters").await, 1);
    assert_eq!(fixture.count("SELECT count(*) FROM media_tags").await, 503);
    fixture.close().await;
}

#[tokio::test]
async fn rejects_unknown_versions_corruption_and_missing_database() {
    let fixture = Fixture::new().await;
    sqlx::query("UPDATE schema_version SET version=11")
        .execute(&fixture.old)
        .await
        .unwrap();
    assert!(matches!(
        import(&fixture.paths, &fixture.new).await,
        Err(ImportError::Schema)
    ));
    sqlx::query("UPDATE schema_version SET version=10; INSERT INTO schema_version VALUES(10)")
        .execute(&fixture.old)
        .await
        .unwrap();
    assert!(matches!(
        import(&fixture.paths, &fixture.new).await,
        Err(ImportError::Schema)
    ));
    fixture.old.close().await;
    std::fs::write(&fixture.paths.database, b"corrupt").unwrap();
    assert!(import(&fixture.paths, &fixture.new).await.is_err());
    assert_eq!(std::fs::read(&fixture.paths.database).unwrap(), b"corrupt");
    std::fs::remove_file(&fixture.paths.database).unwrap();
    assert!(matches!(
        import(&fixture.paths, &fixture.new).await,
        Err(ImportError::Missing)
    ));
    assert!(!fixture.paths.database.exists());
    fixture.close().await;
}

#[tokio::test]
async fn supports_early_schema_and_reuses_existing_active_source() {
    let fixture = Fixture::new().await;
    sqlx::raw_sql(
        "DROP TRIGGER trg_media_ai; DROP TABLE media; DROP TABLE staging;
        UPDATE schema_version SET version=1;
        CREATE TABLE media(path TEXT PRIMARY KEY, name TEXT NOT NULL, category TEXT NOT NULL,
            author TEXT NOT NULL, media_type TEXT NOT NULL);",
    )
    .execute(&fixture.old)
    .await
    .unwrap();
    sqlx::query("INSERT INTO media VALUES(?, 'old.png', 'World', 'Artist', 'Image')")
        .bind(fixture.library.join("old.png").to_str().unwrap())
        .execute(&fixture.old)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sources(kind,root_path,path_key,created_at) VALUES('library',?,?,1)")
        .bind(fixture.library.to_str().unwrap())
        .bind(fixture.library.to_str().unwrap())
        .execute(fixture.new.pool())
        .await
        .unwrap();

    let result = import(&fixture.paths, &fixture.new).await.unwrap();
    assert_eq!(result.library_added, 1);
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM sources WHERE is_active=1")
            .await,
        1
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM media WHERE file_modified_at IS NULL")
            .await,
        1
    );
    fixture.close().await;
}

#[tokio::test]
async fn stage_failure_rolls_back_library_and_preserves_existing_new_data() {
    let fixture = Fixture::new().await;
    fixture.media("one.png", "World", "Alice", "Tag").await;
    sqlx::query("INSERT INTO tags(name,name_key) VALUES('Keep','keep')")
        .execute(fixture.new.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO staging VALUES(?, 'bad.mp4', 'Video', 1, 1)")
        .bind(fixture.library.join("outside-stage.mp4").to_str().unwrap())
        .execute(&fixture.old)
        .await
        .unwrap();
    assert!(import(&fixture.paths, &fixture.new).await.is_err());
    assert_eq!(fixture.count("SELECT count(*) FROM media").await, 0);
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM tags WHERE name='Keep'")
            .await,
        1
    );
    assert_eq!(fixture.count("SELECT count(*) FROM tags").await, 1);
    fixture.close().await;
}

#[tokio::test]
async fn preserves_both_extreme_rowids() {
    let fixture = Fixture::new().await;
    fixture.media("one.png", "World", "Alice", "Tag").await;
    fixture.media("two.png", "World", "Alice", "Tag").await;
    sqlx::query("UPDATE media SET rowid=? WHERE name='one.png'")
        .bind(i64::MIN)
        .execute(&fixture.old)
        .await
        .unwrap();
    sqlx::query("UPDATE media SET rowid=? WHERE name='two.png'")
        .bind(i64::MAX)
        .execute(&fixture.old)
        .await
        .unwrap();
    assert_eq!(
        import(&fixture.paths, &fixture.new)
            .await
            .unwrap()
            .library_added,
        2
    );
    fixture.close().await;
}
