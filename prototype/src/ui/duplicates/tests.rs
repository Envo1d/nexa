use super::{Action, Duplicates};

fn completed() -> Duplicates {
    let mut state = Duplicates::new();
    state.processed = state.pairs.len();
    state
}

#[test]
fn results_are_unavailable_before_their_batch_completes() {
    let mut state = Duplicates::new();
    assert_eq!(state.count(true), 0);
    assert_eq!(state.count(false), 0);
    state.processed = 1;
    assert_eq!(state.count(true), 1);
    assert_eq!(state.count(false), 0);
}

#[test]
fn exact_pairs_cannot_be_rejected() {
    let mut state = completed();
    state.selected = Some(0);
    assert!(!state.reject());
    assert!(state.rejected.is_empty());
    assert_eq!(state.count(true), 2);
}

#[test]
fn rejecting_and_undoing_restores_the_same_similar_pair() {
    let mut state = completed();
    state.select_mode(false);
    state.selected = Some(1);
    assert!(state.reject());
    assert_eq!(state.count(false), 1);
    assert!(state.selected_pair().is_none());
    state.select_mode(true);
    assert!(state.undo());
    assert!(!state.exact);
    assert_eq!(state.selected_pair().unwrap().id, 1);
    assert_eq!(state.count(false), 2);
    assert!(!state.undo());
}

#[test]
fn mutations_require_confirmation_and_are_blocked_during_search() {
    let mut state = completed();
    state.selected = Some(1);
    assert!(!state.confirm());
    state.confirmation = Some(Action::Delete(0));
    state.running = true;
    assert!(!state.confirm());
    assert!(!state.reject());
    state.running = false;
    assert!(state.confirm());
    assert_eq!(state.count(false), 1);
    assert!(state.selected_pair().is_none());
    assert!(!state.confirm());
}

#[test]
fn switching_category_cancels_pending_confirmation() {
    let mut state = completed();
    state.selected = Some(0);
    state.confirmation = Some(Action::Replace);
    state.select_mode(false);
    assert!(state.confirmation.is_none());
    assert!(state.selected.is_none());
    assert!(!state.confirm());
}
