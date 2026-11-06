use super::Health;

#[test]
fn repairs_only_selected_category_and_preserves_remaining_counts() {
    let mut health = Health::new();
    health.choose(2);
    health.repair();
    assert_eq!(health.remaining, [true, true, false, true]);
    health.choose(0);
    assert!(!health.proposal);
    assert!(health.message.is_none());
    assert!(!health.remaining[2]);
}

#[test]
fn changing_category_discards_relink_confirmation() {
    let mut health = Health::new();
    health.proposal = true;
    health.confirmed = true;
    health.choose(1);
    assert!(!health.proposal);
    assert!(!health.confirmed);
    assert_eq!(health.remaining, [true; 4]);
}

#[test]
fn missing_file_repair_requires_both_proposal_and_confirmation() {
    let mut health = Health::new();
    health.repair();
    assert!(health.remaining[0]);
    health.confirmed = true;
    health.repair();
    assert!(health.remaining[0]);
    health.proposal = true;
    health.confirmed = false;
    health.repair();
    assert!(health.remaining[0]);
    health.confirmed = true;
    health.repair();
    assert!(!health.remaining[0]);
    assert!(!health.can_repair());
    assert!(!health.proposal);
    assert!(!health.confirmed);
}

#[test]
fn metadata_save_resolves_only_metadata_problem_and_keeps_session_values() {
    let mut health = Health::new();
    health.metadata_entry.title = "Updated title".into();
    health.metadata_saved();
    assert_eq!(health.remaining, [true, true, true, false]);
    assert_eq!(health.metadata_entry.title, "Updated title");
    health.choose(2);
    assert_eq!(health.metadata_entry.title, "Updated title");
    assert!(!health.remaining[3]);
}
