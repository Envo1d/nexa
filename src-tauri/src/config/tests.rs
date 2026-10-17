use super::*;

#[test]
fn saves_report_only_disk_structure_changes() {
    let dir = tempfile::tempdir().unwrap();

    let store = ConfigStore::load(paths(dir.path())).unwrap();

    let mut draft = AppConfig {
        theme: AppTheme::Paper,
        locale: AppLocale::Russian,
        auto_scan: true,
        auto_update_check: true,
        staging_path: Some(dir.path().join("Stage")),
        ..AppConfig::default()
    };
    assert!(!store.save(draft.clone()).unwrap());
    assert!(!store.save(draft.clone()).unwrap());

    for field in 0..7 {
        match field {
            0 => draft.sync_structure = true,
            1 => draft.library_path = Some(dir.path().join("Library")),
            2 => draft.copyright_directory = false,
            3 => draft.artist_directory = false,
            4 => draft.character_separator = " & ".into(),
            5 => draft.video_subfolder = "Clips".into(),
            6 => draft.gif_subfolder = "GIF".into(),
            _ => unreachable!(),
        }
        assert!(store.save(draft.clone()).unwrap());
        assert!(!store.save(draft.clone()).unwrap());
    }
}

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
        locale: AppLocale::Russian,
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
    assert_eq!(old.locale, AppLocale::System);
    assert_eq!(old.theme, AppTheme::MidnightViolet);

    let explicit: AppConfig = serde_json::from_str(r#"{"locale":"en"}"#).unwrap();
    assert_eq!(explicit.locale, AppLocale::English);
}

#[test]
fn rejects_unsupported_locales_and_serializes_supported_language_codes() {
    assert!(serde_json::from_str::<AppConfig>(r#"{"locale":"invalid"}"#).is_err());
    assert_eq!(
        serde_json::to_string(&AppLocale::System).unwrap(),
        "\"system\""
    );
    assert_eq!(
        serde_json::to_string(&AppLocale::English).unwrap(),
        "\"en\""
    );
    assert_eq!(
        serde_json::to_string(&AppLocale::Russian).unwrap(),
        "\"ru\""
    );
}

#[test]
fn system_language_preference_survives_save_and_reload() {
    let dir = tempfile::tempdir().unwrap();

    let paths = paths(dir.path());

    let store = ConfigStore::load(paths.clone()).unwrap();
    store
        .save(AppConfig {
            locale: AppLocale::Russian,
            ..AppConfig::default()
        })
        .unwrap();
    store.save(AppConfig::default()).unwrap();

    let reopened = ConfigStore::load(paths).unwrap();
    assert_eq!(
        reopened.snapshot().unwrap().settings.locale,
        AppLocale::System
    );
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

#[test]
fn all_themes_survive_save_and_reload_and_invalid_themes_are_rejected() {
    let directory = tempfile::tempdir().unwrap();

    let paths = paths(directory.path());

    let store = ConfigStore::load(paths.clone()).unwrap();

    let themes = [
        (AppTheme::MidnightViolet, "midnight-violet"),
        (AppTheme::Graphite, "graphite"),
        (AppTheme::ArcticBlue, "arctic-blue"),
        (AppTheme::Forest, "forest"),
        (AppTheme::WarmAmber, "warm-amber"),
        (AppTheme::Rosewood, "rosewood"),
        (AppTheme::Paper, "paper"),
    ];

    for (theme, name) in themes {
        let draft = AppConfig {
            theme,
            ..AppConfig::default()
        };

        store.save(draft.clone()).unwrap();

        let reloaded = ConfigStore::load(paths.clone()).unwrap();

        assert_eq!(reloaded.snapshot().unwrap().settings, draft);
        assert_eq!(serde_json::to_value(theme).unwrap(), name);
    }

    assert!(serde_json::from_str::<AppConfig>(r#"{"theme":"unknown"}"#).is_err());
}
