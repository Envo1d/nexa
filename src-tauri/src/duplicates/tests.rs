use std::fs;

use image::{Rgb, RgbImage};

use super::fingerprints::{perceptual, read};
use crate::{
    config::{AppConfig, AppPaths, ConfigStore},
    database::Database,
};

#[test]
fn exact_hash_ignores_names_and_detects_different_bytes() {
    let directory = tempfile::tempdir().unwrap();

    fs::write(directory.path().join("first.png"), b"same bytes").unwrap();
    fs::write(directory.path().join("renamed.jpg"), b"same bytes").unwrap();
    fs::write(directory.path().join("other.png"), b"same byteS").unwrap();

    let first = read(directory.path(), "first.png", None, false).unwrap();

    assert_eq!(
        first.0,
        read(directory.path(), "renamed.jpg", None, false)
            .unwrap()
            .0
    );
    assert_ne!(
        first.0,
        read(directory.path(), "other.png", None, false).unwrap().0
    );
    assert!(first.1.is_none());
}

#[test]
fn rejects_stale_revision_missing_and_escaping_paths() {
    let directory = tempfile::tempdir().unwrap();

    fs::write(directory.path().join("file.png"), b"bytes").unwrap();

    assert!(read(directory.path(), "file.png", Some("stale"), false).is_err());
    assert!(read(directory.path(), "missing.png", None, false).is_err());
    assert!(read(directory.path(), "../outside.png", None, false).is_err());
}

#[test]
fn visual_hash_survives_resize_and_brightness_but_rejects_flat_images() {
    let image = RgbImage::from_fn(128, 128, |x, y| {
        let value = ((x * x + y * 11 + x * y) % 180 + 25) as u8;

        Rgb([value, value, value])
    });

    let resized = image::imageops::resize(&image, 256, 256, image::imageops::FilterType::Triangle);

    let brighter = RgbImage::from_fn(128, 128, |x, y| {
        let value = image.get_pixel(x, y)[0] + 20;

        Rgb([value, value, value])
    });

    let original = perceptual(&image).unwrap();

    assert!((original ^ perceptual(&resized).unwrap()).count_ones() <= 8);
    assert!((original ^ perceptual(&brighter).unwrap()).count_ones() <= 8);
    assert!(perceptual(&RgbImage::from_pixel(64, 64, Rgb([80, 80, 80]))).is_none());
}

#[tokio::test]
async fn batches_preserve_metadata_scope_cursor_and_skipped_files() {
    let temp = tempfile::tempdir().unwrap();

    let library = temp.path().join("Library");

    let stage = temp.path().join("Stage");

    let inactive = temp.path().join("Previous");

    for root in [&library, &stage, &inactive] {
        fs::create_dir(root).unwrap();
    }

    let config = ConfigStore::load(AppPaths {
        config_path: temp.path().join("settings.json"),
        database_path: temp.path().join("vault.db"),
        cache_path: temp.path().join("cache"),
        logs_path: temp.path().join("logs"),
    })
    .unwrap();

    config
        .save(AppConfig {
            library_path: Some(library.clone()),
            staging_path: Some(stage.clone()),
            ..AppConfig::default()
        })
        .unwrap();

    let db = Database::open(&temp.path().join("vault.db")).await.unwrap();

    for (id, kind, root) in [
        (1, "library", &library),
        (2, "stage", &stage),
        (3, "library", &inactive),
    ] {
        sqlx::query("INSERT INTO sources(id,kind,root_path,path_key,is_active,created_at) VALUES(?,?,?,?,?,1)")
            .bind(id)
            .bind(kind)
            .bind(root.to_str().unwrap())
            .bind(root.to_str().unwrap())
            .bind(i64::from(id != 3))
            .execute(db.pool())
            .await
            .unwrap();
    }

    for id in 1..=18 {
        let source = if id == 3 {
            3
        } else if id % 2 == 0 {
            2
        } else {
            1
        };

        let root = match source {
            3 => &inactive,
            2 => &stage,
            _ => &library,
        };

        let filename = format!("file-{id}.mp4");

        if id != 4 {
            fs::write(root.join(&filename), b"shared video bytes").unwrap();
        }

        sqlx::query(concat!(
            "INSERT INTO media(id,source_id,relative_path,path_key,original_filename,kind,extension,",
            "last_seen_at,created_at,updated_at) VALUES(?,?,?,?,?,'video','mp4',1,1,1)",
        )).bind(id).bind(source).bind(&filename).bind(&filename).bind(&filename)
            .execute(db.pool()).await.unwrap();
    }

    sqlx::raw_sql(concat!(
        "INSERT INTO artists VALUES(1,'Artist','artist'); UPDATE media SET artist_id=1 WHERE id=1;",
        "INSERT INTO copyrights VALUES(1,'Series','series'); INSERT INTO media_copyrights VALUES(1,1);",
        "INSERT INTO characters VALUES(1,1,'Hero','hero'); INSERT INTO media_characters VALUES(1,1,1);",
        "INSERT INTO tags VALUES(1,'Review','review'); INSERT INTO media_tags VALUES(1,1);",
        "INSERT INTO video_metadata(media_id,title) VALUES(1,'Original');",
        "UPDATE media SET file_revision='stale' WHERE id=5;",
        "UPDATE media SET availability='missing' WHERE id=6;",
    )).execute(db.pool()).await.unwrap();

    let inventory =
        serde_json::to_value(super::inventory::prepare(&db, &config).await.unwrap()).unwrap();

    assert_eq!(inventory["total"], 16);
    assert_eq!(inventory["upperId"], "18");

    let first = super::batch(0, None, &db, &config, Some(17), None)
        .await
        .unwrap();

    assert!(first.more);
    assert_eq!(first.after, "17");
    assert_eq!(first.skipped, 2);
    assert_eq!(first.processed, 15);
    assert_eq!(first.files.len(), 13);
    assert_eq!(first.files[0].artist, "Artist");
    assert_eq!(first.files[0].copyrights, ["Series"]);
    assert_eq!(first.files[0].characters, ["Hero (Series)"]);
    assert_eq!(first.files[0].tags, ["Review"]);
    assert_eq!(first.files[0].video_title.as_deref(), Some("Original"));
    assert!(first.files[0].main);
    assert!(!first.files[1].main);
    assert_eq!(first.files[0].sha256, first.files[1].sha256);
    assert!(first.files[0].technical.revision.is_some());

    let second = super::batch(17, Some(first.scope.clone()), &db, &config, None, None)
        .await
        .unwrap();

    assert!(!second.more);
    assert_eq!(second.files.len(), 1);
    assert_eq!(second.after, "18");
    assert_eq!(second.processed, 1);

    let bounded = super::batch(17, None, &db, &config, Some(17), None)
        .await
        .unwrap();

    assert!(bounded.files.is_empty());
    assert_eq!(bounded.processed, 0);
    assert!(
        super::batch(0, Some("old scope".into()), &db, &config, None, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn similarity_rejections_are_symmetric_durable_and_reversible() {
    let temp = tempfile::tempdir().unwrap();

    let path = temp.path().join("vault.db");

    let db = Database::open(&path).await.unwrap();

    let left = "a".repeat(64);

    let right = "b".repeat(64);

    super::review::set(db.pool(), right.clone(), left.clone(), true)
        .await
        .unwrap();
    super::review::set(db.pool(), left.clone(), right.clone(), true)
        .await
        .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM duplicate_rejections")
        .fetch_one(db.pool())
        .await
        .unwrap();

    assert_eq!(count, 1);
    assert!(
        super::review::set(db.pool(), left.clone(), left.clone(), true)
            .await
            .is_err()
    );
    assert!(
        super::review::set(db.pool(), "invalid".into(), right.clone(), true)
            .await
            .is_err()
    );
    db.close().await;

    let db = Database::open(&path).await.unwrap();

    let pair: (String, String) =
        sqlx::query_as("SELECT left_hash,right_hash FROM duplicate_rejections")
            .fetch_one(db.pool())
            .await
            .unwrap();

    assert_eq!(pair, (left.clone(), right.clone()));
    super::review::set(db.pool(), right, left, false)
        .await
        .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM duplicate_rejections")
        .fetch_one(db.pool())
        .await
        .unwrap();

    assert_eq!(count, 0);
}

async fn persistent_fixture() -> (tempfile::TempDir, Database, ConfigStore) {
    let temp = tempfile::tempdir().unwrap();

    let root = temp.path().join("Library");

    fs::create_dir(&root).unwrap();

    let config = ConfigStore::load(AppPaths {
        config_path: temp.path().join("settings.json"),
        database_path: temp.path().join("vault.db"),
        cache_path: temp.path().join("cache"),
        logs_path: temp.path().join("logs"),
    })
    .unwrap();

    config
        .save(AppConfig {
            library_path: Some(root.clone()),
            ..AppConfig::default()
        })
        .unwrap();

    let db = Database::open(&temp.path().join("vault.db")).await.unwrap();

    sqlx::query(
        "INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(1,'library',?,?,1)",
    )
    .bind(root.to_str().unwrap())
    .bind(root.to_str().unwrap())
    .execute(db.pool())
    .await
    .unwrap();

    for id in 1..=2 {
        let filename = format!("file-{id}.mp4");

        fs::write(root.join(&filename), b"identical content").unwrap();

        sqlx::query(concat!(
            "INSERT INTO media(id,source_id,relative_path,path_key,original_filename,kind,extension,last_seen_at,created_at,updated_at) ",
            "VALUES(?,1,?,?,?,'video','mp4',1,1,1)",
        )).bind(id).bind(&filename).bind(&filename).bind(&filename).execute(db.pool()).await.unwrap();
    }

    (temp, db, config)
}

fn saved_checkpoint(
    scope: String,
    generation: i64,
    files: &[super::File],
) -> super::snapshots::Checkpoint {
    super::snapshots::Checkpoint {
        scope,
        generation,
        status: "complete".into(),
        report: super::snapshots::Report {
            total: 2,
            processed: 2,
            checked: 2,
            hashed: 2,
            ..Default::default()
        },
        pairs: vec![super::snapshots::Pair {
            left_id: files[0].technical.id.clone(),
            right_id: files[1].technical.id.clone(),
            left_hash: files[0].sha256.clone(),
            right_hash: files[1].sha256.clone(),
            exact: true,
            distance: 0,
        }],
    }
}

#[tokio::test]
async fn fingerprints_and_results_survive_reopen_and_unchanged_updates_skip_hashing() {
    let (temp, db, config) = persistent_fixture().await;

    let first = super::batch(0, None, &db, &config, None, None)
        .await
        .unwrap();

    assert_eq!(first.hashed, 2);
    assert_eq!(first.reused, 0);

    let settings = config.snapshot().unwrap().settings;

    let scope = super::snapshots::scope(&settings).unwrap();

    let generation = super::snapshots::begin(db.pool(), &scope, 2).await.unwrap();

    super::snapshots::save(db.pool(), saved_checkpoint(scope, generation, &first.files))
        .await
        .unwrap();
    db.close().await;

    let db = Database::open(&temp.path().join("vault.db")).await.unwrap();

    let loaded =
        serde_json::to_value(super::snapshots::load(&db, &settings).await.unwrap()).unwrap();

    assert_eq!(loaded["pairs"].as_array().unwrap().len(), 1);
    assert_eq!(loaded["snapshot"]["status"], "complete");
    assert_eq!(loaded["files"].as_array().unwrap().len(), 2);

    let second = super::batch(0, None, &db, &config, None, None)
        .await
        .unwrap();

    assert_eq!(second.reused, 2);
    assert_eq!(second.hashed, 0);
    assert_eq!(first.files[0].sha256, second.files[0].sha256);

    let root = settings.library_path.as_ref().unwrap();

    fs::rename(root.join("file-1.mp4"), root.join("renamed.mp4")).unwrap();
    sqlx::query("UPDATE media SET relative_path='renamed.mp4',path_key='renamed.mp4' WHERE id=1")
        .execute(db.pool())
        .await
        .unwrap();

    let renamed = super::batch(0, None, &db, &config, None, None)
        .await
        .unwrap();

    assert_eq!(renamed.reused, 2);
    assert_eq!(renamed.hashed, 0);

    fs::write(
        root.join("file-2.mp4"),
        b"new changed content with different length",
    )
    .unwrap();

    let stale =
        serde_json::to_value(super::snapshots::load(&db, &settings).await.unwrap()).unwrap();

    assert!(stale["pairs"].as_array().unwrap().is_empty());
    assert_eq!(stale["stale"], 1);

    let changed = super::batch(0, None, &db, &config, None, None)
        .await
        .unwrap();

    assert_eq!(changed.hashed, 1);
    assert_eq!(changed.reused, 1);
    assert_ne!(changed.files[0].sha256, changed.files[1].sha256);

    fs::write(root.join("new.mp4"), b"newly indexed file").unwrap();
    sqlx::query(concat!(
        "INSERT INTO media(id,source_id,relative_path,path_key,original_filename,kind,extension,last_seen_at,created_at,updated_at) ",
        "VALUES(3,1,'new.mp4','new.mp4','new.mp4','video','mp4',1,1,1)",
    )).execute(db.pool()).await.unwrap();

    let added = super::batch(0, None, &db, &config, None, None)
        .await
        .unwrap();

    assert_eq!(added.hashed, 1);
    assert_eq!(added.reused, 2);

    sqlx::query("DELETE FROM media WHERE id=2")
        .execute(db.pool())
        .await
        .unwrap();

    assert!(super::cache::get(db.pool(), 2).await.unwrap().is_none());

    let removed =
        serde_json::to_value(super::snapshots::load(&db, &settings).await.unwrap()).unwrap();

    assert!(removed["pairs"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn snapshots_reject_old_generations_preserve_empty_and_partial_runs_and_isolate_folders() {
    let (temp, db, config) = persistent_fixture().await;

    let page = super::batch(0, None, &db, &config, None, None)
        .await
        .unwrap();

    let settings = config.snapshot().unwrap().settings;

    let scope = super::snapshots::scope(&settings).unwrap();

    let old = super::snapshots::begin(db.pool(), &scope, 2).await.unwrap();

    let current = super::snapshots::begin(db.pool(), &scope, 2).await.unwrap();

    assert!(
        super::snapshots::save(db.pool(), saved_checkpoint(scope.clone(), old, &page.files))
            .await
            .is_err()
    );

    let mut checkpoint = saved_checkpoint(scope.clone(), current, &page.files);

    checkpoint.status = "partial".into();
    super::snapshots::save(db.pool(), checkpoint).await.unwrap();

    let partial =
        serde_json::to_value(super::snapshots::load(&db, &settings).await.unwrap()).unwrap();

    assert_eq!(partial["snapshot"]["status"], "partial");

    let mut empty = saved_checkpoint(scope, current, &page.files);

    empty.pairs.clear();
    super::snapshots::save(db.pool(), empty).await.unwrap();

    let loaded =
        serde_json::to_value(super::snapshots::load(&db, &settings).await.unwrap()).unwrap();

    assert!(loaded["pairs"].as_array().unwrap().is_empty());
    assert_eq!(loaded["snapshot"]["status"], "complete");

    let mut other = settings;

    other.library_path = Some(temp.path().join("Other Library"));

    let loaded = serde_json::to_value(super::snapshots::load(&db, &other).await.unwrap()).unwrap();

    assert!(loaded["snapshot"].is_null());

    sqlx::query("UPDATE duplicate_fingerprints SET algorithm_version=0")
        .execute(db.pool())
        .await
        .unwrap();

    let upgraded = super::batch(0, None, &db, &config, None, None)
        .await
        .unwrap();

    assert_eq!(upgraded.hashed, 2);
    assert_eq!(upgraded.reused, 0);
}

#[tokio::test]
async fn restored_visual_pairs_respect_rejections_and_current_dimensions() {
    let (_temp, db, config) = persistent_fixture().await;

    let root = config.snapshot().unwrap().settings.library_path.unwrap();

    fs::write(root.join("file-2.mp4"), b"different bytes").unwrap();

    let page = super::batch(0, None, &db, &config, None, None)
        .await
        .unwrap();

    sqlx::raw_sql(concat!(
        "UPDATE media SET kind='image',width=100,height=100;",
        "UPDATE duplicate_fingerprints SET kind='image',visual='0000000000000000',visual_retry=0;",
    ))
    .execute(db.pool())
    .await
    .unwrap();

    let settings = config.snapshot().unwrap().settings;

    let scope = super::snapshots::scope(&settings).unwrap();

    let generation = super::snapshots::begin(db.pool(), &scope, 2).await.unwrap();

    let mut checkpoint = saved_checkpoint(scope.clone(), generation, &page.files);

    checkpoint.pairs[0].exact = false;
    super::snapshots::save(db.pool(), checkpoint).await.unwrap();

    let loaded =
        serde_json::to_value(super::snapshots::load(&db, &settings).await.unwrap()).unwrap();

    assert_eq!(loaded["pairs"].as_array().unwrap().len(), 1);
    super::review::set(
        db.pool(),
        page.files[0].sha256.clone(),
        page.files[1].sha256.clone(),
        true,
    )
    .await
    .unwrap();

    let rejected =
        serde_json::to_value(super::snapshots::load(&db, &settings).await.unwrap()).unwrap();

    assert!(rejected["pairs"].as_array().unwrap().is_empty());
    super::review::set(
        db.pool(),
        page.files[0].sha256.clone(),
        page.files[1].sha256.clone(),
        false,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE media SET height=300 WHERE id=2")
        .execute(db.pool())
        .await
        .unwrap();

    let reshaped =
        serde_json::to_value(super::snapshots::load(&db, &settings).await.unwrap()).unwrap();

    assert!(reshaped["pairs"].as_array().unwrap().is_empty());
}
