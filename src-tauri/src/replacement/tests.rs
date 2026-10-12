use std::{fs, path::PathBuf};

use super::*;
use super::{
    queries::candidates,
    workflow::{Operation, backup, begin, finish, preserve},
};
use crate::database::Database;
use crate::naming::name_key;

struct Fixture {
    temp: tempfile::TempDir,
    db: Database,
    settings: AppConfig,
}

impl Fixture {
    fn old(&self) -> PathBuf {
        self.settings.library_path.as_ref().unwrap().join("old.mp4")
    }

    fn incoming(&self) -> PathBuf {
        self.settings
            .staging_path
            .as_ref()
            .unwrap()
            .join("full.webm")
    }

    fn backups(&self) -> PathBuf {
        self.temp.path().join("backups")
    }

    async fn request(&self) -> Request {
        Request {
            main_id: "1".into(),
            stage_id: "2".into(),
            main_token: token(&record(self.db.pool(), 1).await.unwrap()),
            stage_token: token(&record(self.db.pool(), 2).await.unwrap()),
            sync_with_disk: self.settings.sync_structure,
        }
    }
}

async fn fixture(sync: bool) -> Fixture {
    let temp = tempfile::tempdir().unwrap();

    let library = temp.path().join("Library");

    let stage = temp.path().join("Stage");
    fs::create_dir(&library).unwrap();
    fs::create_dir(&stage).unwrap();
    fs::write(library.join("old.mp4"), b"old partial video").unwrap();
    fs::write(stage.join("full.webm"), b"complete new video contents").unwrap();

    let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
    sqlx::query(concat!(
        "INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(1,'library',?,?,1),",
        "(2,'stage',?,?,1)",
    ))
    .bind(library.to_str().unwrap())
    .bind(library.to_str().unwrap())
    .bind(stage.to_str().unwrap())
    .bind(stage.to_str().unwrap())
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::raw_sql(concat!(
        "INSERT INTO artists VALUES(1,'Artist','artist'); INSERT INTO tags VALUES(1,'Keep',",
        "'keep'); INSERT INTO copyrights VALUES(1,'Copyright','copyright'); INSERT INTO ",
        "characters VALUES(1,1,'Character','character');",
    ))
    .execute(db.pool())
    .await
    .unwrap();

    for (id, source, path, filename, width) in [
        (1, 1, library.join("old.mp4"), "old.mp4", 1280),
        (2, 2, stage.join("full.webm"), "full.webm", 1920),
    ] {
        sqlx::query(concat!(
            "INSERT INTO media(id,source_id,relative_path,path_key,original_filename,kind,",
            "extension,size_bytes,width,height,file_revision,metadata_state,metadata_version,",
            "last_seen_at,created_at,updated_at,stage_search_key) VALUES(?,?,?,?,?,'video',?,?,?,?,",
            "?,'ready',1,1,1,1,?)",
        ))
.bind(id)
.bind(source)
.bind(filename)
.bind(filename)
.bind(filename)
.bind(if id==1 {"mp4"} else {"webm"})
.bind(fs::metadata(&path)
.unwrap().len() as i64)
.bind(width)
.bind(1080)
.bind(crate::disk_sync::revision(&path)
.unwrap())
.bind(name_key(filename))
.execute(db.pool())
.await
.unwrap();
    }
    sqlx::raw_sql(concat!(
        "UPDATE media SET artist_id=1,favorite=1,naming_order_key='group',naming_order=3 WHERE ",
        "id=1; INSERT INTO media_tags VALUES(1,1); INSERT INTO media_copyrights VALUES(1,1); ",
        "INSERT INTO media_characters VALUES(1,1,1); INSERT INTO video_metadata(media_id,title,",
        "duration_ms,codec) VALUES(1,'Keep title',1000,'old'),(2,'Incoming title',8000,'new');",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    Fixture {
        temp,
        db,
        settings: AppConfig {
            library_path: Some(library),
            staging_path: Some(stage),
            sync_structure: sync,
            ..Default::default()
        },
    }
}

async fn pending(f: &Fixture) -> Operation {
    let a = record(f.db.pool(), 1).await.unwrap();

    let b = record(f.db.pool(), 2).await.unwrap();
    sqlx::query(concat!(
        "INSERT INTO media_replacements(main_id,stage_id,old_source,old_root,old_relative,",
        "old_revision,new_source,new_root,new_relative,new_revision,incoming_filename,",
        "library_root,backup_root,delete_old) VALUES(1,2,1,?,'old.mp4',?,2,?,'full.webm',?,",
        "'full.webm',?,?,1)",
    ))
    .bind(a.get::<String, _>("root_path"))
    .bind(a.get::<Option<String>, _>("file_revision"))
    .bind(b.get::<String, _>("root_path"))
    .bind(b.get::<Option<String>, _>("file_revision"))
    .bind(root(&f.settings).unwrap())
    .bind(
        f.settings
            .sync_structure
            .then(|| f.backups().to_string_lossy().into_owned()),
    )
    .execute(f.db.pool())
    .await
    .unwrap();
    sqlx::query_as("SELECT * FROM media_replacements ORDER BY id DESC LIMIT 1")
        .fetch_one(f.db.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn replacement_retains_main_identity_annotations_order_and_refreshes_technical_fields() {
    let f = fixture(false).await;

    let before = token(&record(f.db.pool(), 1).await.unwrap());

    let report = begin(f.db.pool(), &f.settings, &f.backups(), f.request().await)
        .await
        .unwrap();
    assert!(report.old_file_deleted);
    assert!(!f.old().exists());
    assert_eq!(
        fs::read(f.incoming()).unwrap(),
        b"complete new video contents"
    );

    let row = record(f.db.pool(), 1).await.unwrap();
    assert_ne!(token(&row), before);
    assert_eq!(row.get::<i64, _>("source_id"), 2);
    assert_eq!(row.get::<String, _>("original_filename"), "old.mp4");
    assert_eq!(row.get::<String, _>("extension"), "webm");
    assert_eq!(row.get::<i64, _>("width"), 1920);
    assert_eq!(row.get::<i64, _>("favorite"), 1);
    assert_eq!(row.get::<i64, _>("artist_id"), 1);
    assert_eq!(row.get::<String, _>("title"), "Keep title");
    assert_eq!(row.get::<i64, _>("duration_ms"), 8000);
    assert_eq!(row.get::<String, _>("codec"), "new");
    assert_eq!(row.get::<i64, _>("naming_order"), 3);
    assert_eq!(row.get::<String, _>("naming_order_key"), "group");

    let links: i64 = sqlx::query_scalar(concat!(
        "SELECT (SELECT count(*) FROM media_tags WHERE media_id=1)+(SELECT count(*) FROM ",
        "media_characters WHERE media_id=1)+(SELECT count(*) FROM media_copyrights WHERE ",
        "media_id=1)",
    ))
    .fetch_one(f.db.pool())
    .await
    .unwrap();
    assert_eq!(links, 3);
    assert!(record(f.db.pool(), 2).await.is_err());
    assert_eq!(
        stage::browse(
            f.db.pool(),
            Some(
                f.settings
                    .staging_path
                    .as_ref()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            ),
            None,
            ""
        )
        .await
        .unwrap()
        .total,
        0
    );
    assert_eq!(
        candidates(f.db.pool(), &f.settings, "video", "", 0)
            .await
            .unwrap()
            .files
            .len(),
        1
    );
    f.db.close().await;

    let reopened = Database::open(&f.temp.path().join("vault.db"))
        .await
        .unwrap();
    recover(reopened.pool()).await.unwrap();
    assert_eq!(
        record(reopened.pool(), 1)
            .await
            .unwrap()
            .get::<String, _>("title"),
        "Keep title"
    );
    reopened.close().await;
}

#[tokio::test]
async fn enabled_sync_removes_old_file_and_temporary_backup_after_commit() {
    let f = fixture(true).await;

    let report = begin(f.db.pool(), &f.settings, &f.backups(), f.request().await)
        .await
        .unwrap();
    assert!(!f.old().exists());
    assert!(report.old_file_deleted);
    assert_eq!(fs::read_dir(f.backups()).unwrap().count(), 0);
    assert_eq!(
        fs::read(f.incoming()).unwrap(),
        b"complete new video contents"
    );
    f.db.close().await;
}

#[tokio::test]
async fn cleanup_keeps_original_and_temporary_backup_until_adopted_file_is_available() {
    let f = fixture(true).await;

    let op = pending(&f).await;
    preserve(&op).unwrap();
    // Crash boundary: adoption committed, filesystem cleanup has not run.
    let mut tx = f.db.pool().begin().await.unwrap();
    sqlx::query("DELETE FROM media WHERE id=2")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(concat!(
        "UPDATE media SET source_id=2,relative_path='full.webm',path_key='full.webm',",
        "file_revision=? WHERE id=1",
    ))
    .bind(&op.new_revision)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("UPDATE media_replacements SET committed=1")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let moved = f.temp.path().join("held.webm");
    fs::rename(f.incoming(), &moved).unwrap();
    assert!(recover(f.db.pool()).await.is_err());
    assert_eq!(fs::read(f.old()).unwrap(), b"old partial video");
    assert_eq!(
        fs::read(backup(&op).unwrap()).unwrap(),
        b"old partial video"
    );
    fs::rename(moved, f.incoming()).unwrap();
    f.db.close().await;

    let db = Database::open(&f.temp.path().join("vault.db"))
        .await
        .unwrap();
    recover(db.pool()).await.unwrap();
    assert!(!f.old().exists());
    assert!(!backup(&op).unwrap().exists());
    assert_eq!(
        fs::read(f.incoming()).unwrap(),
        b"complete new video contents"
    );
    // Completion is also recoverable after both cleanup deletions.
    sqlx::query("UPDATE media_replacements SET finished=0")
        .execute(db.pool())
        .await
        .unwrap();
    recover(db.pool()).await.unwrap();
    db.close().await;
}

#[tokio::test]
async fn existing_retention_policy_does_not_retroactively_delete_old_versions() {
    let f = fixture(false).await;
    pending(&f).await;
    sqlx::query("UPDATE media_replacements SET delete_old=0")
        .execute(f.db.pool())
        .await
        .unwrap();
    recover(f.db.pool()).await.unwrap();
    assert_eq!(fs::read(f.old()).unwrap(), b"old partial video");
    assert_eq!(
        record(f.db.pool(), 1)
            .await
            .unwrap()
            .get::<i64, _>("source_id"),
        2
    );
    f.db.close().await;
}

#[tokio::test]
async fn replaces_images_across_formats_without_creating_video_metadata() {
    let f = fixture(false).await;

    let old = f.settings.library_path.as_ref().unwrap().join("old.png");

    let new = f.settings.staging_path.as_ref().unwrap().join("new.webp");
    fs::rename(f.old(), &old).unwrap();
    fs::rename(f.incoming(), &new).unwrap();
    sqlx::raw_sql(concat!(
        "DELETE FROM video_metadata; UPDATE media SET kind='image',extension='png',",
        "relative_path='old.png',path_key='old.png',original_filename='old.png' WHERE id=1; ",
        "UPDATE media SET kind='image',extension='webp',relative_path='new.webp',",
        "path_key='new.webp',original_filename='new.webp' WHERE id=2;",
    ))
    .execute(f.db.pool())
    .await
    .unwrap();
    begin(f.db.pool(), &f.settings, &f.backups(), f.request().await)
        .await
        .unwrap();

    let row = record(f.db.pool(), 1).await.unwrap();
    assert_eq!(row.get::<String, _>("kind"), "image");
    assert_eq!(row.get::<String, _>("extension"), "webp");
    assert_eq!(row.get::<String, _>("original_filename"), "old.png");
    assert_eq!(row.get::<i64, _>("width"), 1920);

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM video_metadata")
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(!old.exists());
    assert!(new.exists());
    f.db.close().await;
}

#[tokio::test]
async fn changed_incoming_abandons_uncommitted_recovery_without_blocking_scans() {
    let f = fixture(false).await;
    pending(&f).await;
    fs::write(f.incoming(), b"changed after intent").unwrap();
    recover(f.db.pool()).await.unwrap();
    assert_eq!(
        record(f.db.pool(), 1)
            .await
            .unwrap()
            .get::<i64, _>("source_id"),
        1
    );
    assert!(record(f.db.pool(), 2).await.is_ok());

    let row = sqlx::query("SELECT committed,finished FROM media_replacements")
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("committed"), 0);
    assert_eq!(row.get::<i64, _>("finished"), 1);
    assert_eq!(fs::read(f.old()).unwrap(), b"old partial video");
    f.db.close().await;
}

#[tokio::test]
async fn rejects_stale_tokens_settings_kinds_and_changed_bytes_without_retiring_stage() {
    let f = fixture(false).await;

    let mut request = f.request().await;
    request.main_token = "stale".into();
    assert!(
        begin(f.db.pool(), &f.settings, &f.backups(), request)
            .await
            .is_err()
    );

    let mut request = f.request().await;
    request.sync_with_disk = true;
    assert!(
        begin(f.db.pool(), &f.settings, &f.backups(), request)
            .await
            .is_err()
    );

    let request = f.request().await;
    fs::write(f.incoming(), b"changed source").unwrap();
    assert!(
        begin(f.db.pool(), &f.settings, &f.backups(), request)
            .await
            .is_err()
    );
    sqlx::raw_sql(
        "DELETE FROM video_metadata WHERE media_id=2; UPDATE media SET kind='image' WHERE id=2",
    )
    .execute(f.db.pool())
    .await
    .unwrap();
    assert!(
        begin(f.db.pool(), &f.settings, &f.backups(), f.request().await)
            .await
            .is_err()
    );
    assert!(record(f.db.pool(), 2).await.is_ok());

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM media_replacements")
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(fs::read(f.old()).unwrap(), b"old partial video");
    f.db.close().await;
}

#[tokio::test]
async fn recovers_intent_and_existing_backup_after_restart_without_duplicate_records() {
    for backed_up in [false, true] {
        let f = fixture(true).await;

        let op = pending(&f).await;

        if backed_up {
            preserve(&op).unwrap();
        }
        f.db.close().await;

        let db = Database::open(&f.temp.path().join("vault.db"))
            .await
            .unwrap();
        recover(db.pool()).await.unwrap();
        recover(db.pool()).await.unwrap();
        assert!(!f.old().exists());
        assert!(!backup(&op).unwrap().exists());
        assert!(record(db.pool(), 2).await.is_err());
        assert_eq!(
            record(db.pool(), 1)
                .await
                .unwrap()
                .get::<i64, _>("source_id"),
            2
        );
        db.close().await;
    }
}

#[tokio::test]
async fn failed_database_commit_rolls_back_stage_retirement_and_keeps_originals() {
    let f = fixture(true).await;
    sqlx::raw_sql(concat!(
        "CREATE TRIGGER reject_replacement BEFORE UPDATE OF source_id ON media WHEN OLD.id=1 ",
        "BEGIN SELECT RAISE(ABORT,'test commit failure'); END;",
    ))
    .execute(f.db.pool())
    .await
    .unwrap();
    assert!(
        begin(f.db.pool(), &f.settings, &f.backups(), f.request().await)
            .await
            .is_err()
    );
    assert!(record(f.db.pool(), 2).await.is_ok());
    assert_eq!(
        record(f.db.pool(), 1)
            .await
            .unwrap()
            .get::<i64, _>("source_id"),
        1
    );
    assert_eq!(fs::read(f.old()).unwrap(), b"old partial video");

    let finished: i64 = sqlx::query_scalar("SELECT finished FROM media_replacements")
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    assert_eq!(finished, 1);
    f.db.close().await;
}

#[tokio::test]
async fn committed_cleanup_failure_recovers_after_reopen_and_preserves_changed_old_file() {
    let f = fixture(true).await;

    let op = pending(&f).await;

    let old_time = fs::metadata(f.old()).unwrap().modified().unwrap();
    finish(f.db.pool(), op.clone()).await.unwrap();
    // Restore the transient copy to represent the postcommit cleanup boundary.
    let temporary = backup(&op).unwrap();
    fs::write(&temporary, b"old partial video").unwrap();
    fs::File::options()
        .write(true)
        .open(&temporary)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(old_time))
        .unwrap();
    // Simulate a crash just after commit, with the old file still present.
    fs::write(f.old(), b"externally changed old file").unwrap();
    sqlx::query("UPDATE media_replacements SET finished=0")
        .execute(f.db.pool())
        .await
        .unwrap();
    assert!(recover(f.db.pool()).await.is_err());
    assert_eq!(fs::read(f.old()).unwrap(), b"externally changed old file");
    assert_eq!(fs::read(&temporary).unwrap(), b"old partial video");
    fs::remove_file(f.old()).unwrap();
    f.db.close().await;

    let db = Database::open(&f.temp.path().join("vault.db"))
        .await
        .unwrap();
    recover(db.pool()).await.unwrap();
    assert!(!temporary.exists());
    assert_eq!(
        record(db.pool(), 1)
            .await
            .unwrap()
            .get::<i64, _>("source_id"),
        2
    );
    db.close().await;
}

#[tokio::test]
async fn picker_searches_metadata_and_pages_full_main_independent_of_stage() {
    let f = fixture(false).await;

    for search in ["Artist", "Character", "Keep", "Copyright"] {
        assert_eq!(
            candidates(f.db.pool(), &f.settings, "video", search, 0)
                .await
                .unwrap()
                .files
                .len(),
            1,
            "{search}"
        );
    }
    assert!(
        candidates(f.db.pool(), &f.settings, "video", "full", 0)
            .await
            .unwrap()
            .files
            .is_empty()
    );
    assert!(
        candidates(f.db.pool(), &f.settings, "image", "", 0)
            .await
            .unwrap()
            .files
            .is_empty()
    );

    for value in 3..=56 {
        let filename = format!("item-{value}.mp4");
        sqlx::query(concat!(
            "INSERT INTO media(id,source_id,relative_path,path_key,original_filename,kind,",
            "extension,last_seen_at,created_at,updated_at) VALUES(?,1,?,?,?,'video','mp4',1,1,1)",
        ))
        .bind(value)
        .bind(&filename)
        .bind(&filename)
        .bind(&filename)
        .execute(f.db.pool())
        .await
        .unwrap();
    }

    let first = candidates(f.db.pool(), &f.settings, "video", "", 0)
        .await
        .unwrap();
    assert_eq!(first.files.len(), 50);

    let second = candidates(
        f.db.pool(),
        &f.settings,
        "video",
        "",
        id(first.next.as_deref().unwrap()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(second.files.len(), 5);
    assert!(second.next.is_none());
    sqlx::query("UPDATE artists SET name='Тестовый Художник',name_key=? WHERE id=1")
        .bind(name_key("Тестовый Художник"))
        .execute(f.db.pool())
        .await
        .unwrap();
    assert_eq!(
        candidates(
            f.db.pool(),
            &f.settings,
            "video",
            "тестовый художник; CHARACTER",
            0
        )
        .await
        .unwrap()
        .files
        .len(),
        1
    );
    f.db.close().await;
}

#[tokio::test]
async fn replacement_metadata_search_matches_main_and_requires_all_terms_on_one_file() {
    let f = fixture(false).await;
    sqlx::raw_sql(concat!(
        "INSERT INTO artists VALUES(2,'Other','other'); INSERT INTO media(id,source_id,",
        "relative_path,path_key,original_filename,kind,extension,artist_id,last_seen_at,",
        "created_at,updated_at) VALUES(3,1,'artist-only.mp4','artist-only.mp4',",
        "'artist-only.mp4','video','mp4',1,1,1,1),(4,1,'character-only.mp4',",
        "'character-only.mp4','character-only.mp4','video','mp4',2,1,1,1); INSERT INTO ",
        "media_copyrights VALUES(4,1); INSERT INTO media_characters VALUES(4,1,1);",
    ))
    .execute(f.db.pool())
    .await
    .unwrap();

    for (search, expected) in [
        ("Artist; Character", 1),
        (" artist ; CHARACTER; Keep; Copyright ", 1),
        ("Artist;Artist; ;Character;", 1),
        ("Artist; Other", 0),
        ("Art", 0),
        ("old", 0),
        ("Keep title", 0),
        ("Other;Keep", 0),
        ("Artist", 2),
        ("Character", 2),
        (" ; ; ", 3),
    ] {
        let result = candidates(f.db.pool(), &f.settings, "video", search, 0)
            .await
            .unwrap();
        assert_eq!(result.files.len(), expected, "{search}");

        let filters = serde_json::from_value(serde_json::json!({
            "scope": "gallery",
            "search": search,
            "kind": "video",
            "sort": "newest",
            "tagged": false,
            "offset": 0,
        }))
        .unwrap();

        let main = crate::library::browse(f.db.pool(), Some(root(&f.settings).unwrap()), filters)
            .await
            .unwrap();

        let main = serde_json::to_value(main).unwrap();
        assert_eq!(
            main["total"].as_u64().unwrap() as usize,
            expected,
            "Main comparison: {search}"
        );
    }
    f.db.close().await;
}
