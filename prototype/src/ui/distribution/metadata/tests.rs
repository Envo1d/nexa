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
