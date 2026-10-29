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
fn development_storage_is_separate_and_production_paths_are_preserved() {
    let development = AppPaths::resolve_for(true).unwrap();
    let production = AppPaths::resolve_for(false).unwrap();
    let dirs = ProjectDirs::from("com", "Envo1d", "NexaNew").unwrap();

    for (dev, prod, root, suffix) in [
        (
            &development.config_path,
            &production.config_path,
            dirs.config_dir(),
            "settings.json",
        ),
        (
            &development.database_path,
            &production.database_path,
            dirs.data_local_dir(),
            "db/vault.db",
        ),
        (
            &development.cache_path,
            &production.cache_path,
            dirs.cache_dir(),
            "thumbnails",
        ),
        (
            &development.logs_path,
            &production.logs_path,
            dirs.data_local_dir(),
            "logs",
        ),
    ] {
        assert_eq!(*prod, root.join(suffix));
        assert_eq!(*dev, root.join("dev").join(suffix));
        assert_ne!(dev, prod);
    }

    let selected = AppPaths::resolve().unwrap();
    let expected = if cfg!(debug_assertions) {
        development
    } else {
        production
    };

    assert_eq!(selected.database_path, expected.database_path);
    assert_eq!(selected.config_path, expected.config_path);
    assert_eq!(selected.cache_path, expected.cache_path);
    assert_eq!(selected.logs_path, expected.logs_path);
}

#[tokio::test]
async fn separate_catalogs_and_settings_persist_without_inheriting_production_data() {
    let directory = tempfile::tempdir().unwrap();
    let production = paths(directory.path());
    let development = paths(&directory.path().join("dev"));

    let store = ConfigStore::load(production.clone()).unwrap();
    let saved = AppConfig {
        library_path: Some(directory.path().join("production-media")),
        auto_scan: true,
        sync_structure: true,
        ..AppConfig::default()
    };
    store.save(saved.clone()).unwrap();

    let production_database = crate::database::Database::open(&production.database_path)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tags(name,name_key) VALUES ('Production','production')")
        .execute(production_database.pool())
        .await
        .unwrap();

    let development_store = ConfigStore::load(development.clone()).unwrap();
    assert_eq!(
        development_store.snapshot().unwrap().settings,
        AppConfig::default()
    );

    let development_database = crate::database::Database::open(&development.database_path)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tags")
        .fetch_one(development_database.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);

    sqlx::query("INSERT INTO tags(name,name_key) VALUES ('Development','development')")
        .execute(development_database.pool())
        .await
        .unwrap();
    development_database.close().await;

    let reopened = crate::database::Database::open(&development.database_path)
        .await
        .unwrap();
    let development_name: String = sqlx::query_scalar("SELECT name FROM tags")
        .fetch_one(reopened.pool())
        .await
        .unwrap();
    let production_name: String = sqlx::query_scalar("SELECT name FROM tags")
        .fetch_one(production_database.pool())
        .await
        .unwrap();

    assert_eq!(development_name, "Development");
    assert_eq!(production_name, "Production");
    assert_eq!(
        ConfigStore::load(production)
            .unwrap()
            .snapshot()
            .unwrap()
            .settings,
        saved
    );

    reopened.close().await;
    production_database.close().await;
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
