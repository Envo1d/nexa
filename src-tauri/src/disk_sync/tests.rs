use super::ordering::{find_order_group, persist_order};
use super::*;
use crate::config::AppConfig;

async fn fixture() -> (tempfile::TempDir, Database, String, String) {
    let temp = tempfile::tempdir().unwrap();

    let library = temp.path().join("Library");

    let stage = temp.path().join("Stage");
    fs::create_dir(&library).unwrap();
    fs::create_dir(&stage).unwrap();

    let db = Database::open(&temp.path().join("vault.db")).await.unwrap();

    for (id, kind, root) in [(1, "library", &library), (2, "stage", &stage)] {
        sqlx::query("INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(?,?,?,?,1)")
            .bind(id)
            .bind(kind)
            .bind(root.to_str())
            .bind(root.to_str())
            .execute(db.pool())
            .await
            .unwrap();
    }
    sqlx::raw_sql(concat!(
        "INSERT INTO artists VALUES(1,'Artist','artist'); INSERT INTO copyrights VALUES(1,",
        "'Zebra','zebra'),(2,'Alpha','alpha'); INSERT INTO characters VALUES(1,1,'Zed','zed'),",
        "(2,2,'Alice','alice');",
    ))
    .execute(db.pool())
    .await
    .unwrap();

    for id in 1..=4 {
        let filename = format!("capture-{id}.png");
        fs::write(stage.join(&filename), format!("independent content {id}")).unwrap();
        sqlx::query(concat!(
            "INSERT INTO media(id,source_id,artist_id,relative_path,path_key,original_filename,",
            "kind,extension,last_seen_at,created_at,updated_at,library_assignment) VALUES(?,2,1,?,",
            "?,?,'image','png',1,1,1,?)",
        ))
        .bind(id)
        .bind(&filename)
        .bind(&filename)
        .bind(&filename)
        .bind(library.to_str())
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query("INSERT INTO media_copyrights VALUES(?,1),(?,2)")
            .bind(id)
            .bind(id)
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("INSERT INTO media_characters VALUES(?,1,1),(?,2,2)")
            .bind(id)
            .bind(id)
            .execute(db.pool())
            .await
            .unwrap();
    }
    (
        temp,
        db,
        library.to_string_lossy().into_owned(),
        stage.to_string_lossy().into_owned(),
    )
}

async fn run(pool: &SqlitePool, root: &str, settings: &AppConfig) {
    let plan = targets(pool, root, settings).await.unwrap();
    execute_plan(pool, 1, root, plan, || true, |_| {})
        .await
        .unwrap();
}

fn indexed(root: &str, index: i64) -> PathBuf {
    Path::new(root)
        .join("Alpha/Artist")
        .join(format!("Alice х Zed [Artist] - {index}.png"))
}

#[tokio::test]
async fn mixed_extensions_share_indices_and_persistent_order_for_images_and_videos() {
    for (kind, extensions, stem, folder) in [
        (
            "image",
            ["png", "jpg", "png"],
            "Alice х Zed [Artist]",
            "Alpha/Artist",
        ),
        (
            "video",
            ["mp4", "webm", "mp4"],
            "Alice х Zed - Clip [Artist]",
            "Alpha/Artist/Videos",
        ),
    ] {
        let (temp, db, root, _) = fixture().await;
        sqlx::query("DELETE FROM media WHERE id=4")
            .execute(db.pool())
            .await
            .unwrap();

        for (index, extension) in extensions.iter().enumerate() {
            sqlx::query("UPDATE media SET kind=?,extension=? WHERE id=?")
                .bind(kind)
                .bind(extension)
                .bind(index as i64 + 1)
                .execute(db.pool())
                .await
                .unwrap();
        }

        if kind == "video" {
            sqlx::query("INSERT INTO video_metadata(media_id,title) SELECT id,'Clip' FROM media")
                .execute(db.pool())
                .await
                .unwrap();
        }

        let settings = AppConfig::default();

        let path = |index: usize, id: usize| {
            Path::new(&root)
                .join(folder)
                .join(format!("{stem} - {index}.{}", extensions[id - 1]))
        };
        run(db.pool(), &root, &settings).await;

        for id in 1..=3 {
            assert_eq!(
                fs::read(path(id, id)).unwrap(),
                format!("independent content {id}").as_bytes()
            );
        }

        let (key, group) = find_order_group(db.pool(), &root, &settings, 2)
            .await
            .unwrap();
        assert_eq!(
            group.iter().map(|file| file.id).collect::<Vec<_>>(),
            [1, 2, 3]
        );

        let expected = ["1", "2", "3"].map(String::from);

        let ordered = ["3", "1", "2"].map(String::from);
        persist_order(db.pool(), &root, &settings, &key, &expected, &ordered)
            .await
            .unwrap();
        db.close().await;

        let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
        run(db.pool(), &root, &settings).await;
        run(db.pool(), &root, &settings).await;

        for (index, id) in [3, 1, 2].into_iter().enumerate() {
            assert_eq!(
                fs::read(path(index + 1, id)).unwrap(),
                format!("independent content {id}").as_bytes()
            );
        }
        assert!(!path(3, 3).exists());
        // A format replacement keeps the saved rank in the stem-based group.
        sqlx::query("UPDATE media SET extension='other' WHERE id=1")
            .execute(db.pool())
            .await
            .unwrap();

        let (updated_key, group) = find_order_group(db.pool(), &root, &settings, 1)
            .await
            .unwrap();
        assert_eq!(updated_key, key);
        assert_eq!(
            group.iter().map(|file| file.id).collect::<Vec<_>>(),
            [3, 1, 2]
        );
    }
}

#[tokio::test]
async fn legacy_extension_orders_merge_deterministically_and_keep_destination_folders_separate() {
    let (_temp, db, root, _) = fixture().await;

    let settings = AppConfig::default();

    for (id, extension, position) in [(1, "png", 1), (2, "jpg", 0), (3, "png", 0)] {
        let relative = name_key(
            &Path::new("Alpha")
                .join("Artist")
                .join(format!("Alice х Zed [Artist].{extension}"))
                .to_string_lossy(),
        );

        let key = serde_json::to_string(&(&root, relative)).unwrap();
        sqlx::query("UPDATE media SET extension=?,naming_order_key=?,naming_order=? WHERE id=?")
            .bind(extension)
            .bind(key)
            .bind(position)
            .bind(id)
            .execute(db.pool())
            .await
            .unwrap();
    }
    sqlx::query("UPDATE media SET extension='gif' WHERE id=4")
        .execute(db.pool())
        .await
        .unwrap();

    let (key, group) = find_order_group(db.pool(), &root, &settings, 1)
        .await
        .unwrap();
    assert_eq!(
        group.iter().map(|file| file.id).collect::<Vec<_>>(),
        [2, 3, 1]
    );

    let (gif_key, group) = find_order_group(db.pool(), &root, &settings, 4)
        .await
        .unwrap();
    assert_ne!(gif_key, key);
    assert_eq!(group.len(), 1);

    let merged = AppConfig {
        gif_subfolder: String::new(),
        ..settings
    };

    let (_, group) = find_order_group(db.pool(), &root, &merged, 4)
        .await
        .unwrap();
    assert_eq!(
        group.iter().map(|file| file.id).collect::<Vec<_>>(),
        [2, 3, 1, 4]
    );
}

#[tokio::test]
async fn video_order_survives_swaps_reopen_and_batch_deletion_without_content_changes() {
    let (temp, db, root, stage) = fixture().await;
    fs::write(
        Path::new(&stage).join("capture-5.png"),
        b"independent content 5",
    )
    .unwrap();
    sqlx::raw_sql(concat!(
        "INSERT INTO media(id,source_id,artist_id,relative_path,path_key,original_filename,",
        "kind,extension,last_seen_at,created_at,updated_at,library_assignment) SELECT 5,",
        "source_id,artist_id,'capture-5.png','capture-5.png','capture-5.png',kind,extension,",
        "last_seen_at,created_at,updated_at,library_assignment FROM media WHERE id=1; INSERT ",
        "INTO media_copyrights VALUES(5,1),(5,2); INSERT INTO media_characters VALUES(5,1,1),",
        "(5,2,2); UPDATE media SET kind='video',extension='mp4'; INSERT INTO ",
        "video_metadata(media_id,title) SELECT id,'Clip' FROM media;",
    ))
    .execute(db.pool())
    .await
    .unwrap();

    let settings = AppConfig::default();
    run(db.pool(), &root, &settings).await;

    let (key, _) = find_order_group(db.pool(), &root, &settings, 3)
        .await
        .unwrap();

    let expected: Vec<String> = (1..=5).map(|id| id.to_string()).collect();

    let ordered: Vec<String> = [1, 3, 2, 4, 5]
        .into_iter()
        .map(|id| id.to_string())
        .collect();
    persist_order(db.pool(), &root, &settings, &key, &expected, &ordered)
        .await
        .unwrap();

    let filename = |index| {
        Path::new(&root)
            .join("Alpha/Artist/Videos")
            .join(format!("Alice х Zed - Clip [Artist] - {index}.mp4"))
    };
    // Saving does not rename on disk; the coordinator applies it later.
    assert_eq!(fs::read(filename(2)).unwrap(), b"independent content 2");
    db.close().await;

    let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
    run(db.pool(), &root, &settings).await;
    run(db.pool(), &root, &settings).await;

    for (index, id) in [1, 3, 2, 4, 5].into_iter().enumerate() {
        assert_eq!(
            fs::read(filename(index + 1)).unwrap(),
            format!("independent content {id}").as_bytes()
        );
    }
    assert!(
        persist_order(db.pool(), &root, &settings, &key, &expected, &expected)
            .await
            .is_err()
    );
    fs::remove_file(filename(2)).unwrap();
    fs::remove_file(filename(3)).unwrap();
    sqlx::query("DELETE FROM media WHERE id IN (2,3)")
        .execute(db.pool())
        .await
        .unwrap();
    run(db.pool(), &root, &settings).await;

    for (index, id) in [1, 4, 5].into_iter().enumerate() {
        assert_eq!(
            fs::read(filename(index + 1)).unwrap(),
            format!("independent content {id}").as_bytes()
        );
    }
    assert!(!filename(4).exists());
    assert!(!filename(5).exists());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM file_operations")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn order_rejects_invalid_or_changed_groups_and_appends_new_members() {
    let (_temp, db, root, _stage) = fixture().await;

    let settings = AppConfig::default();

    let (key, _) = find_order_group(db.pool(), &root, &settings, 1)
        .await
        .unwrap();

    let expected: Vec<String> = (1..=4).map(|id| id.to_string()).collect();

    for invalid in [
        vec!["1", "2", "3"],
        vec!["1", "2", "3", "3"],
        vec!["1", "2", "3", "99"],
    ] {
        let ids = invalid.into_iter().map(String::from).collect::<Vec<_>>();
        assert!(
            persist_order(db.pool(), &root, &settings, &key, &expected, &ids)
                .await
                .is_err()
        );
    }

    let ordered: Vec<String> = [4, 3, 2, 1].into_iter().map(|id| id.to_string()).collect();
    persist_order(db.pool(), &root, &settings, &key, &expected, &ordered)
        .await
        .unwrap();
    sqlx::raw_sql(concat!(
        "INSERT INTO media(id,source_id,artist_id,relative_path,path_key,original_filename,",
        "kind,extension,last_seen_at,created_at,updated_at,library_assignment) SELECT 5,",
        "source_id,artist_id,'new.png','new.png','new.png',kind,extension,last_seen_at,",
        "created_at,updated_at,library_assignment FROM media WHERE id=1; INSERT INTO ",
        "media_copyrights VALUES(5,1),(5,2); INSERT INTO media_characters VALUES(5,1,1),(5,2,",
        "2);",
    ))
    .execute(db.pool())
    .await
    .unwrap();

    let (_, group) = find_order_group(db.pool(), &root, &settings, 1)
        .await
        .unwrap();
    assert_eq!(
        group.iter().map(|file| file.id).collect::<Vec<_>>(),
        [4, 3, 2, 1, 5]
    );
    assert!(
        persist_order(db.pool(), &root, &settings, &key, &ordered, &ordered)
            .await
            .is_err()
    );

    let changed = AppConfig {
        character_separator: " + ".into(),
        ..settings
    };

    let (_, group) = find_order_group(db.pool(), &root, &changed, 1)
        .await
        .unwrap();
    assert_eq!(
        group.iter().map(|file| file.id).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5]
    );
    assert!(
        persist_order(db.pool(), &root, &changed, &key, &expected, &expected)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn sorts_primary_copyright_preserves_contents_and_compacts_single_and_batch_deletions() {
    let (_temp, db, root, stage) = fixture().await;

    let settings = AppConfig::default();
    run(db.pool(), &root, &settings).await;

    for id in 1..=4 {
        assert_eq!(
            fs::read(indexed(&root, id)).unwrap(),
            format!("independent content {id}").as_bytes()
        );
    }
    assert_eq!(fs::read_dir(&stage).unwrap().count(), 0);
    fs::remove_file(indexed(&root, 2)).unwrap();
    sqlx::query("DELETE FROM media WHERE id=2")
        .execute(db.pool())
        .await
        .unwrap();
    run(db.pool(), &root, &settings).await;
    assert_eq!(
        fs::read(indexed(&root, 2)).unwrap(),
        b"independent content 3"
    );
    assert_eq!(
        fs::read(indexed(&root, 3)).unwrap(),
        b"independent content 4"
    );
    assert!(!indexed(&root, 4).exists());
    fs::remove_file(indexed(&root, 1)).unwrap();
    fs::remove_file(indexed(&root, 3)).unwrap();
    sqlx::query("DELETE FROM media WHERE id IN (1,4)")
        .execute(db.pool())
        .await
        .unwrap();
    run(db.pool(), &root, &settings).await;

    let single = Path::new(&root).join("Alpha/Artist/Alice х Zed [Artist].png");
    assert_eq!(fs::read(single).unwrap(), b"independent content 3");
    assert!(!indexed(&root, 2).exists());

    let original: String = sqlx::query_scalar("SELECT original_filename FROM media WHERE id=3")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(original, "capture-3.png");
    db.close().await;
}

#[tokio::test]
async fn pauses_between_operations_and_rebuilds_using_current_folder_settings() {
    let (_temp, db, root, _stage) = fixture().await;

    let settings = AppConfig::default();

    let plan = targets(db.pool(), &root, &settings).await.unwrap();

    let mut calls = 0;
    execute_plan(
        db.pool(),
        1,
        &root,
        plan,
        || {
            calls += 1;
            calls <= 2
        },
        |_| {},
    )
    .await
    .unwrap();
    assert!(indexed(&root, 1).exists());

    let settings = AppConfig {
        copyright_directory: false,
        artist_directory: false,
        ..settings
    };
    run(db.pool(), &root, &settings).await;

    for id in 1..=4 {
        assert!(
            Path::new(&root)
                .join(format!("Alice х Zed [Artist] - {id}.png"))
                .exists()
        );
    }
    assert!(!Path::new(&root).join(".nexa-sync-1.png").exists());
    run(db.pool(), &root, &AppConfig::default()).await;
    assert!(indexed(&root, 1).exists());
    db.close().await;
}

#[tokio::test]
async fn refuses_external_conflicts_and_changed_sources_without_losing_files() {
    let (_temp, db, root, stage) = fixture().await;
    fs::create_dir_all(Path::new(&root).join("Alpha/Artist")).unwrap();
    fs::write(indexed(&root, 1), b"external content").unwrap();

    let plan = targets(db.pool(), &root, &AppConfig::default())
        .await
        .unwrap();
    assert!(
        execute_plan(db.pool(), 1, &root, plan, || true, |_| {})
            .await
            .is_err()
    );
    assert_eq!(fs::read(indexed(&root, 1)).unwrap(), b"external content");
    assert!(Path::new(&stage).join("capture-1.png").exists());
    assert!(!Path::new(&root).join(".nexa-sync-1.png").exists());
    sqlx::query("UPDATE media SET file_revision='stale' WHERE id=1")
        .execute(db.pool())
        .await
        .unwrap();
    assert!(relocate(db.pool(), 1, 1, &root, "moved.png").await.is_err());
    assert!(Path::new(&stage).join("capture-1.png").exists());
    db.close().await;
}

#[tokio::test]
async fn recovers_publication_and_database_commit_boundaries_after_reopen() {
    let (temp, db, root, stage) = fixture().await;

    let source = Path::new(&stage).join("capture-1.png");

    let rev = revision(&source).unwrap();
    sqlx::query("INSERT INTO file_operations VALUES(1,?,'capture-1.png',?,1,?,'published.png')")
        .bind(&stage)
        .bind(&rev)
        .bind(&root)
        .execute(db.pool())
        .await
        .unwrap();
    fs::hard_link(&source, Path::new(&root).join("published.png")).unwrap();
    db.close().await;

    let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
    recover(db.pool()).await.unwrap();
    assert!(!source.exists());

    let current: String = sqlx::query_scalar("SELECT relative_path FROM media WHERE id=1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(current, "published.png");
    // Simulate committed target followed by source cleanup, before journal deletion.
    sqlx::query("INSERT INTO file_operations VALUES(1,?,'capture-1.png',?,1,?,'published.png')")
        .bind(&stage)
        .bind(&rev)
        .bind(&root)
        .execute(db.pool())
        .await
        .unwrap();
    recover(db.pool()).await.unwrap();

    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM file_operations")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(pending, 0);
    assert_eq!(
        fs::read(Path::new(&root).join("published.png")).unwrap(),
        b"independent content 1"
    );
    db.close().await;
}

#[test]
fn confines_components_and_rejects_escaping_paths() {
    assert_eq!(component("CON.png").unwrap(), "_CON.png");
    assert_eq!(component("Artist/../Other").unwrap(), "Artist_.._Other");
    assert!(component(".. ").is_err());

    let root = tempfile::tempdir().unwrap();
    assert!(relative(root.path(), "../outside.png", false).is_err());
    assert!(relative(root.path(), "missing/folder/image.png", false).is_ok());
}

#[tokio::test]
async fn relocates_gifs_to_configured_subfolder_preserving_bytes_and_image_naming() {
    let (_temp, db, root, stage) = fixture().await;

    let source = Path::new(&stage).join("capture-1.gif");
    fs::rename(Path::new(&stage).join("capture-1.png"), &source).unwrap();

    let bytes = fs::read(&source).unwrap();
    sqlx::query(concat!(
        "UPDATE media SET extension='GIF',relative_path='capture-1.gif',",
        "path_key='capture-1.gif',original_filename='capture-1.gif' WHERE id=1",
    ))
    .execute(db.pool())
    .await
    .unwrap();

    let mut settings = AppConfig::default();
    run(db.pool(), &root, &settings).await;

    let default_path = Path::new(&root).join("Alpha/Artist/Animations/Alice х Zed [Artist].gif");
    assert_eq!(fs::read(&default_path).unwrap(), bytes);
    assert!(!source.exists());
    assert!(indexed(&root, 1).exists()); // Other images stay outside Animations.
    settings.gif_subfolder = "GIFs".into();
    run(db.pool(), &root, &settings).await;

    let custom_path = Path::new(&root).join("Alpha/Artist/GIFs/Alice х Zed [Artist].gif");
    assert_eq!(fs::read(&custom_path).unwrap(), bytes);
    assert!(!default_path.exists());
    settings.gif_subfolder = " ".into();
    settings.copyright_directory = false;
    settings.artist_directory = false;
    run(db.pool(), &root, &settings).await;
    assert_eq!(
        fs::read(Path::new(&root).join("Alice х Zed [Artist] - 1.gif")).unwrap(),
        bytes
    );
    assert!(!custom_path.exists());

    let kind: String = sqlx::query_scalar("SELECT kind FROM media WHERE id=1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(kind, "image");
    db.close().await;
}

#[tokio::test]
async fn names_videos_without_characters_and_images_using_original_provenance() {
    let (_temp, db, root, _stage) = fixture().await;
    sqlx::query("DELETE FROM media_characters WHERE media_id IN (1,2)")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE media SET kind='video',extension='mp4' WHERE id=1")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO video_metadata(media_id,title) VALUES(1,'Video title')")
        .execute(db.pool())
        .await
        .unwrap();

    let plan = targets(db.pool(), &root, &AppConfig::default())
        .await
        .unwrap();
    assert_eq!(
        Path::new(&plan.iter().find(|p| p.0 == 1).unwrap().1),
        Path::new("Alpha/Artist/Videos/Video title [Artist].mp4")
    );
    assert_eq!(
        Path::new(&plan.iter().find(|p| p.0 == 2).unwrap().1),
        Path::new("Alpha/Artist/capture-2 [Artist].png")
    );
    // Physical renames cannot change the fallback base.
    sqlx::query(
        "UPDATE media SET relative_path='edited-name.png',path_key='edited-name.png' WHERE id=2",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let again = targets(db.pool(), &root, &AppConfig::default())
        .await
        .unwrap();
    assert_eq!(
        again.iter().find(|p| p.0 == 2).unwrap().1,
        plan.iter().find(|p| p.0 == 2).unwrap().1
    );
    db.close().await;
}

#[tokio::test]
async fn resumes_a_paused_filename_swap_without_overwriting_either_file() {
    let (_temp, db, root, _stage) = fixture().await;
    sqlx::raw_sql(concat!(
        "UPDATE media SET needs_distribution=1 WHERE id IN (3,4); DELETE FROM media_characters ",
        "WHERE media_id IN (1,2); INSERT INTO media_characters VALUES(1,2,2),(2,1,1);",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    run(db.pool(), &root, &AppConfig::default()).await;
    sqlx::raw_sql(concat!(
        "DELETE FROM media_characters WHERE media_id IN (1,2); INSERT INTO media_characters ",
        "VALUES(1,1,1),(2,2,2);",
    ))
    .execute(db.pool())
    .await
    .unwrap();

    let plan = targets(db.pool(), &root, &AppConfig::default())
        .await
        .unwrap();

    let mut calls = 0;
    execute_plan(
        db.pool(),
        1,
        &root,
        plan,
        || {
            calls += 1;
            calls <= 2
        },
        |_| {},
    )
    .await
    .unwrap();
    assert!(Path::new(&root).join(".nexa-sync-1.png").exists());
    run(db.pool(), &root, &AppConfig::default()).await;
    assert_eq!(
        fs::read(Path::new(&root).join("Alpha/Artist/Zed [Artist].png")).unwrap(),
        b"independent content 1"
    );
    assert_eq!(
        fs::read(Path::new(&root).join("Alpha/Artist/Alice [Artist].png")).unwrap(),
        b"independent content 2"
    );
    assert!(!Path::new(&root).join(".nexa-sync-1.png").exists());
    db.close().await;
}
