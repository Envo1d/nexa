use super::{Metadata, Preset, validate_name};

#[test]
fn removing_copyright_keeps_only_its_qualified_characters() {
    let mut metadata = Metadata {
        artist: "Artist".into(),
        values: [
            vec!["World (2026)".into()],
            vec!["tag".into()],
            vec!["Hero (World (2026))".into(), "Other (Elsewhere)".into()],
        ],
    };

    metadata.prune_characters();

    assert_eq!(metadata.values[2], ["Hero (World (2026))"]);
    assert_eq!(metadata.values[1], ["tag"]);

    metadata.values[0].clear();
    metadata.prune_characters();

    assert!(metadata.values[2].is_empty());
}

#[test]
fn preset_name_uniqueness_excludes_only_the_updated_preset() {
    let entries = [Preset {
        id: 7,
        name: "Набор".into(),
        metadata: Metadata::default(),
    }];

    assert!(validate_name(&entries, None, "  НАБОР  ", "Artist").is_err());
    assert!(validate_name(&entries, Some(8), "Набор", "Artist").is_err());
    assert!(validate_name(&entries, Some(7), "Набор", "Artist").is_ok());
    assert!(validate_name(&entries, None, "Another", "Artist").is_ok());
}

#[test]
fn rejects_missing_required_fields_and_invalid_preset_names() {
    assert!(validate_name(&[], None, "  ", "Artist").is_err());
    assert!(validate_name(&[], None, "Preset", "  ").is_err());
    assert!(validate_name(&[], None, "Bad\nname", "Artist").is_err());
    assert!(validate_name(&[], None, &"Я".repeat(257), "Artist").is_err());
    assert!(validate_name(&[], None, &"Я".repeat(256), "Artist").is_ok());
}

#[test]
fn custom_values_keep_first_spelling_and_ignore_case_duplicates() {
    let mut metadata = Metadata::default();
    metadata.insert_value(0, "Wonderland".into());
    metadata.insert_value(0, "WONDERLAND".into());
    metadata.insert_value(1, "Тест".into());
    metadata.insert_value(1, "тест".into());
    metadata.insert_value(2, "Alice (Wonderland)".into());
    metadata.insert_value(2, "Alice (Lunar Tales)".into());

    assert_eq!(metadata.values[0], ["Wonderland"]);
    assert_eq!(metadata.values[1], ["Тест"]);
    assert_eq!(metadata.values[2].len(), 2);
}

#[test]
fn preset_search_prioritizes_prefix_and_preserves_equal_rank_order() {
    let entries = [
        Preset {
            id: 1,
            name: "A night".into(),
            metadata: Metadata::default(),
        },
        Preset {
            id: 2,
            name: "Night sky".into(),
            metadata: Metadata::default(),
        },
        Preset {
            id: 3,
            name: "The night".into(),
            metadata: Metadata::default(),
        },
    ];
    let result = super::matching_presets(&entries, " NIGHT ");

    assert_eq!(result.iter().map(|p| p.id).collect::<Vec<_>>(), [2, 1, 3]);
    assert!(super::matching_presets(&entries, "missing").is_empty());
}

#[test]
fn preset_name_input_matches_utf16_maxlength_without_splitting_characters() {
    assert_eq!(super::limited_name(&"Я".repeat(257)), "Я".repeat(256));
    assert_eq!(super::limited_name(&"😀".repeat(129)), "😀".repeat(128));
    assert_eq!(super::limited_name("Unchanged"), "Unchanged");
}

#[test]
fn picker_summary_orders_copyrights_characters_then_tags_and_omits_qualifiers() {
    let metadata = Metadata {
        artist: "Artist".into(),
        values: [
            vec!["World".into()],
            vec!["Tag".into()],
            vec!["Alice (World)".into()],
        ],
    };

    assert_eq!(metadata.summary(), "World · Alice · Tag");
    assert_eq!(Metadata::default().summary(), "—");
}
