use super::*;
use crate::config::{AppConfig, AppPaths, ConfigStore};

#[test]
fn flushes_config_successes_and_errors_to_plain_text_file() {
    let root = tempfile::tempdir().unwrap();

    let logs = root.path().join("logs");

    let (writer, guard) = file_writer(&logs).unwrap();

    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(writer)
        .finish();
    // Match production's global subscriber so parallel config tests cannot
    // disable shared callsites through their thread-local default dispatcher.
    tracing::subscriber::set_global_default(subscriber).unwrap();
    {
        let paths = AppPaths {
            config_path: root.path().join("config/settings.json"),
            database_path: root.path().join("db/vault.db"),
            cache_path: root.path().join("cache"),
            logs_path: logs.clone(),
        };

        let store = ConfigStore::load(paths.clone()).unwrap();
        store.save(AppConfig::default()).unwrap();
        std::fs::write(&paths.config_path, b"invalid json").unwrap();
        assert!(ConfigStore::load(paths).is_err());
    }
    drop(guard);

    let entries: Vec<_> = std::fs::read_dir(logs).unwrap().collect();
    assert_eq!(entries.len(), 1);

    let text = std::fs::read_to_string(entries[0].as_ref().unwrap().path()).unwrap();
    assert!(text.contains("Default configuration created"));
    assert!(text.contains("Configuration saved"));
    assert!(text.contains("Invalid configuration"));
    assert!(text.contains("ERROR"));
    assert!(!text.contains('\u{1b}'));
    assert!(!text.contains("characterSeparator"));
}
