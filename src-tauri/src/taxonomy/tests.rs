use super::*;

#[tokio::test]
async fn character_reassignment_preserves_identity_links_and_existing_copyrights() {
    let (root, db) = fixture().await;
    sqlx::query("INSERT INTO copyrights(id,name,name_key) VALUES(3,'New','new')")
        .execute(db.pool())
        .await
        .unwrap();

    sqlx::raw_sql(concat!(
        "INSERT INTO media(id,source_id,artist_id,relative_path,original_filename,path_key,",
        "kind,extension,last_seen_at,created_at,updated_at,needs_distribution) ",
        "VALUES(2,1,1,'queued.png','queued.png','queued.png','image','png',1,1,1,1); ",
        "INSERT INTO media_copyrights VALUES(2,1); INSERT INTO media_characters VALUES(2,1,1);",
    ))
    .execute(db.pool())
    .await
    .unwrap();

    assert_eq!(
        editing::update_character(db.pool(), 1, " Hero ", 3)
            .await
            .unwrap(),
        2
    );

    let character: (i64, String) =
        sqlx::query_as("SELECT copyright_id,name FROM characters WHERE id = 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(character, (3, "Hero".into()));

    let links: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT character_id,copyright_id FROM media_characters ORDER BY character_id,media_id",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    assert_eq!(links, vec![(1, 3), (1, 3), (2, 2)]);

    let copyrights: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT copyright_id FROM media_copyrights ORDER BY copyright_id",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    assert_eq!(copyrights, vec![1, 2, 3]);

    let media: (String, String, i64, i64) = sqlx::query_as(
        "SELECT relative_path,original_filename,artist_id,needs_distribution FROM media WHERE id = 1",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(media, ("image.png".into(), "image.png".into(), 1, 0));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pragma_foreign_key_check")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        std::fs::read(root.path().join("image.png")).unwrap(),
        b"unchanged media bytes"
    );

    db.close().await;
    let reopened = Database::open(&root.path().join("vault.db")).await.unwrap();
    assert_eq!(
        entries(reopened.pool(), Category::Character)
            .await
            .unwrap()
            .iter()
            .find(|entry| entry.id == "1")
            .unwrap()
            .copyright
            .as_deref(),
        Some("New")
    );
}

#[tokio::test]
async fn character_reassignment_rejects_collisions_missing_entries_and_rolls_back_links() {
    let (_root, db) = fixture().await;

    assert!(
        editing::update_character(db.pool(), 1, "ALICE", 2)
            .await
            .unwrap_err()
            .contains("already exists")
    );
    assert!(
        editing::update_character(db.pool(), 1, "Hero", 999)
            .await
            .is_err()
    );
    assert!(
        editing::update_character(db.pool(), 999, "Hero", 2)
            .await
            .is_err()
    );
    assert!(
        editing::update_character(db.pool(), 1, " ", 2)
            .await
            .is_err()
    );

    sqlx::query("INSERT INTO copyrights(id,name,name_key) VALUES(3,'New','new')")
        .execute(db.pool())
        .await
        .unwrap();

    // Fail after parent mutation and link removal to exercise complete rollback.
    sqlx::raw_sql(concat!(
        "CREATE TRIGGER reject_character_link BEFORE INSERT ON media_characters ",
        "BEGIN SELECT RAISE(ABORT,'blocked'); END;",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    assert!(
        editing::update_character(db.pool(), 1, "Hero", 3)
            .await
            .is_err()
    );

    assert_eq!(
        sqlx::query_as::<_, (i64, String)>("SELECT copyright_id,name FROM characters WHERE id = 1")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        (1, "Alice".into())
    );
    assert_eq!(
        sqlx::query_as::<_, (i64, i64)>(
            "SELECT character_id,copyright_id FROM media_characters WHERE character_id = 1"
        )
        .fetch_one(db.pool())
        .await
        .unwrap(),
        (1, 1)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pragma_foreign_key_check")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn character_edit_supports_same_copyright_and_unused_characters() {
    let (_root, db) = fixture().await;
    assert_eq!(
        editing::update_character(db.pool(), 1, "Hero", 1)
            .await
            .unwrap(),
        1
    );

    sqlx::query(
        "INSERT INTO characters(id,copyright_id,name,name_key) VALUES(3,1,'Unused','unused')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    assert_eq!(
        editing::update_character(db.pool(), 3, "Unused", 2)
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

async fn fixture() -> (tempfile::TempDir, Database) {
    let root = tempfile::tempdir().unwrap();

    let db = Database::open(&root.path().join("vault.db")).await.unwrap();
    std::fs::write(root.path().join("image.png"), b"unchanged media bytes").unwrap();
    sqlx::raw_sql(concat!(
        "INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(1,'library',",
        "'/library','/library',1); INSERT INTO copyrights(id,name,name_key) VALUES(1,'World',",
        "'world'),(2,'Other','other'); INSERT INTO artists(id,name,name_key) VALUES(1,'Artist',",
        "'artist'); INSERT INTO tags(id,name,name_key) VALUES(1,'Tag','tag'),(2,'Other',",
        "'other'); INSERT INTO characters(id,copyright_id,name,name_key) VALUES(1,1,'Alice',",
        "'alice'),(2,2,'Alice','alice'); INSERT INTO media(id,source_id,artist_id,",
        "relative_path,original_filename,path_key,kind,extension,last_seen_at,created_at,",
        "updated_at) VALUES(1,1,1,'image.png','image.png','image.png','image','png',1,1,1); ",
        "INSERT INTO media_copyrights VALUES(1,1),(1,2); INSERT INTO media_characters VALUES(1,",
        "1,1),(1,2,2); INSERT INTO media_tags VALUES(1,1);",
    ))
    .execute(db.pool())
    .await
    .unwrap();
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
async fn character_rename_preserves_qualified_links_and_scopes_duplicates() {
    let (_root, db) = fixture().await;
    rename(db.pool(), Category::Character, 1, "Hero")
        .await
        .unwrap();
    rename(db.pool(), Category::Character, 2, "Hero")
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO characters(id,copyright_id,name,name_key) VALUES(3,1,'Other','other')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    assert!(
        rename(db.pool(), Category::Character, 3, "HERO")
            .await
            .is_err()
    );

    let catalog = entries(db.pool(), Category::Character).await.unwrap();
    assert_eq!(catalog.len(), 3);

    let entry = catalog.iter().find(|entry| entry.id == "1").unwrap();
    assert_eq!(entry.name, "Hero");
    assert_eq!(entry.copyright.as_deref(), Some("World"));
    assert_eq!(entry.count, 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM media_characters")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT needs_distribution FROM media")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn character_deletion_preserves_other_metadata_and_rolls_back_on_failure() {
    let (root, db) = fixture().await;
    sqlx::raw_sql(concat!(
        "CREATE TRIGGER reject_character_delete BEFORE DELETE ON characters BEGIN SELECT ",
        "RAISE(ABORT,'blocked'); END;",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    assert!(remove(db.pool(), Category::Character, 1).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM media_characters")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        2
    );
    sqlx::raw_sql("DROP TRIGGER reject_character_delete;")
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(remove(db.pool(), Category::Character, 1).await.unwrap(), 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT character_id FROM media_characters")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM media_copyrights")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM media_tags")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_as::<_, (i64, i64, String)>(
            "SELECT needs_distribution,artist_id,relative_path FROM media"
        )
        .fetch_one(db.pool())
        .await
        .unwrap(),
        (0, 1, "image.png".into())
    );
    db.close().await;

    let reopened = Database::open(&root.path().join("vault.db")).await.unwrap();

    let catalog = entries(reopened.pool(), Category::Character).await.unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].id, "2");
    assert_eq!(
        std::fs::read(root.path().join("image.png")).unwrap(),
        b"unchanged media bytes"
    );
}

#[tokio::test]
async fn failed_delete_rolls_back_queue_and_links() {
    let (_root, db) = fixture().await;
    sqlx::raw_sql("CREATE TRIGGER reject_delete BEFORE DELETE ON copyrights BEGIN SELECT RAISE(ABORT,'blocked'); END;")
    .execute(db.pool())
    .await
    .unwrap();
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
