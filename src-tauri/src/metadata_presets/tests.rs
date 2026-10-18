use super::{Character, Metadata, list, remove, save, validate};
use crate::database::Database;

fn metadata() -> Metadata {
    Metadata {
        artist: " Artist ".into(),
        copyrights: vec!["World".into(), "world".into()],
        characters: vec![Character {
            name: "Alice".into(),
            copyright: "WORLD".into(),
        }],
        tags: vec!["Tag".into(), " tag ".into()],
    }
}

#[test]
fn normalizes_and_deduplicates_preserving_character_copyright() {
    let result = validate(metadata()).unwrap();

    assert_eq!(result.artist, "Artist");
    assert_eq!(result.copyrights, ["World"]);
    assert_eq!(result.tags, ["Tag"]);
    assert_eq!(result.characters[0].copyright, "World");
}

#[test]
fn rejects_missing_artist_invalid_names_and_unselected_copyright() {
    let mut draft = metadata();
    draft.artist = " ".into();
    assert!(validate(draft).is_err());

    let mut draft = metadata();
    draft.tags = vec!["bad\nname".into()];
    assert!(validate(draft).is_err());

    let mut draft = metadata();
    draft.copyrights.clear();
    assert!(validate(draft).is_err());

    let mut draft = metadata();
    draft.tags = vec!["tag".into(); 501];
    assert!(validate(draft).is_err());
}

#[tokio::test]
async fn persists_updates_and_deletes_presets_across_reopen_without_creating_taxonomy() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("presets.db");
    let database = Database::open(&path).await.unwrap();
    let preset = save(database.pool(), None, " Regular artist ", metadata())
        .await
        .unwrap();
    let id = preset.id.parse().unwrap();

    assert_eq!(preset.name, "Regular artist");
    assert!(
        save(database.pool(), None, "REGULAR ARTIST", metadata())
            .await
            .is_err()
    );
    assert!(
        save(database.pool(), Some(i64::MAX), "Missing", metadata())
            .await
            .is_err()
    );
    database.close().await;

    let database = Database::open(&path).await.unwrap();
    let presets = list(database.pool()).await.unwrap();
    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].metadata.characters[0].name, "Alice");

    let mut draft = metadata();
    draft.tags = vec!["New tag".into()];
    save(database.pool(), Some(id), "Renamed", draft)
        .await
        .unwrap();
    let presets = list(database.pool()).await.unwrap();
    assert_eq!(presets[0].name, "Renamed");
    assert_eq!(presets[0].metadata.tags, ["New tag"]);

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM artists")
        .fetch_one(database.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);

    remove(database.pool(), id).await.unwrap();
    assert!(remove(database.pool(), id).await.is_err());
    database.close().await;

    let database = Database::open(&path).await.unwrap();
    assert!(list(database.pool()).await.unwrap().is_empty());
    database.close().await;
}
