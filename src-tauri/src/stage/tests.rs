use sqlx::Row;

use super::scanning::{Prepared, WalkMessage};
use super::scanning::{
    activate_source, prepare, reconcile, save_batch, scan, scan_source, video_title_from_filename,
    walk,
};
use super::*;
use crate::media_metadata::FrameRate;
use crate::{database::Database, media_metadata::MediaMetadata};

#[test]
fn file_actions_reject_escaping_missing_and_non_file_paths() {
    let temp = tempfile::tempdir().unwrap();

    let root = temp.path().join("Stage");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("日本.png"), b"test").unwrap();
    assert_eq!(
        checked_action_path(&root, "日本.png").unwrap(),
        root.join("日本.png")
    );
    std::fs::write(temp.path().join("outside.png"), b"preserve").unwrap();

    for relative in ["../outside.png", "", "missing.png", "."] {
        assert!(checked_action_path(&root, relative).is_err());
    }
    std::fs::create_dir(root.join("folder")).unwrap();
    assert!(checked_action_path(&root, "folder").is_err());
    assert!(checked_action_path(&root, temp.path().join("outside.png").to_str().unwrap()).is_err());
    assert!(checked_action_path(Path::new("relative"), "image.png").is_err());
    assert_eq!(
        std::fs::read(temp.path().join("outside.png")).unwrap(),
        b"preserve"
    );
}

#[test]
fn file_action_guard_releases_scan_gate_on_failure() {
    let gate = AtomicBool::new(true);
    {
        let _guard = FileActionGuard(&gate);
        assert!(gate.load(Ordering::Acquire));
    }
    assert!(!gate.load(Ordering::Acquire));
}

async fn fixture() -> (tempfile::TempDir, Database, PathBuf) {
    let temp = tempfile::tempdir().unwrap();

    let root = temp.path().join("Stage 日本");
    std::fs::create_dir(&root).unwrap();

    let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
    (temp, db, root)
}

#[tokio::test]
async fn retained_replacement_is_not_reindexed_until_its_revision_changes() {
    let (_temp, db, root) = fixture().await;

    let source = activate_source(db.pool(), &root, 1).await.unwrap();

    let path = root.join("old.bmp");
    write_bmp(&path, 2);

    let file = inspect(path.clone(), &root).unwrap().unwrap();
    sqlx::query(concat!(
        "INSERT INTO media_replacements(main_id,stage_id,old_source,old_root,old_relative,",
        "old_revision,new_source,new_root,new_relative,new_revision,incoming_filename,",
        "library_root,committed,finished) VALUES(1,2,?,?,'old.bmp',?,?,?,'new.bmp','new',",
        "'new.bmp',?,1,1)",
    ))
    .bind(source)
    .bind(root.to_str().unwrap())
    .bind(&file.revision)
    .bind(source)
    .bind(root.to_str().unwrap())
    .bind(root.to_str().unwrap())
    .execute(db.pool())
    .await
    .unwrap();

    let mut status = ScanStatus::default();

    let result = prepare(db.pool(), source, vec![file], &mut status)
        .await
        .unwrap();
    assert!(result.is_empty());
    save_batch(db.pool(), source, 2, result).await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM media")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    write_bmp(&path, 5);

    let file = inspect(path, &root).unwrap().unwrap();

    let result = prepare(db.pool(), source, vec![file], &mut status)
        .await
        .unwrap();
    assert_eq!(result.len(), 1);
    save_batch(db.pool(), source, 3, result).await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM media")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
    db.close().await;
}

fn write_bmp(path: &Path, width: u32) {
    let row = (width * 3).div_ceil(4) * 4;

    let mut bytes = vec![0u8; (54 + row) as usize];
    bytes[0..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&(54 + row).to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&width.to_le_bytes());
    bytes[22..26].copy_from_slice(&1u32.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
    bytes[34..38].copy_from_slice(&row.to_le_bytes());
    std::fs::write(path, bytes).unwrap();
}

fn prepared(root: &Path, name: &str, width: u32) -> Prepared {
    let path = root.join(name);
    std::fs::write(&path, b"sample").unwrap();
    Prepared {
        file: inspect(path, root).unwrap().unwrap(),
        metadata: Some(MediaMetadata {
            width: Some(width),
            height: Some(1),
            duration_ms: Some(1234),
            frame_rate: Some(FrameRate {
                numerator: 30000,
                denominator: 1001,
            }),
            codec: Some("H.264".into()),
            bitrate_bps: Some(4_000_000),
            ..Default::default()
        }),
        failed: false,
    }
}

#[cfg(windows)]
#[tokio::test]
async fn library_scan_preserves_annotations_and_allows_nested_stage() {
    let (_temp, db, root) = fixture().await;
    write_bmp(&root.join("Image.bmp"), 2);

    let first = scan_source(db.pool(), root.clone(), "library", None, |_| {})
        .await
        .unwrap();
    assert_eq!(first.scanned, 1);
    sqlx::query("UPDATE media SET favorite=1")
        .execute(db.pool())
        .await
        .unwrap();

    let second = scan_source(db.pool(), root.clone(), "library", None, |_| {})
        .await
        .unwrap();
    assert_eq!(second.extracted, 0);

    let row =
        sqlx::query("SELECT s.kind,m.favorite FROM media m JOIN sources s ON s.id=m.source_id")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(row.get::<String, _>("kind"), "library");
    assert_eq!(row.get::<i64, _>("favorite"), 1);

    let nested = root.join("Stage");
    std::fs::create_dir(&nested).unwrap();
    write_bmp(&nested.join("Incoming.bmp"), 2);
    std::fs::create_dir(nested.join("deep")).unwrap();
    write_bmp(&nested.join("deep/Incoming.bmp"), 2);
    std::fs::create_dir(root.join("Stage sibling")).unwrap();
    write_bmp(&root.join("Stage sibling/Kept.bmp"), 2);

    let library = scan_source(
        db.pool(),
        root.clone(),
        "library",
        Some(nested.clone()),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(library.scanned, 2);

    let stage = scan_source(db.pool(), nested, "stage", None, |_| {})
        .await
        .unwrap();
    assert_eq!(stage.scanned, 2);

    let library = scan_source(
        db.pool(),
        root.clone(),
        "library",
        Some(root.join("Stage")),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(library.scanned, 2);
    assert!(
        scan_source(db.pool(), root, "stage", None, |_| {})
            .await
            .is_err()
    );
    db.close().await;
}

#[cfg(windows)]
#[tokio::test]
async fn scan_reuses_metadata_refreshes_changes_and_retains_missing_identity() {
    let (_temp, db, root) = fixture().await;

    let nested = root.join("nested");
    std::fs::create_dir(&nested).unwrap();

    let path = nested.join("Зображення.bmp");
    write_bmp(&path, 2);
    std::fs::write(root.join("ignore.zip"), b"archive").unwrap();

    let first = scan(db.pool(), root.clone(), |_| {}).await.unwrap();
    assert_eq!((first.scanned, first.extracted, first.errors), (1, 1, 0));

    let id: i64 = sqlx::query_scalar("SELECT id FROM media")
        .fetch_one(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE media SET favorite=1,needs_distribution=1")
        .execute(db.pool())
        .await
        .unwrap();

    let second = scan(db.pool(), root.clone(), |_| {}).await.unwrap();
    assert_eq!((second.scanned, second.extracted), (1, 0));
    write_bmp(&path, 5);

    let third = scan(db.pool(), root.clone(), |_| {}).await.unwrap();
    assert_eq!(third.extracted, 1);

    let row = sqlx::query(
        "SELECT id,width,size_bytes,favorite,needs_distribution,file_created_at FROM media",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<i64, _>("id"), id);
    assert_eq!(row.get::<i64, _>("width"), 5);
    assert_eq!(row.get::<i64, _>("favorite"), 1);
    assert_eq!(row.get::<i64, _>("needs_distribution"), 1);
    assert_eq!(
        row.get::<i64, _>("size_bytes"),
        std::fs::metadata(&path).unwrap().len() as i64
    );
    assert!(row.get::<Option<i64>, _>("file_created_at").is_some());
    // Tests remove only their own disposable fixture file.
    std::fs::remove_file(&path).unwrap();
    scan(db.pool(), root.clone(), |_| {}).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT availability FROM media")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        "missing"
    );
    write_bmp(&path, 2);
    scan(db.pool(), root, |_| {}).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT id FROM media WHERE availability='present'")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        id
    );
    db.close().await;
}

#[tokio::test]
async fn library_scan_recovers_titles_for_unchanged_imported_videos_without_overwriting_edits() {
    let (temp, db, root) = fixture().await;

    let source = activate_source_for(db.pool(), &root, 1, "library")
        .await
        .unwrap();

    let name = "Alice x Zed - Назва - Part 2 [Artist] - 3.mp4";
    save_batch(db.pool(), source, 1, vec![prepared(&root, name, 20)])
        .await
        .unwrap();

    let id: i64 = sqlx::query_scalar("SELECT id FROM media")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT title FROM video_metadata")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        "Назва - Part 2"
    );
    // An old import/rescan can already have fresh technical metadata but no title.
    sqlx::query("UPDATE video_metadata SET title=NULL")
        .execute(db.pool())
        .await
        .unwrap();

    let files = vec![inspect(root.join(name), &root).unwrap().unwrap()];

    let mut status = ScanStatus::default();

    let batch = prepare(db.pool(), source, files, &mut status)
        .await
        .unwrap();
    assert_eq!(status.extracted, 0);
    save_batch(db.pool(), source, 2, batch).await.unwrap();

    let row = sqlx::query(concat!(
        "SELECT m.id,m.width,v.title,v.duration_ms,v.codec FROM media m JOIN video_metadata v ",
        "ON v.media_id=m.id",
    ))
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<i64, _>("id"), id);
    assert_eq!(row.get::<i64, _>("width"), 20);
    assert_eq!(row.get::<String, _>("title"), "Назва - Part 2");
    assert_eq!(row.get::<i64, _>("duration_ms"), 1234);
    assert_eq!(row.get::<String, _>("codec"), "H.264");
    sqlx::query("UPDATE video_metadata SET title='My edited title'")
        .execute(db.pool())
        .await
        .unwrap();
    save_batch(db.pool(), source, 3, vec![prepared(&root, name, 30)])
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT title FROM video_metadata")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        "My edited title"
    );
    sqlx::query("DELETE FROM video_metadata")
        .execute(db.pool())
        .await
        .unwrap();

    let mut unchanged = prepared(&root, name, 30);
    unchanged.metadata = None;
    save_batch(db.pool(), source, 4, vec![unchanged])
        .await
        .unwrap();
    sqlx::query("UPDATE video_metadata SET title='\t\r\n'")
        .execute(db.pool())
        .await
        .unwrap();

    let mut failed = prepared(&root, name, 30);
    failed.metadata = None;
    failed.failed = true;
    save_batch(db.pool(), source, 5, vec![failed])
        .await
        .unwrap();
    db.close().await;

    let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT title FROM video_metadata")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        "Назва - Part 2"
    );
    db.close().await;
}

#[test]
fn parses_only_structured_video_titles_and_keeps_title_punctuation() {
    for name in [
        "Alice x Zed - Video name [Artist].mp4",
        "Alice х Zed - Video name [Artist].mkv",
        "Alice - Video name [Artist] - 12.webm",
    ] {
        assert_eq!(video_title_from_filename(name), Some("Video name"));
    }
    assert_eq!(
        video_title_from_filename("Alice - A [cut] - Part 2 [Artist].mp4"),
        Some("A [cut] - Part 2")
    );

    for name in [
        "clip.mp4",
        "Alice [Artist].mp4",
        "Alice -   [Artist].mp4",
        " - Title [Artist].mp4",
        "Alice - Title [].mp4",
        "Alice - Title [Artist] extra.mp4",
        "Alice - Title [Artist] - 0.mp4",
    ] {
        assert_eq!(video_title_from_filename(name), None, "{name}");
    }
}

#[tokio::test]
async fn title_recovery_ignores_stage_and_unstructured_library_filenames() {
    let (_temp, db, root) = fixture().await;

    let source = activate_source(db.pool(), &root, 1).await.unwrap();
    save_batch(
        db.pool(),
        source,
        1,
        vec![prepared(&root, "Alice - Title [Artist].mp4", 10)],
    )
    .await
    .unwrap();
    assert!(
        sqlx::query_scalar::<_, Option<String>>("SELECT title FROM video_metadata")
            .fetch_one(db.pool())
            .await
            .unwrap()
            .is_none()
    );

    let library_root = root.parent().unwrap().join("Library");
    std::fs::create_dir(&library_root).unwrap();

    let library = activate_source_for(db.pool(), &library_root, 2, "library")
        .await
        .unwrap();
    save_batch(
        db.pool(),
        library,
        2,
        vec![prepared(&library_root, "plain video.mp4", 10)],
    )
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM video_metadata WHERE title IS NOT NULL")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        0
    );
    db.close().await;
}

#[tokio::test]
async fn persists_video_details_and_preserves_annotations_when_refreshing() {
    let (temp, db, root) = fixture().await;

    let source = activate_source(db.pool(), &root, 1).await.unwrap();
    save_batch(db.pool(), source, 1, vec![prepared(&root, "clip.mp4", 10)])
        .await
        .unwrap();
    sqlx::query("UPDATE video_metadata SET title='Keep title'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE media SET favorite=1,needs_distribution=1,original_filename='original.mp4'",
    )
    .execute(db.pool())
    .await
    .unwrap();
    save_batch(db.pool(), source, 2, vec![prepared(&root, "clip.mp4", 20)])
        .await
        .unwrap();

    let row = sqlx::query("SELECT m.*,v.* FROM media m JOIN video_metadata v ON m.id=v.media_id")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("width"), 20);
    assert_eq!(row.get::<i64, _>("duration_ms"), 1234);
    assert_eq!(row.get::<i64, _>("fps_den"), 1001);
    assert_eq!(row.get::<String, _>("codec"), "H.264");
    assert_eq!(row.get::<i64, _>("bitrate_bps"), 4_000_000);
    assert_eq!(row.get::<String, _>("title"), "Keep title");
    assert_eq!(row.get::<String, _>("original_filename"), "original.mp4");
    assert_eq!(row.get::<i64, _>("favorite"), 1);
    db.close().await;

    let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT width FROM media")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        20
    );
    db.close().await;
}

#[tokio::test]
async fn incomplete_scans_preserve_unseen_records_and_failed_roots_do_not_activate() {
    let (_temp, db, root) = fixture().await;

    let source = activate_source(db.pool(), &root, 1).await.unwrap();
    save_batch(db.pool(), source, 1, vec![prepared(&root, "clip.mp4", 10)])
        .await
        .unwrap();
    assert!(!reconcile(db.pool(), source, 2, true, 1).await.unwrap());
    assert!(!reconcile(db.pool(), source, 2, false, 0).await.unwrap());
    assert!(scan(db.pool(), root.join("missing"), |_| {}).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT availability FROM media")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        "present"
    );
    assert!(
        sqlx::query_scalar::<_, Option<i64>>("SELECT last_successful_scan_at FROM sources")
            .fetch_one(db.pool())
            .await
            .unwrap()
            .is_none()
    );
    assert!(reconcile(db.pool(), source, 2, true, 0).await.unwrap());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT availability FROM media")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        "missing"
    );
    db.close().await;
}

#[tokio::test]
async fn batch_failure_rolls_back_and_switching_roots_keeps_history() {
    let (_temp, db, root) = fixture().await;

    let source = activate_source(db.pool(), &root, 1).await.unwrap();
    sqlx::raw_sql(concat!(
        "CREATE TRIGGER reject_second BEFORE INSERT ON media WHEN NEW.relative_path='bad.mp4' ",
        "BEGIN SELECT RAISE(ABORT,'test failure'); END;",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    assert!(
        save_batch(
            db.pool(),
            source,
            1,
            vec![
                prepared(&root, "ok.mp4", 10),
                prepared(&root, "bad.mp4", 10)
            ]
        )
        .await
        .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM media")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        0
    );
    save_batch(db.pool(), source, 1, vec![prepared(&root, "ok.mp4", 10)])
        .await
        .unwrap();

    let other = root.join("other");
    std::fs::create_dir(&other).unwrap();

    let second = activate_source(db.pool(), &other, 2).await.unwrap();
    assert_ne!(source, second);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM media")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT is_active FROM sources WHERE id=?")
            .bind(source)
            .fetch_one(db.pool())
            .await
            .unwrap(),
        0
    );
    assert_eq!(activate_source(db.pool(), &root, 3).await.unwrap(), source);
    db.close().await;
}

#[tokio::test]
async fn paginates_without_duplicates_searches_unicode_and_keeps_queued_files() {
    let (_temp, db, root) = fixture().await;

    let source = activate_source(db.pool(), &root, 1).await.unwrap();

    let mut batch = Vec::new();

    for index in 0..501 {
        batch.push(prepared(&root, &format!("ЗОБРАЖЕННЯ {index}.mp4"), 10));

        if batch.len() == BATCH {
            save_batch(db.pool(), source, 1, std::mem::take(&mut batch))
                .await
                .unwrap();
        }
    }
    save_batch(db.pool(), source, 1, batch).await.unwrap();

    let root_name = Some(root.to_string_lossy().into_owned());

    let first = browse(db.pool(), root_name.clone(), None, "")
        .await
        .unwrap();
    assert_eq!(first.total, 501);
    assert_eq!(first.files.len(), 500);

    let cursor = first.files.last().unwrap().id.parse().unwrap();

    let second = browse(db.pool(), root_name.clone(), Some(cursor), "")
        .await
        .unwrap();
    assert_eq!(second.files.len(), 1);
    assert_eq!(second.total, 501);

    let matched = browse(db.pool(), root_name, None, "зображення 500")
        .await
        .unwrap();
    assert_eq!(matched.total, 1);
    assert_eq!(matched.files[0].filename, "ЗОБРАЖЕННЯ 500.mp4");
    sqlx::query("UPDATE media SET needs_distribution=1,availability='missing',stage_search_key=NULL WHERE id=?")
    .bind(cursor)
    .execute(db.pool())
    .await
    .unwrap();

    let queued = browse(db.pool(), None, None, "").await.unwrap();
    assert_eq!(queued.total, 1);
    assert!(queued.files[0].native_redistribution);
    assert_eq!(queued.files[0].availability, "missing");
    db.close().await;
}

#[tokio::test]
async fn walker_streams_bounded_batches_and_ignores_archives() {
    let (_temp, db, root) = fixture().await;

    for index in 0..130 {
        write_bmp(&root.join(format!("file{index}.bmp")), 2);
    }
    std::fs::write(root.join("archive.zip"), b"archive").unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel(1);

    let worker = tauri::async_runtime::spawn_blocking(move || {
        let mut batch = Vec::new();

        let mut errors = 0;
        walk(&root, &root, None, 0, &mut batch, &mut errors, &tx).unwrap();
        tx.blocking_send(WalkMessage::Batch(batch)).unwrap();
        tx.blocking_send(WalkMessage::Complete(errors)).unwrap();
    });

    let mut count = 0;

    let mut batches = 0;

    while let Some(message) = rx.recv().await {
        match message {
            WalkMessage::Batch(files) => {
                assert!(files.len() <= BATCH);
                count += files.len();
                batches += 1;
            }
            WalkMessage::Complete(errors) => assert_eq!(errors, 0),
        }
    }
    worker.await.unwrap();
    assert_eq!(count, 130);
    assert_eq!(batches, 3);
    db.close().await;
}
