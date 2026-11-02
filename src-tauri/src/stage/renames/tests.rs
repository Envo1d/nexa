use std::{fs, path::PathBuf};

use sqlx::Row;

use super::*;
use crate::{database::Database, stage::scanning::scan_watched_source};

fn image(path: &Path, width: u32) {
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
    fs::write(path, bytes).unwrap();
}

async fn fixture() -> (tempfile::TempDir, Database, PathBuf, RenameTracker) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Media 日本");
    fs::create_dir(&root).unwrap();
    image(&root.join("original.bmp"), 2);
    let database = Database::open(&temp.path().join("catalog.db"))
        .await
        .unwrap();

    (temp, database, root, RenameTracker::default())
}

async fn scan(database: &Database, root: &Path, kind: &str, tracker: &mut RenameTracker) {
    let status = scan_watched_source(
        database.pool(),
        root.to_path_buf(),
        kind,
        None,
        |_| {},
        tracker,
    )
    .await
    .unwrap();
    assert_eq!(status.errors, 0);
}

#[tokio::test]
async fn ordinary_scan_baseline_preserves_rename_on_first_notified_scan() {
    for kind in ["library", "stage"] {
        let (_temp, database, root, _) = fixture().await;
        let baselines = tokio::sync::Mutex::new(RenameBaselines::default());
        let initial = crate::stage::scanning::scan_with_baseline(
            database.pool(),
            root.clone(),
            kind,
            None,
            |_| {},
            &baselines,
        )
        .await
        .unwrap();
        assert_eq!(
            (initial.scanned, initial.extracted, initial.errors),
            (1, 1, 0)
        );

        let id: i64 = sqlx::query_scalar("SELECT id FROM media")
            .fetch_one(database.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE media SET favorite=1 WHERE id=?")
            .bind(id)
            .execute(database.pool())
            .await
            .unwrap();
        fs::rename(root.join("original.bmp"), root.join("renamed.bmp")).unwrap();

        let notified = crate::stage::scanning::scan_with_baseline(
            database.pool(),
            root.clone(),
            kind,
            None,
            |_| {},
            &baselines,
        )
        .await
        .unwrap();
        assert_eq!(
            (notified.scanned, notified.extracted, notified.errors),
            (1, 0, 0)
        );
        let rows = sqlx::query("SELECT id,favorite,relative_path FROM media")
            .fetch_all(database.pool())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get::<i64, _>("id"), id);
        assert_eq!(rows[0].get::<i64, _>("favorite"), 1);
        assert_eq!(rows[0].get::<String, _>("relative_path"), "renamed.bmp");
    }
}

#[test]
fn shared_baselines_are_separate_per_source_and_reset_on_root_or_exclusion_change() {
    let mut baselines = RenameBaselines::default();
    let root = Path::new("Library");
    let identity = Identity {
        volume: 1,
        object: 2,
        created: 3,
    };
    baselines.tracker("library", root, None).previous.insert(
        identity,
        Tracked {
            relative: "kept.bmp".into(),
            revision: "revision".into(),
        },
    );
    assert!(baselines.tracker("stage", root, None).previous.is_empty());
    assert_eq!(baselines.tracker("library", root, None).previous.len(), 1);
    assert!(
        baselines
            .tracker("library", root, Some(Path::new("Stage")))
            .previous
            .is_empty()
    );
    baselines.tracker("library", root, None).previous.insert(
        identity,
        Tracked {
            relative: "kept.bmp".into(),
            revision: "revision".into(),
        },
    );
    assert!(
        baselines
            .tracker("library", Path::new("Other"), None)
            .previous
            .is_empty()
    );
}

#[cfg(windows)]
#[tokio::test]
async fn external_renames_preserve_one_identity_annotations_and_updated_paths_in_both_sources() {
    for kind in ["library", "stage"] {
        let (_temp, database, root, mut tracker) = fixture().await;
        scan(&database, &root, kind, &mut tracker).await;
        let id: i64 = sqlx::query_scalar("SELECT id FROM media")
            .fetch_one(database.pool())
            .await
            .unwrap();
        sqlx::raw_sql(concat!(
            "INSERT INTO tags(id,name,name_key) VALUES(1,'Kept tag','kept tag');",
            "INSERT INTO media_tags SELECT id,1 FROM media;",
        ))
        .execute(database.pool())
        .await
        .unwrap();

        let queued = i64::from(kind == "stage");
        sqlx::query("UPDATE media SET favorite=1,needs_distribution=?")
            .bind(queued)
            .execute(database.pool())
            .await
            .unwrap();

        fs::create_dir(root.join("renamed folder")).unwrap();
        fs::rename(
            root.join("original.bmp"),
            root.join("renamed folder/новое.bmp"),
        )
        .unwrap();
        scan(&database, &root, kind, &mut tracker).await;

        let rows = sqlx::query("SELECT id,relative_path,original_filename,favorite,needs_distribution,stage_search_key FROM media")
            .fetch_all(database.pool()).await.unwrap();
        assert_eq!(rows.len(), 1, "A rename must never insert a second row");
        let row = &rows[0];
        assert_eq!(row.get::<i64, _>("id"), id);
        assert!(row.get::<String, _>("relative_path").ends_with("новое.bmp"));
        assert_eq!(row.get::<String, _>("original_filename"), "original.bmp");
        assert_eq!(row.get::<i64, _>("favorite"), 1);
        assert_eq!(row.get::<i64, _>("needs_distribution"), queued);
        assert!(
            row.get::<String, _>("stage_search_key")
                .contains("новое.bmp")
        );
        let tags: i64 = sqlx::query_scalar("SELECT count(*) FROM media_tags WHERE media_id=?")
            .bind(id)
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(tags, 1);

        let root_string = Some(root.to_string_lossy().into_owned());
        let page = if kind == "library" {
            let filters = serde_json::from_value(serde_json::json!({
                "scope": "gallery", "search": "", "kind": "all",
                "sort": "newest", "tagged": false, "offset": 0,
            }))
            .unwrap();
            serde_json::to_value(
                crate::library::browse(database.pool(), root_string, filters)
                    .await
                    .unwrap(),
            )
            .unwrap()
        } else {
            serde_json::to_value(
                crate::stage::browse(database.pool(), root_string, 0, "", "newest", "all")
                    .await
                    .unwrap(),
            )
            .unwrap()
        };
        assert_eq!(page["total"], 1);
        assert_eq!(page["files"].as_array().unwrap().len(), 1);

        // A second rename and an external content edit still describe the same file.
        fs::rename(root.join("renamed folder"), root.join("moved folder")).unwrap();
        image(&root.join("moved folder/новое.bmp"), 5);
        scan(&database, &root, kind, &mut tracker).await;
        let row = sqlx::query("SELECT id,width FROM media")
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(row.get::<i64, _>("id"), id);
        assert_eq!(row.get::<i64, _>("width"), 5);
        database.close().await;
    }
}

#[cfg(windows)]
#[tokio::test]
async fn real_copies_remain_separate_and_a_case_only_rename_keeps_identity() {
    let (_temp, database, root, mut tracker) = fixture().await;
    scan(&database, &root, "library", &mut tracker).await;
    let original: i64 = sqlx::query_scalar("SELECT id FROM media")
        .fetch_one(database.pool())
        .await
        .unwrap();
    fs::copy(root.join("original.bmp"), root.join("copy.bmp")).unwrap();
    scan(&database, &root, "library", &mut tracker).await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM media WHERE availability='present'")
        .fetch_one(database.pool())
        .await
        .unwrap();
    assert_eq!(count, 2);

    fs::rename(root.join("original.bmp"), root.join("ORIGINAL.bmp")).unwrap();
    scan(&database, &root, "library", &mut tracker).await;
    let renamed: i64 =
        sqlx::query_scalar("SELECT id FROM media WHERE relative_path='ORIGINAL.bmp'")
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(renamed, original);
    database.close().await;
}

#[tokio::test]
async fn repeated_external_renames_between_scan_batches_update_the_same_catalog_row() {
    let (_temp, database, root, mut tracker) = fixture().await;
    scan(&database, &root, "library", &mut tracker).await;
    let source: i64 = sqlx::query_scalar("SELECT source_id FROM media")
        .fetch_one(database.pool())
        .await
        .unwrap();
    let id: i64 = sqlx::query_scalar("SELECT id FROM media")
        .fetch_one(database.pool())
        .await
        .unwrap();
    tracker.begin();

    let mut old = "original.bmp";

    for new in ["intermediate.bmp", "final.bmp"] {
        fs::rename(root.join(old), root.join(new)).unwrap();
        let file = inspect(root.join(new), &root).unwrap().unwrap();
        tracker
            .observe(database.pool(), source, &root, &[file])
            .await
            .unwrap();
        let row = sqlx::query("SELECT id,relative_path FROM media")
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(row.get::<i64, _>("id"), id);
        assert_eq!(row.get::<String, _>("relative_path"), new);
        old = new;
    }

    scan(&database, &root, "library", &mut tracker).await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM media")
        .fetch_one(database.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
    database.close().await;
}

#[cfg(windows)]
#[tokio::test]
async fn incomplete_identity_pass_keeps_baseline_and_stale_catalog_revision_blocks_adoption() {
    let (_temp, database, root, mut tracker) = fixture().await;
    scan(&database, &root, "library", &mut tracker).await;
    tracker.begin();
    tracker.finish(false);
    fs::rename(root.join("original.bmp"), root.join("renamed.bmp")).unwrap();
    scan(&database, &root, "library", &mut tracker).await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM media")
        .fetch_one(database.pool())
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "Incomplete passes cannot erase the identity baseline"
    );

    sqlx::query("UPDATE media SET file_revision='catalog changed independently'")
        .execute(database.pool())
        .await
        .unwrap();
    fs::rename(root.join("renamed.bmp"), root.join("last.bmp")).unwrap();
    scan(&database, &root, "library", &mut tracker).await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM media WHERE availability='present'")
        .fetch_one(database.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
    let old: String =
        sqlx::query_scalar("SELECT availability FROM media WHERE relative_path='renamed.bmp'")
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(
        old, "missing",
        "An independently changed catalog row must not be reassigned"
    );
    database.close().await;
}

#[cfg(windows)]
#[tokio::test]
async fn hard_links_and_existing_destination_records_never_steal_another_identity() {
    let (_temp, database, root, mut tracker) = fixture().await;
    fs::hard_link(root.join("original.bmp"), root.join("linked.bmp")).unwrap();
    assert!(identity(&root.join("original.bmp")).is_none());
    scan(&database, &root, "stage", &mut tracker).await;
    fs::rename(root.join("original.bmp"), root.join("renamed.bmp")).unwrap();
    scan(&database, &root, "stage", &mut tracker).await;
    let present: i64 =
        sqlx::query_scalar("SELECT count(*) FROM media WHERE availability='present'")
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(present, 2);
    database.close().await;

    let (_temp, database, root, mut tracker) = fixture().await;
    image(&root.join("target.bmp"), 3);
    scan(&database, &root, "stage", &mut tracker).await;
    let target_id: i64 =
        sqlx::query_scalar("SELECT id FROM media WHERE relative_path='target.bmp'")
            .fetch_one(database.pool())
            .await
            .unwrap();
    fs::remove_file(root.join("target.bmp")).unwrap();
    fs::rename(root.join("original.bmp"), root.join("target.bmp")).unwrap();
    scan(&database, &root, "stage", &mut tracker).await;
    let current: i64 = sqlx::query_scalar("SELECT id FROM media WHERE relative_path='target.bmp'")
        .fetch_one(database.pool())
        .await
        .unwrap();
    assert_eq!(current, target_id);
    database.close().await;
}
