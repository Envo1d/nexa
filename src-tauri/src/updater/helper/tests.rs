use std::{thread, time::Duration};

use super::session;

#[test]
fn parent_rejects_helper_launch_failure() {
    let directory = tempfile::tempdir().unwrap();

    session::write(
        directory.path(),
        "launched.json",
        &Err::<(), _>("launch failed"),
    )
    .unwrap();

    let error = session::wait_for_launch(directory.path(), Duration::from_secs(1)).unwrap_err();

    assert_eq!(error, "launch failed");
    assert!(!directory.path().join("commit.json").exists());
}

#[test]
fn parent_does_not_accept_unready_interface() {
    let directory = tempfile::tempdir().unwrap();

    assert!(session::wait_for_launch(directory.path(), Duration::from_millis(10)).is_err());
}

#[test]
fn helper_requires_explicit_parent_commit() {
    let directory = tempfile::tempdir().unwrap();

    session::write(directory.path(), "launched.json", &Ok::<(), String>(())).unwrap();

    assert!(session::await_commit(directory.path(), Duration::from_millis(10)).is_err());

    session::write(directory.path(), "commit.json", &true).unwrap();

    assert!(session::await_commit(directory.path(), Duration::from_secs(1)).is_ok());
}

#[test]
fn cancellation_overrides_commit() {
    let directory = tempfile::tempdir().unwrap();

    session::write(directory.path(), "commit.json", &true).unwrap();
    session::write(directory.path(), "cancel.json", &true).unwrap();

    assert!(session::await_commit(directory.path(), Duration::from_secs(1)).is_err());
}

#[test]
fn helper_observes_commit_from_independent_worker() {
    let directory = tempfile::tempdir().unwrap();

    let path = directory.path().to_path_buf();

    let worker = thread::spawn(move || session::await_commit(&path, Duration::from_secs(2)));

    session::write(directory.path(), "commit.json", &true).unwrap();

    assert!(worker.join().unwrap().is_ok());
}
