use std::collections::BTreeSet;

use super::{replace_content, selection};

#[test]
fn batch_actions_use_selected_stage_files_only_when_target_is_selected() {
    let selected = BTreeSet::from([1, 3, 7]);
    assert_eq!(selection(1, true, &selected, 5), vec![1, 3]);
    assert_eq!(selection(2, true, &selected, 5), vec![2]);
    assert_eq!(selection(1, false, &selected, 5), vec![1]);
    assert!(selection(5, true, &selected, 5).is_empty());
}

#[test]
fn replacement_preserves_main_annotations_and_identity() {
    let entries = crate::catalog::demo_entries();
    let mut main = entries[0].clone();
    let before = main.clone();
    let source = &entries[4];
    replace_content(&mut main, source);
    assert_eq!(main.path, source.path);
    assert_eq!(main.video, source.video);
    assert_eq!(main.details.resolution, source.details.resolution);
    assert_eq!(main.title, before.title);
    assert_eq!(main.details.filename, before.details.filename);
    assert_eq!(main.details.artist, before.details.artist);
    assert_eq!(main.details.copyrights, before.details.copyrights);
    assert_eq!(
        main.details.character_copyrights,
        before.details.character_copyrights
    );
    assert_eq!(main.tags, before.tags);
    assert_eq!(main.favorite, before.favorite);
}

#[test]
fn reorder_draft_preserves_membership_and_can_be_discarded() {
    let original = vec![2, 4, 7];
    let mut draft = original.clone();
    super::move_order(&mut draft, 0, 2);
    assert_eq!(draft, vec![4, 7, 2]);
    super::move_order(&mut draft, 99, 0);
    assert_eq!(draft, vec![4, 7, 2]);
    assert_eq!(original, vec![2, 4, 7]);
}

#[test]
fn group_order_survives_reopening_without_renaming_or_reordering_records() {
    let mut entries = crate::catalog::demo_entries();
    entries.truncate(2);
    entries[1] = entries[0].clone();
    entries[1].extension = "PNG".into();
    entries[1].details.filename = "second.png".into();
    entries[1].details.order_position = Some(0);
    entries[0].details.order_position = Some(1);
    assert_eq!(super::order_group(&entries, 0), vec![1, 0]);
    assert_eq!(entries[1].details.filename, "second.png");
    entries[1].details.artist = Some("Another artist".into());
    assert_eq!(super::order_group(&entries, 0), vec![0]);
}
