use super::*;

fn fixture() -> (Catalog, Vec<Entry>, Vec<Entry>) {
    let main: Vec<_> = crate::catalog::demo_entries()
        .into_iter()
        .take(12)
        .collect();
    let incoming = crate::catalog::incoming_demo_entries();
    let mut catalog = Catalog::default();
    catalog.refresh(main.iter().chain(incoming.iter()));

    (catalog, main, incoming)
}

fn find(catalog: &Catalog, category: Category, name: &str) -> usize {
    catalog
        .records
        .iter()
        .find(|r| r.category == category && r.name == name)
        .unwrap()
        .id
}

#[test]
fn rename_updates_both_workspaces_and_rejects_collisions_without_changes() {
    let (mut catalog, mut main, mut incoming) = fixture();
    incoming[0].tags = main[0].tags.clone();
    catalog.refresh(main.iter().chain(incoming.iter()));
    let old = main[0].tags[0].clone();
    let id = find(&catalog, Category::Tag, &old);
    catalog
        .apply(id, Some(("  Новое имя  ", "")), &mut main, &mut incoming)
        .unwrap();
    assert!(main[0].tags.contains(&"Новое имя".into()));
    assert!(incoming[0].tags.contains(&"Новое имя".into()));
    let other = catalog
        .records
        .iter()
        .find(|r| r.category == Category::Tag && r.id != id)
        .unwrap()
        .name
        .clone();
    assert!(
        catalog
            .apply(
                id,
                Some((&other.to_uppercase(), "")),
                &mut main,
                &mut incoming
            )
            .is_err()
    );
    assert!(
        catalog
            .apply(id, Some(("bad\nname", "")), &mut main, &mut incoming)
            .is_err()
    );
    assert!(main[0].tags.contains(&"Новое имя".into()));
}

#[test]
fn character_reassignment_keeps_other_qualified_identity_and_remaining_copyright() {
    let (mut catalog, mut main, mut incoming) = fixture();
    main[0].details.character_copyrights = vec![
        ("Alice".into(), "Wonderland".into()),
        ("Bob".into(), "Wonderland".into()),
    ];
    main[1].details.character_copyrights = vec![("Alice".into(), "Lunar Tales".into())];
    catalog.refresh(main.iter().chain(incoming.iter()));
    let id = catalog
        .records
        .iter()
        .find(|r| {
            r.category == Category::Character && r.name == "Alice" && r.copyright == "Wonderland"
        })
        .unwrap()
        .id;
    catalog
        .apply(
            id,
            Some(("Alicia", "Lunar Tales")),
            &mut main,
            &mut incoming,
        )
        .unwrap();
    assert!(
        main[0]
            .details
            .character_copyrights
            .contains(&("Alicia".into(), "Lunar Tales".into()))
    );
    assert!(main[0].details.copyrights.contains(&"Wonderland".into()));
    assert!(
        main[1]
            .details
            .character_copyrights
            .contains(&("Alice".into(), "Lunar Tales".into()))
    );
    assert!(
        catalog
            .apply(id, Some(("Alice", "Lunar Tales")), &mut main, &mut incoming)
            .is_err()
    );
}

#[test]
fn deleting_tag_and_character_does_not_queue_files() {
    let (mut catalog, mut main, mut incoming) = fixture();
    let tag = main[0].tags[0].clone();
    let id = find(&catalog, Category::Tag, &tag);
    assert_eq!(
        catalog.apply(id, None, &mut main, &mut incoming).unwrap(),
        0
    );
    assert!(main.iter().all(|r| !r.tags.contains(&tag)));
    let character = catalog
        .records
        .iter()
        .find(|r| r.category == Category::Character)
        .unwrap()
        .clone();
    assert_eq!(
        catalog
            .apply(character.id, None, &mut main, &mut incoming)
            .unwrap(),
        0
    );
    assert_eq!(main.len(), 12);
    assert_eq!(incoming.len(), 48);
}

#[test]
fn deleting_copyright_queues_only_affected_main_and_removes_its_characters() {
    let (mut catalog, mut main, mut incoming) = fixture();
    let name = main[0].details.copyrights[0].clone();
    let expected = main
        .iter()
        .filter(|r| r.details.copyrights.contains(&name))
        .count();
    let paths: Vec<_> = main
        .iter()
        .chain(incoming.iter())
        .map(|r| r.path.clone())
        .collect();
    let id = find(&catalog, Category::Copyright, &name);
    assert_eq!(
        catalog.apply(id, None, &mut main, &mut incoming).unwrap(),
        expected
    );
    assert_eq!(main.len(), 12 - expected);
    assert_eq!(incoming.len(), 48 + expected);
    assert!(main.iter().chain(incoming.iter()).all(|r| {
        !r.details.copyrights.contains(&name)
            && r.details
                .character_copyrights
                .iter()
                .all(|(_, c)| c != &name)
    }));
    assert!(catalog.records.iter().all(|r| r.copyright != name));
    let mut after: Vec<_> = main
        .iter()
        .chain(incoming.iter())
        .map(|r| r.path.clone())
        .collect();
    let mut paths = paths;
    paths.sort();
    after.sort();
    assert_eq!(paths, after);
}

#[test]
fn filters_rank_prefix_and_preserve_unused_entries() {
    let (mut catalog, mut main, incoming) = fixture();
    catalog.records.push(Record {
        id: 9999,
        category: Category::Tag,
        name: "Unused".into(),
        copyright: String::new(),
        count: 0,
    });
    main.clear();
    catalog.refresh(main.iter().chain(incoming.iter()));
    assert!(
        catalog
            .filtered(Category::Tag, "UNU", Some(0), false)
            .contains(&9999)
    );
    assert!(
        catalog
            .filtered(Category::Tag, "UNU", Some(1), false)
            .is_empty()
    );
    catalog.records.push(Record {
        id: 10000,
        category: Category::Character,
        name: "Alice".into(),
        copyright: "Other".into(),
        count: 0,
    });
    assert!(catalog.repeated().contains(&naming::name_key("Alice")));
    let ids = catalog.filtered(Category::Character, "ali", None, true);
    assert!(ids.contains(&10000));
    assert!(
        ids.iter()
            .all(|id| catalog.records.iter().find(|r| r.id == *id).unwrap().name == "Alice")
    );
}

#[test]
fn copyright_rename_updates_qualified_records_and_artist_deletion_preserves_paths() {
    let (mut catalog, mut main, mut incoming) = fixture();
    let id = find(&catalog, Category::Copyright, "Wonderland");
    catalog
        .apply(id, Some(("New World", "")), &mut main, &mut incoming)
        .unwrap();
    assert!(catalog.records.iter().all(|r| r.copyright != "Wonderland"));
    assert!(
        main[0]
            .details
            .character_copyrights
            .iter()
            .all(|(_, copyright)| copyright == "New World")
    );
    let artist = main[0].details.artist.clone().unwrap();
    let path = main[0].path.clone();
    let id = find(&catalog, Category::Artist, &artist);
    let expected = main
        .iter()
        .filter(|r| r.details.artist.as_deref() == Some(&artist))
        .count();
    assert_eq!(
        catalog.apply(id, None, &mut main, &mut incoming).unwrap(),
        expected
    );
    assert!(
        incoming
            .iter()
            .any(|r| r.path == path && r.details.artist.is_none())
    );
}

#[test]
fn moving_last_character_removes_previous_copyright_and_updates_session_image_title() {
    let (mut catalog, mut main, mut incoming) = fixture();
    let id = catalog
        .records
        .iter()
        .find(|r| {
            r.category == Category::Character && r.name == "Alice" && r.copyright == "Wonderland"
        })
        .unwrap()
        .id;
    catalog
        .apply(
            id,
            Some(("Alicia", "Lunar Tales")),
            &mut main,
            &mut incoming,
        )
        .unwrap();
    assert_eq!(main[0].characters, "Alicia");
    assert_eq!(main[0].title, "Alicia");
    assert_eq!(main[0].details.copyrights, vec!["Lunar Tales"]);
    assert_eq!(main[1].characters, "Alicia, Bob");
    assert!(main[1].details.copyrights.contains(&"Wonderland".into()));
}
