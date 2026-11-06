use super::add_tag;

#[test]
fn draft_changes_leave_original_tags_untouched_until_save() {
    let original = vec!["landscape".to_string()];
    let mut draft = original.clone();
    add_tag(&mut draft, "favorite").unwrap();
    draft.remove(0);
    assert_eq!(original, ["landscape"]);
    assert_eq!(draft, ["favorite"]);
}

#[test]
fn tags_normalize_unicode_and_ignore_case_duplicates() {
    let mut draft = vec![];
    add_tag(&mut draft, "  Cafe\u{301}  ").unwrap();
    add_tag(&mut draft, "CAFÉ").unwrap();
    assert_eq!(draft, ["Café"]);
}

#[test]
fn invalid_tags_do_not_mutate_the_draft() {
    let mut draft = vec![];
    assert!(add_tag(&mut draft, &"a".repeat(25)).is_err());
    assert!(add_tag(&mut draft, "a\nb").is_err());
    assert!(draft.is_empty());
    draft = (0..100).map(|i| format!("tag{i}")).collect();
    assert!(add_tag(&mut draft, "extra").is_err());
    assert!(add_tag(&mut draft, "TAG0").is_ok());
    assert_eq!(draft.len(), 100);
}
