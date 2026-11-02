use std::fs;

use notify::event::{AccessKind, CreateKind, Flag, ModifyKind, RemoveKind};

use super::*;

#[test]
fn notifications_ignore_reads_and_nested_stage_but_keep_cross_boundary_renames() {
    let stage = PathBuf::from("library").join("Stage");
    let incoming = stage.join("new.bmp");
    let event = Event::new(EventKind::Create(CreateKind::File)).add_path(incoming.clone());
    assert!(relevant(&event, None));
    assert!(!relevant(&event, Some(&stage)));
    assert!(relevant(
        &event.add_path(PathBuf::from("library/kept.bmp")),
        Some(&stage)
    ));
    assert!(!relevant(
        &Event::new(EventKind::Access(AccessKind::Read)),
        None
    ));
    assert!(relevant(
        &Event::new(EventKind::Other).set_flag(Flag::Rescan),
        Some(&stage)
    ));

    for kind in [
        EventKind::Modify(ModifyKind::Any),
        EventKind::Remove(RemoveKind::File),
    ] {
        assert!(relevant(&Event::new(kind).add_path(incoming.clone()), None));
    }
}

#[test]
fn bursts_are_coalesced_without_starvation_and_busy_work_retains_pending_changes() {
    let start = Instant::now();
    let mut pending = Pending::new(start);
    assert!(!pending.ready(start + Duration::from_secs(1)));
    assert!(pending.ready(start + QUIET));

    pending.last = start + Duration::from_secs(9);
    assert!(!pending.ready(start + Duration::from_secs(9)));
    assert!(pending.ready(start + MAX_WAIT));

    let mut watch = SourceWatch::new("stage".into(), "stage", None);
    watch.pending = Some(pending);
    // Skipping a busy operation does not acknowledge pending work.
    assert!(watch.pending.as_ref().unwrap().ready(start + MAX_WAIT));
    for _ in 0..3 {
        watch.finished(1);
        assert!(watch.pending.is_some());
    }
    watch.finished(1);
    assert!(watch.pending.is_none());
    watch.changed.store(true, Ordering::Release);
    watch.finished(0);
    assert!(
        watch.changed.load(Ordering::Acquire),
        "Events during scans survive completion"
    );
}

#[test]
fn saved_preferences_select_roots_and_reconfigure_library_stage_exclusion() {
    let mut settings = AppConfig {
        library_path: Some("library".into()),
        staging_path: Some("library/Stage".into()),
        ..AppConfig::default()
    };
    let initial = sources(&settings);
    assert_eq!(initial.len(), 2);
    assert_eq!(initial[0].2, settings.staging_path);
    assert_eq!(initial[1].2, None);
    settings.staging_path = Some("other-stage".into());
    assert_ne!(sources(&settings), initial);
    settings.watch_folders = false;
    assert!(sources(&settings).is_empty());
}

fn wait_for_notification(watch: &SourceWatch) {
    let deadline = Instant::now() + Duration::from_secs(10);

    while !watch.changed.load(Ordering::Acquire) {
        assert!(
            Instant::now() < deadline,
            "Native folder notification timed out"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn native_watch_observes_nested_add_modify_delete_and_reattaches_missing_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Stage");
    let mut watch = SourceWatch::new(root.clone(), "stage", None).maintain();
    assert!(watch.watcher.is_none());
    assert!(watch.pending.is_none());
    assert!(!watch.changed.load(Ordering::Acquire));

    watch.attempted = None;
    watch = watch.maintain();
    assert!(
        watch.pending.is_none(),
        "Registration retries must not index"
    );

    fs::create_dir_all(root.join("nested")).unwrap();
    watch.attempted = None;
    watch = watch.maintain();
    assert!(watch.watcher.is_some());
    assert!(watch.pending.is_none(), "Attaching must not index");
    watch = watch.maintain();
    assert!(watch.pending.is_none(), "Idle watches must not index");
    watch.pending = None;
    watch.changed.store(false, Ordering::Release);

    let file = root.join("nested/new.bmp");
    fs::write(&file, b"first").unwrap();
    wait_for_notification(&watch);
    watch.changed.store(false, Ordering::Release);
    fs::write(&file, b"changed contents").unwrap();
    wait_for_notification(&watch);
    watch.changed.store(false, Ordering::Release);
    fs::remove_file(&file).unwrap();
    wait_for_notification(&watch);

    // Remove only disposable test directories, after releasing the native handle.
    watch.watcher = None;
    watch.changed.store(false, Ordering::Release);
    watch.finished(0);
    fs::remove_dir(root.join("nested")).unwrap();
    fs::remove_dir(&root).unwrap();
    watch.attempted = None;
    watch = watch.maintain();
    assert!(watch.watcher.is_none());
    fs::create_dir(&root).unwrap();
    watch.attempted = None;
    watch = watch.maintain();
    assert!(watch.watcher.is_some());
    assert!(watch.pending.is_none(), "Reattaching must not index");
}

#[test]
fn notifications_schedule_only_the_affected_source() {
    let library_root = PathBuf::from("library");
    let stage_root = library_root.join("Stage");
    let mut library = SourceWatch::new(library_root.clone(), "library", Some(stage_root.clone()));
    let mut stage = SourceWatch::new(stage_root.clone(), "stage", None);

    let event =
        Event::new(EventKind::Create(CreateKind::File)).add_path(stage_root.join("incoming.bmp"));
    library.changed.store(
        relevant(&event, library.excluded.as_deref()),
        Ordering::Release,
    );
    stage.changed.store(
        relevant(&event, stage.excluded.as_deref()),
        Ordering::Release,
    );

    // Avoid native registration: exercise the same notification consumption as maintenance.
    library.attempted = Some(Instant::now());
    stage.attempted = Some(Instant::now());
    library = library.maintain();
    stage = stage.maintain();
    assert!(library.pending.is_none());
    assert!(stage.pending.is_some());

    stage.finished(0);
    let event =
        Event::new(EventKind::Remove(RemoveKind::File)).add_path(library_root.join("removed.bmp"));
    library.changed.store(
        relevant(&event, library.excluded.as_deref()),
        Ordering::Release,
    );
    library = library.maintain();
    assert!(library.pending.is_some());
    assert!(stage.pending.is_none());
}
