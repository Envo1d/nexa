use super::{insert_value, prune_characters};

#[test]
fn distribution_custom_values_deduplicate_unicode_case_and_preserve_first_spelling() {
    let mut values = vec![];
    for value in [
        "Произведение",
        "ПРОИЗВЕДЕНИЕ",
        "Alice (World)",
        "ALICE (WORLD)",
        "Alice (Other)",
    ] {
        insert_value(&mut values, value.into());
    }

    assert_eq!(values, ["Произведение", "Alice (World)", "Alice (Other)"]);
}

#[test]
fn removing_one_copyright_prunes_only_its_characters_with_case_insensitive_identity() {
    let mut values = [
        vec!["World (2026)".into(), "Other".into()],
        vec!["Tag".into()],
        vec!["Hero (WORLD (2026))".into(), "Other hero (Other)".into()],
    ];
    values[0].remove(1);
    prune_characters(&mut values);

    assert_eq!(values[2], ["Hero (WORLD (2026))"]);
    assert_eq!(values[1], ["Tag"]);

    values[0].clear();
    prune_characters(&mut values);

    assert!(values[2].is_empty());
}

#[test]
fn metadata_prefill_and_save_preserve_qualified_characters_and_file_identity() {
    let mut entry = crate::catalog::demo_entries().remove(1);
    entry.details.copyrights = vec!["World (2026)".into(), "Other".into()];
    entry.details.character_copyrights = vec![
        ("Hero".into(), "World (2026)".into()),
        ("Hero".into(), "Other".into()),
    ];
    let path = entry.path.clone();
    let filename = entry.details.filename.clone();
    let favorite = entry.favorite;
    let values = super::entry_values(&entry);
    super::apply_entry(&mut entry, &values, "New artist", "New title");
    assert_eq!(super::entry_values(&entry), values);
    assert_eq!(entry.path, path);
    assert_eq!(entry.details.filename, filename);
    assert_eq!(entry.favorite, favorite);
    assert_eq!(entry.details.artist.as_deref(), Some("New artist"));
    assert_eq!(entry.title, "New title");
    assert_eq!(entry.details.character_copyrights[0].1, "World (2026)");
}

#[test]
fn image_metadata_edits_preserve_title_and_support_removing_optional_fields() {
    let mut entry = crate::catalog::demo_entries().remove(0);
    let title = entry.title.clone();
    super::apply_entry(
        &mut entry,
        &Default::default(),
        "Artist",
        "Ignored video title",
    );
    assert_eq!(entry.title, title);
    assert!(entry.tags.is_empty());
    assert!(entry.characters.is_empty());
    assert!(entry.details.copyrights.is_empty());
    assert!(entry.details.character_copyrights.is_empty());
}

#[test]
fn character_parsing_matches_selected_copyright_instead_of_last_parenthesis() {
    let copyrights = vec!["World (2026)".into(), "Other".into()];
    assert_eq!(
        super::character_parts("Hero (Variant) (WORLD (2026))", &copyrights),
        Some(("Hero (Variant)", "World (2026)"))
    );
    assert!(super::character_parts("Hero (Unknown)", &copyrights).is_none());
}
