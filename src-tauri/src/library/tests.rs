use sqlx::Row;

use super::metadata::{Draft, save, save_mode};
use super::*;
use crate::{database::Database, naming::compare_names};

fn filters() -> Filters {
    Filters {
        scope: "gallery".into(),
        kind: "all".into(),
        sort: "newest".into(),
        ..Default::default()
    }
}

async fn fixture() -> (tempfile::TempDir, Database, String) {
    let temp = tempfile::tempdir().unwrap();

    let root = temp
        .path()
        .join("Library 日本")
        .to_string_lossy()
        .into_owned();

    let db = Database::open(&temp.path().join("vault.db")).await.unwrap();
    sqlx::query(
        "INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(1,'library',?,?,1)",
    )
    .bind(&root)
    .bind(&root)
    .execute(db.pool())
    .await
    .unwrap();

    let mut tx = db.pool().begin().await.unwrap();

    for id in 1..=503i64 {
        let path = format!("nested/Image {id}.png");
        sqlx::query(concat!(
            "INSERT INTO media(id,source_id,relative_path,original_filename,path_key,kind,",
            "extension,last_seen_at,created_at,updated_at,file_modified_at,favorite) VALUES(?,1,?,",
            "?,?,'image','png',1,1,1,?,?)",
        ))
        .bind(id)
        .bind(&path)
        .bind(format!("Image {id}.png"))
        .bind(&path)
        .bind(id)
        .bind(id == 2)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::query("UPDATE media SET availability='missing' WHERE id=502")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE media SET needs_distribution=1 WHERE id=503")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (temp, db, root)
}

fn draft() -> Draft {
    Draft {
        copyrights: vec!["Series".into(), "Other".into()],
        artists: vec!["Straße".into()],
        characters: vec![
            Character {
                name: "Alice".into(),
                copyright: "Series".into(),
            },
            Character {
                name: "Alice".into(),
                copyright: "Other".into(),
            },
        ],
        tags: vec!["Cafe\u{301}".into(), "CAFÉ".into()],
        video_title: String::new(),
    }
}

#[tokio::test]
async fn search_matches_substrings_across_metadata_and_viewer_with_literal_punctuation() {
    let (_temp, db, root) = fixture().await;
    save(db.pool(), Some(root.clone()), vec!["1".into()], draft())
        .await
        .unwrap();

    for (search, expected) in [
        (" STRASS ; CAFÉ; Al; Ser ", 1),
        ("traße", 1),
        ("lice", 1),
        ("fé", 1),
        ("ther", 1),
        ("%", 0),
        ("_", 0),
    ] {
        let mut query = filters();
        query.search = search.into();

        let page = browse(db.pool(), Some(root.clone()), query).await.unwrap();
        assert_eq!(page.total, expected, "{search}");

        let mut query = filters();
        query.search = search.into();

        let sequence =
            super::queries::capture_viewer_sequence(db.pool(), Some(root.clone()), query)
                .await
                .unwrap();
        assert_eq!(sequence.len() as i64, expected, "viewer: {search}");
    }
    db.close().await;
}

#[tokio::test]
async fn prefix_groups_keep_sorting_pagination_and_viewer_order() {
    let (_temp, db, root) = fixture().await;
    sqlx::raw_sql(concat!(
        "INSERT INTO tags VALUES(1,'atomic','atomic'),(2,'marvel','marvel'); ",
        "INSERT INTO media_tags SELECT id,1 FROM media WHERE id<=501; ",
        "DELETE FROM media_tags WHERE media_id=1; INSERT INTO media_tags VALUES(1,2);",
    ))
    .execute(db.pool())
    .await
    .unwrap();

    for sort in ["newest", "older", "name-asc", "name-desc"] {
        let mut baseline = filters();
        baseline.sort = sort.into();
        let values =
            super::queries::capture_viewer_sequence(db.pool(), Some(root.clone()), baseline)
                .await
                .unwrap();
        let values = serde_json::to_value(values).unwrap();
        let mut expected = vec!["1".to_string()];
        expected.extend(values.as_array().unwrap().iter().filter_map(|value| {
            let id = value["id"].as_str().unwrap();
            (id != "1").then(|| id.to_string())
        }));

        let mut actual = Vec::new();
        for offset in [0, 500] {
            let mut query = filters();
            query.sort = sort.into();
            query.search = "m".into();
            query.offset = offset;
            let page = browse(db.pool(), Some(root.clone()), query).await.unwrap();
            assert_eq!(page.total, 501);
            actual.extend(page.files.into_iter().map(|file| file.technical.id));
        }
        assert_eq!(actual, expected, "{sort}");

        let mut query = filters();
        query.sort = sort.into();
        query.search = "m".into();
        let sequence =
            super::queries::capture_viewer_sequence(db.pool(), Some(root.clone()), query)
                .await
                .unwrap();
        let sequence = serde_json::to_value(sequence).unwrap();
        let ids: Vec<_> = sequence
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, expected, "viewer: {sort}");
    }
    db.close().await;
}

#[tokio::test]
async fn viewer_order_matches_filters_beyond_one_page_and_survives_annotation_changes() {
    use super::queries::capture_viewer_sequence;

    let (_temp, db, root) = fixture().await;

    let sequence = capture_viewer_sequence(db.pool(), Some(root.clone()), filters())
        .await
        .unwrap();

    let captured = serde_json::to_value(&sequence).unwrap();
    assert_eq!(sequence.len(), 501);
    assert_eq!(captured[0]["id"], "501");
    assert_eq!(captured[500]["id"], "1");

    for sort in ["older", "name-asc", "name-desc"] {
        let mut query = filters();
        query.sort = sort.into();

        let sequence = capture_viewer_sequence(db.pool(), Some(root.clone()), query)
            .await
            .unwrap();
        let captured = serde_json::to_value(&sequence).unwrap();

        let mut query = filters();
        query.sort = sort.into();

        let page = browse(db.pool(), Some(root.clone()), query).await.unwrap();
        assert_eq!(captured[0]["id"], page.files[0].technical.id);
        assert_eq!(captured[499]["id"], page.files[499].technical.id);
    }

    let mut query = filters();
    query.scope = "favorites".into();

    let favorites = capture_viewer_sequence(db.pool(), Some(root.clone()), query)
        .await
        .unwrap();
    assert_eq!(favorites.len(), 1);
    assert_eq!(serde_json::to_value(&favorites).unwrap()[0]["id"], "2");
    sqlx::query("UPDATE media SET favorite=0 WHERE id=2")
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(serde_json::to_value(&favorites).unwrap()[0]["id"], "2");

    save(db.pool(), Some(root.clone()), vec!["1".into()], draft())
        .await
        .unwrap();

    let mut query = filters();
    query.search = "café; STRASSE".into();
    query.tagged = true;

    let matched = capture_viewer_sequence(db.pool(), Some(root), query)
        .await
        .unwrap();
    assert_eq!(matched.len(), 1);
    assert_eq!(serde_json::to_value(&matched).unwrap()[0]["id"], "1");
    db.close().await;
}

#[tokio::test]
async fn nested_stage_is_hidden_from_main_files_counts_favorites_and_tags_before_rescan() {
    let (_temp, db, root) = fixture().await;
    sqlx::query(concat!(
        "UPDATE media SET relative_path='Stage %_/deep/Incoming.png',path_key='Stage ",
        "%_/deep/Incoming.png',favorite=1 WHERE id=1",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(concat!(
        "UPDATE media SET relative_path='Stage %_ sibling/Kept.png',path_key='Stage %_ ",
        "sibling/Kept.png' WHERE id=2",
    ))
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("INSERT INTO tags(id,name,name_key) VALUES(1,'Incoming','incoming')")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO media_tags VALUES(1,1)")
        .execute(db.pool())
        .await
        .unwrap();

    let mut query = filters();
    query.stage_prefix = Some(format!("{}/Stage %_/", root.replace('\\', "/")));

    let page = browse(db.pool(), Some(root.clone()), query).await.unwrap();
    assert_eq!(page.total, 500);
    assert_eq!(page.type_counts, [500, 0, 500]);
    assert!(page.tags.is_empty());
    assert!(!page.files.iter().any(|file| file.technical.id == "1"));
    assert!(page.files.iter().any(|file| file.technical.id == "2"));

    let mut query = filters();
    query.scope = "favorites".into();
    query.stage_prefix = Some(format!("{}/Stage %_/", root.replace('\\', "/")));
    assert_eq!(
        browse(db.pool(), Some(root.clone()), query)
            .await
            .unwrap()
            .total,
        1
    );
    // Distribution with sync disabled gives an explicit logical Main assignment.
    sqlx::query("UPDATE media SET library_assignment=? WHERE id=1")
        .bind(&root)
        .execute(db.pool())
        .await
        .unwrap();

    let mut query = filters();
    query.stage_prefix = Some(format!("{}/Stage %_/", root.replace('\\', "/")));

    let page = browse(db.pool(), Some(root), query).await.unwrap();
    assert_eq!(page.total, 501);
    assert_eq!(page.tags[0].count, 1);
    db.close().await;
}

#[tokio::test]
async fn pages_filter_the_entire_library_and_exclude_missing_queued_and_other_roots() {
    let (_temp, db, root) = fixture().await;

    let page = browse(db.pool(), Some(root.clone()), filters())
        .await
        .unwrap();
    assert_eq!(page.total, 501);
    assert_eq!(page.files.len(), 500);
    assert_eq!(page.type_counts, [501, 0, 501]);

    let mut f = filters();
    f.offset = 500;

    let next = browse(db.pool(), Some(root.clone()), f).await.unwrap();
    assert_eq!(next.files.len(), 1);
    assert_eq!(next.files[0].technical.id, "1");

    let mut f = filters();
    f.sort = "name-asc".into();

    let names = browse(db.pool(), Some(root.clone()), f).await.unwrap();
    assert_eq!(names.files[0].technical.id, "1");
    assert_eq!(names.files[1].technical.id, "2");

    let mut f = filters();
    f.scope = "favorites".into();

    let favorites = browse(db.pool(), Some(root.clone()), f).await.unwrap();
    assert_eq!(favorites.total, 1);
    assert!(favorites.files[0].favorite);
    assert_eq!(favorites.type_counts, [1, 0, 1]);
    assert_eq!(
        browse(db.pool(), Some("other root".into()), filters())
            .await
            .unwrap()
            .total,
        0
    );
    save(db.pool(), Some(root.clone()), vec!["1".into()], draft())
        .await
        .unwrap();

    let mut f = filters();
    f.search = "STRASSE; CAFÉ; Alice".into();
    f.tagged = true;

    let found = browse(db.pool(), Some(root.clone()), f).await.unwrap();
    assert_eq!(found.total, 1);
    assert_eq!(found.files[0].technical.id, "1");
    assert_eq!(found.files[0].character_copyrights.len(), 2);
    assert_eq!(found.files[0].tags, vec!["Café"]);
    assert_eq!(found.tags.len(), 1);
    assert_eq!(found.tags[0].count, 1);
    assert!(found.suggestions.iter().any(|entry| entry.name == "Straße"));

    let mut f = filters();
    f.search = "' OR 1=1 --".into();
    assert_eq!(browse(db.pool(), Some(root), f).await.unwrap().total, 0);
    db.close().await;
}

#[tokio::test]
async fn metadata_saves_are_atomic_qualified_and_preserve_favorites_and_video_details() {
    let (temp, db, root) = fixture().await;
    save(db.pool(), Some(root.clone()), vec!["2".into()], draft())
        .await
        .unwrap();

    let mut bad = draft();
    bad.artists = vec!["Transient".into()];
    bad.characters[0].copyright = "Not selected".into();
    assert!(
        save(db.pool(), Some(root.clone()), vec!["2".into()], bad)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM artists WHERE name='Transient'")
            .fetch_one(db.pool())
            .await
            .unwrap(),
        0
    );
    assert!(
        save(
            db.pool(),
            Some(root.clone()),
            vec!["2".into(), "503".into()],
            draft()
        )
        .await
        .is_err()
    );
    sqlx::query("UPDATE media SET kind='video',extension='mp4' WHERE id=2")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO video_metadata(media_id,title,duration_ms,codec) VALUES(2,'Original title',1234,'H.264')")
    .execute(db.pool())
    .await
    .unwrap();
    assert!(
        save(db.pool(), Some(root.clone()), vec!["2".into()], draft())
            .await
            .is_err()
    );

    let mut changed = draft();
    changed.characters.clear();
    changed.video_title = "New title".into();
    save(db.pool(), Some(root.clone()), vec!["2".into()], changed)
        .await
        .unwrap();

    let row = sqlx::query(concat!(
        "SELECT m.favorite,v.title,v.duration_ms,v.codec,m.original_filename,m.relative_path ",
        "FROM media m JOIN video_metadata v ON v.media_id=m.id WHERE m.id=2",
    ))
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<i64, _>("favorite"), 1);
    assert_eq!(row.get::<String, _>("title"), "New title");
    assert_eq!(row.get::<i64, _>("duration_ms"), 1234);
    assert_eq!(row.get::<String, _>("codec"), "H.264");
    assert_eq!(row.get::<String, _>("original_filename"), "Image 2.png");
    assert_eq!(row.get::<String, _>("relative_path"), "nested/Image 2.png");
    db.close().await;

    let reopened = Database::open(&temp.path().join("vault.db")).await.unwrap();

    let mut f = filters();
    f.scope = "favorites".into();

    let page = browse(reopened.pool(), Some(root), f).await.unwrap();
    assert_eq!(page.files[0].video_title.as_deref(), Some("New title"));
    assert!(page.files[0].character_copyrights.is_empty());
    reopened.close().await;
}

#[test]
fn filename_sort_is_natural_unicode_insensitive_and_ignores_folders() {
    assert!(compare_names("Z/Image 2.png", "A/Image 10.png").is_lt());
    assert!(
        compare_names(
            "Image 99999999999999999999.png",
            "Image 100000000000000000000.png"
        )
        .is_lt()
    );
    assert!(compare_names("Straße 02.png", "STRASSE 2.png").is_eq());
}

#[tokio::test]
async fn distribution_saves_logical_membership_without_moving_and_supports_main_edits() {
    let (temp, db, root) = fixture().await;

    let staging = temp.path().join("Stage");
    std::fs::create_dir(&staging).unwrap();

    let original = staging.join("original.png");
    std::fs::write(&original, b"untouched staged content").unwrap();

    let staging = staging.to_string_lossy().into_owned();
    sqlx::query(
        "INSERT INTO sources(id,kind,root_path,path_key,created_at) VALUES(99,'stage',?,?,1)",
    )
    .bind(&staging)
    .bind(&staging)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(concat!(
        "INSERT INTO media(id,source_id,relative_path,path_key,original_filename,kind,",
        "extension,last_seen_at,created_at,updated_at) VALUES(1000,99,'original.png',",
        "'original.png','original.png','image','png',1,1,1)",
    ))
    .execute(db.pool())
    .await
    .unwrap();

    let mut changed = draft();
    changed.copyrights = vec!["Zebra".into(), "Alpha".into()];
    changed.characters = vec![
        Character {
            name: "Zed".into(),
            copyright: "Zebra".into(),
        },
        Character {
            name: "Alice".into(),
            copyright: "Alpha".into(),
        },
    ];
    changed.tags = vec!["Zed".into(), "Alpha".into()];
    save_mode(
        db.pool(),
        Some(root.clone()),
        Some(Some(staging.clone())),
        vec!["1000".into()],
        changed,
    )
    .await
    .unwrap();

    let mut query = filters();
    query.search = "Straße".into();

    let page = browse(db.pool(), Some(root.clone()), query).await.unwrap();

    let file = page
        .files
        .iter()
        .find(|f| f.technical.id == "1000")
        .unwrap();
    assert_eq!(file.copyrights, ["Alpha", "Zebra"]);
    assert_eq!(file.tags, ["Alpha", "Zed"]);
    assert_eq!(file.character_copyrights[0].name, "Alice");
    assert_eq!(file.character_copyrights[1].name, "Zed");
    assert_eq!(
        serde_json::to_value(&file.technical).unwrap()["filename"],
        "original.png"
    );
    sqlx::query("UPDATE media SET stage_search_key='' WHERE id IN (503,1000)")
        .execute(db.pool())
        .await
        .unwrap();

    let staged = stage::browse(db.pool(), Some(staging), 0, "", "newest")
        .await
        .unwrap();
    assert_eq!(staged.total, 1); // Only the existing redistribution record remains.

    let mut edit = draft();
    edit.characters.clear();
    save(db.pool(), Some(root), vec!["1000".into()], edit)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(original).unwrap(),
        b"untouched staged content"
    );

    let row =
        sqlx::query("SELECT source_id,relative_path,original_filename FROM media WHERE id=1000")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(row.get::<i64, _>("source_id"), 99);
    assert_eq!(row.get::<String, _>("relative_path"), "original.png");
    assert_eq!(row.get::<String, _>("original_filename"), "original.png");
    db.close().await;
}

#[tokio::test]
async fn distribution_persists_batch_order_and_rolls_back_failed_batches() {
    let (temp, db, root) = fixture().await;

    sqlx::query("UPDATE media SET needs_distribution=1 WHERE id IN (1,2,3,4)")
        .execute(db.pool())
        .await
        .unwrap();

    for ids in [vec!["3", "1"], vec!["4", "missing"]] {
        let valid = ids[1] != "missing";

        let result = save_mode(
            db.pool(),
            Some(root.clone()),
            Some(None),
            ids.into_iter().map(String::from).collect(),
            draft(),
        )
        .await;
        assert_eq!(result.is_ok(), valid);
    }

    let failed: (i64, Option<i64>, Option<String>) = sqlx::query_as(
        "SELECT needs_distribution,naming_order,library_assignment FROM media WHERE id=4",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(failed, (1, None, None));

    save_mode(
        db.pool(),
        Some(root.clone()),
        Some(None),
        vec!["4".into(), "2".into()],
        draft(),
    )
    .await
    .unwrap();

    // Main metadata editing must retain the original incoming rank.
    save(db.pool(), Some(root.clone()), vec!["1".into()], draft())
        .await
        .unwrap();
    db.close().await;

    let reopened = Database::open(&temp.path().join("vault.db")).await.unwrap();

    let key = crate::disk_sync::distribution_order_key(&root).unwrap();

    let ordered: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT id,naming_order FROM media WHERE naming_order_key=? ORDER BY naming_order",
    )
    .bind(key)
    .fetch_all(reopened.pool())
    .await
    .unwrap();
    assert_eq!(ordered, [(3, 0), (1, 1), (4, 2), (2, 3)]);
    reopened.close().await;
}
