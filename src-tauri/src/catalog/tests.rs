use serde_json::json;

use super::{portable, snapshots};
use crate::database::Database;

async fn fixture() -> (tempfile::TempDir, Database) {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("catalog.db"))
        .await
        .unwrap();
    let source = root.path().join("media").to_string_lossy().into_owned();
    sqlx::query(
        "INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(1,'library',?,?,1)",
    )
    .bind(&source)
    .bind(&source)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::raw_sql(concat!(
        "INSERT INTO artists VALUES(1,'Artist','artist');",
        "INSERT INTO copyrights VALUES(1,'Series','series');",
        "INSERT INTO characters VALUES(1,1,'Hero','hero');",
        "INSERT INTO tags VALUES(1,'Tag','tag'),(2,'Unused','unused');",
        "INSERT INTO media(id,source_id,artist_id,relative_path,original_filename,path_key,kind,",
        "extension,last_seen_at,created_at,updated_at,favorite,naming_order_key,naming_order) ",
        "VALUES(9007199254740993,1,1,'clip.mp4','original.mp4','clip.mp4','video','mp4',1,1,1,1,'group',2);",
        "INSERT INTO video_metadata(media_id,title) VALUES(9007199254740993,'Video title');",
        "INSERT INTO media_copyrights VALUES(9007199254740993,1);",
        "INSERT INTO media_characters VALUES(9007199254740993,1,1);",
        "INSERT INTO media_tags VALUES(9007199254740993,1);",
        "INSERT INTO metadata_presets(name,name_key,metadata) VALUES('Saved','saved','{}');",
    )).execute(db.pool()).await.unwrap();
    (root, db)
}

#[tokio::test]
async fn backup_roundtrip_preserves_annotations_large_ids_unused_taxonomy_and_order() {
    let (_root, db) = fixture().await;
    let original = snapshots::capture(db.pool()).await.unwrap();
    let json = serde_json::to_vec(&original).unwrap();
    let decoded = serde_json::from_slice(&json).unwrap();
    sqlx::query("UPDATE media SET favorite=0,naming_order=5")
        .execute(db.pool())
        .await
        .unwrap();
    let expected = snapshots::capture(db.pool()).await.unwrap();
    snapshots::restore(db.pool(), &decoded, &expected)
        .await
        .unwrap();
    let restored = snapshots::capture(db.pool()).await.unwrap();
    assert_eq!(original.tables, restored.tables);
    assert_eq!(
        restored.tables["media"][0]["id"],
        json!(9007199254740993_i64)
    );
    db.close().await;
}

#[tokio::test]
async fn invalid_links_and_columns_roll_back_the_entire_catalog() {
    let (_root, db) = fixture().await;
    let original = snapshots::capture(db.pool()).await.unwrap();
    let mut broken = snapshots::capture(db.pool()).await.unwrap();
    broken.tables.get_mut("media_tags").unwrap()[0].insert("tag_id".into(), json!(99));
    assert!(
        snapshots::restore(db.pool(), &broken, &original)
            .await
            .is_err()
    );
    assert_eq!(
        original.tables,
        snapshots::capture(db.pool()).await.unwrap().tables
    );

    broken.tables.get_mut("media").unwrap()[0].insert("unexpected".into(), json!(1));
    assert!(
        snapshots::restore(db.pool(), &broken, &original)
            .await
            .is_err()
    );
    assert_eq!(
        original.tables,
        snapshots::capture(db.pool()).await.unwrap().tables
    );
    db.close().await;
}

#[tokio::test]
async fn portable_metadata_resolves_names_and_qualified_characters_without_database_ids() {
    let (_root, db) = fixture().await;
    let snapshot = snapshots::capture(db.pool()).await.unwrap();
    let result = portable::export(&snapshot).unwrap();
    let file = &result["files"][0];
    assert_eq!(file["artist"], "Artist");
    assert_eq!(
        file["characters"],
        json!([{ "name": "Hero", "copyright": "Series" }])
    );
    assert_eq!(file["tags"], json!(["Tag"]));
    assert_eq!(file["favorite"], true);
    assert_eq!(file["videoTitle"], "Video title");
    assert_eq!(file["namingOrder"], 2);
    assert!(file.get("artist_id").is_none());
    assert_eq!(result["taxonomy"]["tags"], json!(["Tag", "Unused"]));
    assert!(serde_json::from_value::<snapshots::Snapshot>(result).is_err());
    db.close().await;
}

#[tokio::test]
async fn malformed_paths_versions_and_pending_recovery_are_rejected() {
    let (_root, db) = fixture().await;
    let mut snapshot = snapshots::capture(db.pool()).await.unwrap();
    snapshot.tables.get_mut("media").unwrap()[0]
        .insert("relative_path".into(), json!("../escape.png"));
    assert!(snapshot.validate().is_err());
    snapshot.tables.get_mut("media").unwrap()[0].remove("relative_path");
    assert!(snapshot.validate().is_err());

    let mut value = serde_json::to_value(snapshots::capture(db.pool()).await.unwrap()).unwrap();
    value["version"] = json!(999);
    let incompatible: snapshots::Snapshot = serde_json::from_value(value).unwrap();
    assert!(incompatible.validate().is_err());

    sqlx::query(concat!(
        "INSERT INTO file_operations VALUES(9007199254740993,'root','clip.mp4','rev',1,",
        "'target','clip.mp4')",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    assert!(snapshots::capture(db.pool()).await.is_err());
    assert!(
        snapshots::restore(db.pool(), &incompatible, &incompatible)
            .await
            .is_err()
    );
    db.close().await;
}

#[tokio::test]
async fn atomic_files_never_overwrite_and_reject_truncated_json() {
    let (root, db) = fixture().await;
    let backup = snapshots::capture(db.pool()).await.unwrap();
    let path = root.path().join("backup.json");
    let bytes = serde_json::to_vec(&backup).unwrap();
    snapshots::write(path.clone(), bytes.clone()).await.unwrap();
    assert!(
        snapshots::write(path.clone(), b"different".to_vec())
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let (read, digest) = snapshots::read(path.clone()).await.unwrap();
    assert_eq!(read.tables, backup.tables);
    assert_eq!(digest.len(), 64);
    std::fs::write(&path, b"{\"format\":").unwrap();
    assert!(snapshots::read(path).await.is_err());
    db.close().await;
}

#[tokio::test]
async fn restore_refuses_changes_after_the_safety_snapshot_and_pending_recovery() {
    let (_root, db) = fixture().await;
    let backup = snapshots::capture(db.pool()).await.unwrap();
    sqlx::query("UPDATE media SET favorite=0")
        .execute(db.pool())
        .await
        .unwrap();
    assert!(
        snapshots::restore(db.pool(), &backup, &backup)
            .await
            .unwrap_err()
            .contains("catalog changed")
    );
    let expected = snapshots::capture(db.pool()).await.unwrap();
    assert_eq!(expected.tables["media"][0]["favorite"], 0);

    sqlx::query(concat!(
        "INSERT INTO file_operations VALUES(9007199254740993,'root','clip.mp4','rev',1,",
        "'target','clip.mp4')",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    assert!(
        snapshots::restore(db.pool(), &backup, &expected)
            .await
            .unwrap_err()
            .contains("pending disk recovery")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT favorite FROM media")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        0
    );
    db.close().await;
}

#[tokio::test]
async fn restore_requires_matching_active_roots_and_preserves_original_bytes() {
    let (root, db) = fixture().await;
    let backup = snapshots::capture(db.pool()).await.unwrap();
    let mut settings = crate::config::AppConfig::default();
    assert!(snapshots::check_roots(&backup, &settings).is_err());
    settings.library_path = Some(root.path().join("media"));
    snapshots::check_roots(&backup, &settings).unwrap();
    std::fs::create_dir_all(settings.library_path.as_ref().unwrap()).unwrap();
    let original = settings.library_path.unwrap().join("clip.mp4");
    std::fs::write(&original, b"original bytes").unwrap();
    snapshots::restore(db.pool(), &backup, &backup)
        .await
        .unwrap();
    assert_eq!(std::fs::read(original).unwrap(), b"original bytes");
    db.close().await;
}
