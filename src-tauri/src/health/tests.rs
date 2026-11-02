use std::{
    fs,
    path::{Path, PathBuf},
};

use sqlx::Row;

use super::*;
use crate::config::AppConfig;

fn image(path: &Path) {
    let mut bytes = vec![0u8; 62];
    bytes[0..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&62u32.to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&2u32.to_le_bytes());
    bytes[22..26].copy_from_slice(&1u32.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
    fs::write(path, bytes).unwrap();
}

async fn fixture() -> (tempfile::TempDir, Database, PathBuf, AppConfig) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Library 日本");
    fs::create_dir(&root).unwrap();
    let database = Database::open(&temp.path().join("health.db"))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(1,'library',?,?,1)",
    )
    .bind(root.to_str().unwrap())
    .bind(root.to_str().unwrap())
    .execute(database.pool())
    .await
    .unwrap();
    sqlx::query(concat!(
        "INSERT INTO media(id,source_id,relative_path,original_filename,path_key,kind,extension,",
        "availability,file_revision,last_seen_at,created_at,updated_at,favorite) ",
        "VALUES(1,1,'old.bmp','original.bmp','old.bmp','image','bmp','missing','old-revision',1,1,1,1)",
    )).execute(database.pool()).await.unwrap();
    sqlx::raw_sql(concat!(
        "INSERT INTO tags VALUES(1,'Kept','kept');",
        "INSERT INTO media_tags VALUES(1,1);",
    ))
    .execute(database.pool())
    .await
    .unwrap();
    let settings = AppConfig {
        library_path: Some(root.clone()),
        ..AppConfig::default()
    };
    (temp, database, root, settings)
}

#[tokio::test]
async fn health_pages_classify_missing_metadata_previews_and_recovery_with_counts() {
    let (_temp, db, root, _settings) = fixture().await;
    sqlx::query(concat!(
        "INSERT INTO media(id,source_id,relative_path,original_filename,path_key,kind,extension,",
        "file_revision,last_seen_at,created_at,updated_at) ",
        "VALUES(2,1,'current.bmp','current.bmp','current.bmp','image','bmp','r2',1,1,1)",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO file_operations VALUES(1,?,'old.bmp','old-revision',1,?,'target.bmp')",
    )
    .bind(root.to_str().unwrap())
    .bind(root.to_str().unwrap())
    .execute(db.pool())
    .await
    .unwrap();
    let failures = serde_json::json!({"previews":[{ "id":2,"revision":"r2","path":format!("{}/current.bmp",root.to_string_lossy()),"reason":"Preview unavailable" }]}).to_string();
    let page = queries::browse(
        db.pool(),
        "missing",
        0,
        &failures,
        Some("Retry disk synchronization".into()),
    )
    .await
    .unwrap();
    let page = serde_json::to_value(page).unwrap();
    assert_eq!(page["counts"], serde_json::json!([1, 2, 1, 1]));
    assert_eq!(page["issues"][0]["mediaId"], "1");
    assert!(
        queries::browse(db.pool(), "unknown", 0, "[]", None)
            .await
            .is_err()
    );
    assert!(
        queries::browse(db.pool(), "missing", -1, "[]", None)
            .await
            .is_err()
    );
    let page = queries::browse(
        db.pool(),
        "previews",
        0,
        &failures.replace("r2", "stale"),
        None,
    )
    .await
    .unwrap();
    assert_eq!(serde_json::to_value(page).unwrap()["total"], 0);
    sqlx::query("UPDATE media SET needs_distribution=1,library_assignment=? WHERE id=2")
        .bind(root.to_str().unwrap())
        .execute(db.pool())
        .await
        .unwrap();
    let metadata = serde_json::to_value(
        queries::browse(db.pool(), "metadata", 0, "[]", None)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        metadata["issues"][0]["collection"], "stage",
        "Redistribution uses the Stage editor even with a saved Library assignment"
    );
    db.close().await;
}

#[cfg(windows)]
#[tokio::test]
async fn relink_to_external_file_preserves_identity_metadata_and_original_bytes() {
    let (temp, db, _root, settings) = fixture().await;
    sqlx::query("UPDATE media SET library_assignment='previous-library-root' WHERE id=1")
        .execute(db.pool())
        .await
        .unwrap();
    let external = temp.path().join("elsewhere");
    fs::create_dir(&external).unwrap();
    let path = external.join("renamed.bmp");
    image(&path);
    let before = fs::read(&path).unwrap();
    let proposal = relinking::inspect(db.pool(), 1, path.clone(), &settings)
        .await
        .unwrap();
    assert!(!proposal.consumes_discovery);
    relinking::confirm(db.pool(), 1, path.clone(), &proposal.token, &settings)
        .await
        .unwrap();
    let row = sqlx::query(
        "SELECT m.*,s.is_active FROM media m JOIN sources s ON s.id=m.source_id WHERE m.id=1",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("availability"), "present");
    assert_eq!(row.get::<String, _>("relative_path"), "renamed.bmp");
    assert_eq!(row.get::<String, _>("original_filename"), "original.bmp");
    assert_eq!(row.get::<i64, _>("favorite"), 1);
    assert_eq!(row.get::<i64, _>("is_active"), 0);
    assert_eq!(
        row.get::<Option<String>, _>("library_assignment"),
        settings
            .library_path
            .map(|path| path.to_string_lossy().into_owned())
    );
    let tags: i64 = sqlx::query_scalar("SELECT count(*) FROM media_tags WHERE media_id=1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(tags, 1);
    assert_eq!(fs::read(path).unwrap(), before);
    let detail = serde_json::to_value(queries::media(db.pool(), 1).await.unwrap()).unwrap();
    assert_eq!(detail["tags"], serde_json::json!(["Kept"]));
    sqlx::query("UPDATE media SET width=NULL WHERE id=1")
        .execute(db.pool())
        .await
        .unwrap();
    stage::refresh_technical_metadata(db.pool(), 1)
        .await
        .unwrap();
    let width: i64 = sqlx::query_scalar("SELECT width FROM media WHERE id=1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(
        width, 2,
        "A forced metadata repair must bypass ready-revision reuse"
    );
    db.close().await;
}

#[cfg(windows)]
#[tokio::test]
async fn relink_consumes_only_explicitly_confirmed_unannotated_discovery() {
    let (_temp, db, root, settings) = fixture().await;
    let path = root.join("new.bmp");
    image(&path);
    sqlx::query(concat!(
        "INSERT INTO media(id,source_id,relative_path,original_filename,path_key,kind,extension,",
        "file_revision,last_seen_at,created_at,updated_at) ",
        "VALUES(2,1,'new.bmp','new.bmp','new.bmp','image','bmp','discovery',1,1,1)",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    let proposal = relinking::inspect(db.pool(), 1, path.clone(), &settings)
        .await
        .unwrap();
    assert!(proposal.consumes_discovery);
    // A favorite added while confirmation is open makes the proposal unsafe.
    sqlx::query("UPDATE media SET favorite=1 WHERE id=2")
        .execute(db.pool())
        .await
        .unwrap();
    assert!(
        relinking::confirm(db.pool(), 1, path.clone(), &proposal.token, &settings)
            .await
            .is_err()
    );
    sqlx::query("UPDATE media SET favorite=0 WHERE id=2")
        .execute(db.pool())
        .await
        .unwrap();
    let proposal = relinking::inspect(db.pool(), 1, path.clone(), &settings)
        .await
        .unwrap();
    relinking::confirm(db.pool(), 1, path, &proposal.token, &settings)
        .await
        .unwrap();
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM media ORDER BY id")
        .fetch_all(db.pool())
        .await
        .unwrap();
    assert_eq!(ids, [1]);
    db.close().await;
}

#[cfg(windows)]
#[tokio::test]
async fn relink_rejects_changed_target_reappeared_original_and_wrong_kind() {
    let (_temp, db, root, settings) = fixture().await;
    let path = root.join("new.bmp");
    image(&path);
    let proposal = relinking::inspect(db.pool(), 1, path.clone(), &settings)
        .await
        .unwrap();
    fs::write(&path, b"changed file").unwrap();
    assert!(
        relinking::confirm(db.pool(), 1, path.clone(), &proposal.token, &settings)
            .await
            .is_err()
    );
    image(&path);
    let proposal = relinking::inspect(db.pool(), 1, path.clone(), &settings)
        .await
        .unwrap();
    image(&root.join("old.bmp"));
    assert!(
        relinking::confirm(db.pool(), 1, path.clone(), &proposal.token, &settings)
            .await
            .is_err()
    );
    fs::remove_file(root.join("old.bmp")).unwrap();
    let video = root.join("video.mp4");
    fs::write(&video, b"video").unwrap();
    assert!(
        relinking::inspect(db.pool(), 1, video, &settings)
            .await
            .is_err()
    );
    let available: String = sqlx::query_scalar("SELECT availability FROM media WHERE id=1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(available, "missing");
    db.close().await;
}

#[tokio::test]
async fn verification_preserves_unavailable_roots_and_finds_definitely_missing_files() {
    let (temp, db, root, _settings) = fixture().await;
    sqlx::query("UPDATE media SET availability='present'")
        .execute(db.pool())
        .await
        .unwrap();
    let result = verification::verify(db.pool()).await.unwrap();
    assert_eq!(
        (result.checked, result.missing, result.unavailable),
        (1, 1, 0)
    );
    sqlx::query("UPDATE media SET availability='present'")
        .execute(db.pool())
        .await
        .unwrap();
    let absent = temp.path().join("disconnected");
    sqlx::query("UPDATE sources SET root_path=?")
        .bind(absent.to_str().unwrap())
        .execute(db.pool())
        .await
        .unwrap();
    let result = verification::verify(db.pool()).await.unwrap();
    assert_eq!(result.unavailable, 1);
    let available: String = sqlx::query_scalar("SELECT availability FROM media WHERE id=1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(available, "present");
    assert!(root.is_dir());
    db.close().await;
}

#[test]
fn late_preview_failures_do_not_replace_newer_successes() {
    let health = HealthState::default();
    let old = health.begin_preview();
    let new = health.begin_preview();
    health.finish_preview(1, Some("revision".into()), "file.bmp".into(), new, None);
    health.finish_preview(
        1,
        Some("revision".into()),
        "file.bmp".into(),
        old,
        Some("Preview unavailable".into()),
    );
    let diagnostics: serde_json::Value = serde_json::from_str(&health.failures().unwrap()).unwrap();
    assert_eq!(diagnostics["previews"], serde_json::json!([]));
}

#[cfg(windows)]
#[tokio::test]
async fn relink_preserves_video_title_while_consuming_a_mechanically_inferred_title() {
    let (_temp, db, root, settings) = fixture().await;
    sqlx::query(concat!(
        "UPDATE media SET kind='video',relative_path='old.mp4',path_key='old.mp4',",
        "extension='mp4',original_filename='old.mp4' WHERE id=1",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("INSERT INTO video_metadata(media_id,title) VALUES(1,'Original title')")
        .execute(db.pool())
        .await
        .unwrap();
    let name = "Hero - Inferred title [Artist].mp4";
    let path = root.join(name);
    fs::write(&path, b"unavailable video handler fixture").unwrap();
    sqlx::query(concat!(
        "INSERT INTO media(id,source_id,relative_path,original_filename,path_key,kind,extension,",
        "last_seen_at,created_at,updated_at) VALUES(2,1,?,?,?,'video','mp4',1,1,1)",
    ))
    .bind(name)
    .bind(name)
    .bind(name)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("INSERT INTO video_metadata(media_id,title) VALUES(2,'Inferred title')")
        .execute(db.pool())
        .await
        .unwrap();
    let proposal = relinking::inspect(db.pool(), 1, path.clone(), &settings)
        .await
        .unwrap();
    assert!(proposal.consumes_discovery);
    relinking::confirm(db.pool(), 1, path.clone(), &proposal.token, &settings)
        .await
        .unwrap();
    let title: String = sqlx::query_scalar("SELECT title FROM video_metadata WHERE media_id=1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(title, "Original title");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM media")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        fs::read(path).unwrap(),
        b"unavailable video handler fixture"
    );
    db.close().await;
}

#[tokio::test]
async fn health_pagination_is_bounded_and_unavailable_roots_remain_actionable() {
    let (_temp, db, _root, _settings) = fixture().await;
    let mut tx = db.pool().begin().await.unwrap();
    for id in 2..132 {
        let path = format!("{id}.bmp");
        sqlx::query(concat!(
            "INSERT INTO media(id,source_id,relative_path,original_filename,path_key,kind,extension,",
            "availability,last_seen_at,created_at,updated_at) VALUES(?,1,?,?,?,'image','bmp','missing',1,1,1)",
        )).bind(id).bind(&path).bind(&path).bind(&path).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    let diagnostics = serde_json::json!({"previews":[],"roots":["Disconnected drive"]}).to_string();
    let first = serde_json::to_value(
        queries::browse(db.pool(), "missing", 0, &diagnostics, None)
            .await
            .unwrap(),
    )
    .unwrap();
    let next = serde_json::to_value(
        queries::browse(db.pool(), "missing", 100, &diagnostics, None)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first["total"], 131);
    assert_eq!(first["issues"].as_array().unwrap().len(), 100);
    assert_eq!(next["issues"].as_array().unwrap().len(), 31);
    assert_eq!(first["counts"][1], 1);
    let mut ids = first["issues"]
        .as_array()
        .unwrap()
        .iter()
        .chain(next["issues"].as_array().unwrap())
        .map(|issue| issue["key"].as_str().unwrap())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 131);
    db.close().await;
}

#[cfg(windows)]
#[tokio::test]
async fn failed_relink_transaction_restores_consumed_discovery_and_original_annotations() {
    let (_temp, db, root, settings) = fixture().await;
    let path = root.join("new.bmp");
    image(&path);
    sqlx::query(concat!(
        "INSERT INTO media(id,source_id,relative_path,original_filename,path_key,kind,extension,",
        "last_seen_at,created_at,updated_at) VALUES(2,1,'new.bmp','new.bmp','new.bmp','image','bmp',1,1,1)",
    )).execute(db.pool()).await.unwrap();
    sqlx::raw_sql(concat!(
        "CREATE TRIGGER reject_relink BEFORE UPDATE ON media WHEN NEW.id=1 ",
        "BEGIN SELECT RAISE(ABORT,'test rollback'); END;",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    let proposal = relinking::inspect(db.pool(), 1, path.clone(), &settings)
        .await
        .unwrap();
    assert!(
        relinking::confirm(db.pool(), 1, path.clone(), &proposal.token, &settings)
            .await
            .is_err()
    );
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM media ORDER BY id")
        .fetch_all(db.pool())
        .await
        .unwrap();
    assert_eq!(ids, [1, 2]);
    let path_key: String = sqlx::query_scalar("SELECT path_key FROM media WHERE id=1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(path_key, "old.bmp");
    assert!(path.is_file());
    let tags: i64 = sqlx::query_scalar("SELECT count(*) FROM media_tags WHERE media_id=1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(tags, 1);
    db.close().await;
}

#[cfg(windows)]
#[tokio::test]
async fn stage_relink_to_external_location_keeps_queue_membership_and_blocks_pending_moves() {
    let (temp, db, root, mut settings) = fixture().await;
    sqlx::query("UPDATE sources SET kind='stage'")
        .execute(db.pool())
        .await
        .unwrap();
    settings.staging_path = Some(root.clone());
    settings.library_path = None;
    let external = temp.path().join("elsewhere.bmp");
    image(&external);
    sqlx::query(
        "INSERT INTO file_operations VALUES(1,?,'old.bmp','old-revision',1,?,'target.bmp')",
    )
    .bind(root.to_str().unwrap())
    .bind(root.to_str().unwrap())
    .execute(db.pool())
    .await
    .unwrap();
    assert!(
        relinking::inspect(db.pool(), 1, external.clone(), &settings)
            .await
            .is_err()
    );
    sqlx::query("DELETE FROM file_operations")
        .execute(db.pool())
        .await
        .unwrap();
    let proposal = relinking::inspect(db.pool(), 1, external.clone(), &settings)
        .await
        .unwrap();
    relinking::confirm(db.pool(), 1, external, &proposal.token, &settings)
        .await
        .unwrap();
    let page = stage::browse(
        db.pool(),
        Some(root.to_string_lossy().into_owned()),
        0,
        "",
        "newest",
        "all",
    )
    .await
    .unwrap();
    assert_eq!(page.total, 1);
    let queue: i64 = sqlx::query_scalar("SELECT needs_distribution FROM media WHERE id=1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(queue, 1);
    db.close().await;
}
