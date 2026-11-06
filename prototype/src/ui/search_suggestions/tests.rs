use super::*;

fn record(name: &str, category: Category, copyright: &str) -> Record {
    Record {
        id: 0,
        name: name.into(),
        category,
        copyright: copyright.into(),
        count: 1,
    }
}

#[test]
fn ranks_prefix_excludes_previous_terms_and_preserves_qualified_characters() {
    let records = vec![
        record("Cat", Category::Tag, ""),
        record("Bobcat", Category::Tag, ""),
        record("Cat", Category::Character, "A"),
        record("Cat", Category::Character, "B"),
        record("Cat", Category::Tag, ""),
    ];
    let values = matches("ca", &records);
    assert_eq!(values.len(), 4);
    assert_eq!(values.last().unwrap().name, "Bobcat");
    assert_eq!(matches("CAT; ca", &records).len(), 1);
    assert!(matches("Cat; ", &records).is_empty());
}

#[test]
fn caps_matches_and_replaces_only_final_term() {
    let records: Vec<_> = (0..40)
        .map(|i| record(&format!("tag {i}"), Category::Tag, ""))
        .collect();
    assert_eq!(matches("tag", &records).len(), 30);
    assert_eq!(choose(" Artist ; ; par", "Character"), "Artist; Character");
    assert_eq!(choose("partial", "Tag"), "Tag");
}

#[test]
fn keyboard_navigation_wraps_and_handles_empty_results() {
    assert_eq!(next_index(None, 3, false), Some(2));
    assert_eq!(next_index(None, 3, true), Some(0));
    assert_eq!(next_index(Some(2), 3, true), Some(0));
    assert_eq!(next_index(Some(0), 3, false), Some(2));
    assert_eq!(next_index(None, 0, true), None);
}

#[test]
fn library_draft_does_not_filter_until_committed_and_incoming_remains_live() {
    use crate::catalog::{self, BrowseFilter};

    let entries = catalog::demo_entries();
    let mut state = Suggestions::default();
    let draft = "nonexistent search value";
    let browse =
        |query: &str| catalog::browse_with_counts(&entries, query, BrowseFilter::default());

    assert_eq!(
        browse(&state.collection_query(draft, false)).0.len(),
        entries.len()
    );
    state.applied = draft.into();
    assert!(browse(&state.collection_query(draft, false)).0.is_empty());
    assert!(browse(&state.collection_query("", false)).0.is_empty());
    assert_eq!(
        state.collection_query("incoming filename", true),
        "incoming filename"
    );
    assert_eq!(state.collection_query("different draft", false), draft);
    state.applied.clear();
    assert_eq!(
        browse(&state.collection_query(draft, false)).0.len(),
        entries.len()
    );
}
