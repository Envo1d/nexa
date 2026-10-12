use super::*;

fn paths(root: &std::path::Path) -> AppPaths {
    AppPaths {
        config_path: root.join("config/settings.json"),
        database_path: root.join("data/db/vault.db"),
        cache_path: root.join("cache/thumbnails"),
        logs_path: root.join("data/logs"),
    }
}

#[test]
fn creates_defaults_and_round_trips_replacement() {
    let dir = tempfile::tempdir().unwrap();

    let paths = paths(dir.path());

    let store = ConfigStore::load(paths.clone()).unwrap();
    assert_eq!(store.snapshot().unwrap().settings, AppConfig::default());

    let draft = AppConfig {
        library_path: Some(dir.path().join("Медиа")),
        artist_directory: false,
        auto_scan: true,
        gif_subfolder: "GIFs".into(),
        ..AppConfig::default()
    };
    store.save(draft.clone()).unwrap();
    assert_eq!(
        ConfigStore::load(paths)
            .unwrap()
            .snapshot()
            .unwrap()
            .settings,
        draft
    );
    assert_eq!(fs::read_dir(dir.path().join("config")).unwrap().count(), 1);
}

#[test]
fn preserves_invalid_configuration_and_reports_read_errors() {
    let dir = tempfile::tempdir().unwrap();

    let paths = paths(dir.path());
    fs::create_dir_all(paths.config_path.parent().unwrap()).unwrap();
    fs::write(&paths.config_path, b"broken json").unwrap();
    assert!(ConfigStore::load(paths.clone()).is_err());
    assert_eq!(fs::read(&paths.config_path).unwrap(), b"broken json");
    fs::remove_file(&paths.config_path).unwrap();
    fs::create_dir(&paths.config_path).unwrap();
    assert!(ConfigStore::load(paths).is_err());
}

#[test]
fn failed_save_preserves_committed_state() {
    let dir = tempfile::tempdir().unwrap();

    let store = ConfigStore::load(paths(dir.path())).unwrap();
    fs::remove_file(&store.paths.config_path).unwrap();
    fs::create_dir(&store.paths.config_path).unwrap();
    assert!(
        store
            .save(AppConfig {
                auto_scan: true,
                ..AppConfig::default()
            })
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap().settings, AppConfig::default());
}

#[test]
fn missing_fields_use_current_defaults_and_unknown_fields_are_rejected() {
    assert_eq!(
        serde_json::from_str::<AppConfig>("{}").unwrap(),
        AppConfig::default()
    );
    assert!(serde_json::from_str::<AppConfig>(r#"{"databasePath":"legacy.db"}"#).is_err());
    assert_eq!(AppConfig::default().character_separator, " х ");

    let old: AppConfig = serde_json::from_str(r#"{"videoSubfolder":"Clips"}"#).unwrap();
    assert_eq!(old.gif_subfolder, "Animations");
    assert_eq!(old.video_subfolder, "Clips");
}

#[test]
fn service_paths_are_isolated_from_legacy_application() {
    let new = AppPaths::resolve().unwrap();

    let old = ProjectDirs::from("com", "Envo1d", "Nexa").unwrap();
    assert!(!new.config_path.starts_with(old.config_dir()));
    assert!(!new.database_path.starts_with(old.data_local_dir()));
    assert!(!new.cache_path.starts_with(old.cache_dir()));
    assert!(!new.logs_path.starts_with(old.data_local_dir()));
}
