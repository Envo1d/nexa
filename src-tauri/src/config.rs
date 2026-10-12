use std::{fs, io::Write, path::PathBuf, sync::Mutex};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

type ConfigResult<T> = Result<T, String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct AppConfig {
    pub library_path: Option<PathBuf>,
    pub staging_path: Option<PathBuf>,
    pub copyright_directory: bool,
    pub artist_directory: bool,
    pub character_separator: String,
    pub video_subfolder: String,
    pub gif_subfolder: String,
    pub sync_structure: bool,
    pub auto_scan: bool,
    pub auto_update_check: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            library_path: None,
            staging_path: None,
            copyright_directory: true,
            artist_directory: true,
            character_separator: " х ".into(),
            video_subfolder: "Videos".into(),
            gif_subfolder: "Animations".into(),
            sync_structure: false,
            auto_scan: false,
            auto_update_check: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub config_path: PathBuf,
    pub database_path: PathBuf,
    pub cache_path: PathBuf,
    pub logs_path: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> ConfigResult<Self> {
        let dirs = ProjectDirs::from("com", "Envo1d", "NexaNew")
            .ok_or("Unable to locate application system folders")?;
        Ok(Self {
            config_path: dirs.config_dir().join("settings.json"),
            database_path: dirs.data_local_dir().join("db/vault.db"),
            cache_path: dirs.cache_dir().join("thumbnails"),
            logs_path: dirs.data_local_dir().join("logs"),
        })
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSnapshot {
    pub settings: AppConfig,
    pub paths: AppPaths,
}

pub struct ConfigStore {
    pub paths: AppPaths,
    settings: Mutex<AppConfig>,
}

impl ConfigStore {
    #[tracing::instrument(skip_all, fields(config_path = %paths.config_path.display()), err)]
    pub fn load(paths: AppPaths) -> ConfigResult<Self> {
        let settings = match fs::read(&paths.config_path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
                format!(
                    "Invalid configuration {}: {error}",
                    paths.config_path.display()
                )
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let defaults = AppConfig::default();
                Self::write(&paths, &defaults, false)?;

                tracing::info!("Default configuration created");
                defaults
            }
            Err(error) => return Err(format!("Cannot read configuration: {error}")),
        };

        tracing::info!("Configuration loaded");
        Ok(Self {
            paths,
            settings: Mutex::new(settings),
        })
    }

    fn write(paths: &AppPaths, settings: &AppConfig, replace: bool) -> ConfigResult<()> {
        let parent = paths
            .config_path
            .parent()
            .ok_or("Invalid configuration path")?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("Cannot create config directory: {error}"))?;

        let mut file = tempfile::NamedTempFile::new_in(parent)
            .map_err(|error| format!("Cannot create temporary config: {error}"))?;

        let json = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
        file.write_all(&json)
            .map_err(|error| format!("Cannot write configuration: {error}"))?;
        file.as_file()
            .sync_all()
            .map_err(|error| format!("Cannot flush configuration: {error}"))?;

        let result = if replace {
            file.persist(&paths.config_path)
        } else {
            file.persist_noclobber(&paths.config_path)
        };
        result.map_err(|error| format!("Cannot commit configuration: {error}"))?;
        Ok(())
    }

    #[tracing::instrument(skip_all, err)]
    pub fn snapshot(&self) -> ConfigResult<ConfigSnapshot> {
        tracing::debug!("Reading configuration snapshot");

        let settings = self
            .settings
            .lock()
            .map_err(|_| "Configuration lock poisoned")?;
        Ok(ConfigSnapshot {
            settings: settings.clone(),
            paths: self.paths.clone(),
        })
    }

    #[tracing::instrument(skip_all, fields(config_path = %self.paths.config_path.display()), err)]
    pub fn save(&self, draft: AppConfig) -> ConfigResult<()> {
        let mut settings = self
            .settings
            .lock()
            .map_err(|_| "Configuration lock poisoned")?;
        Self::write(&self.paths, &draft, true)?;
        *settings = draft;

        tracing::info!("Configuration saved");
        Ok(())
    }
}

#[tauri::command]
pub fn get_config(store: tauri::State<'_, ConfigStore>) -> ConfigResult<ConfigSnapshot> {
    store.snapshot()
}

#[tauri::command]
pub fn save_config(
    app: tauri::AppHandle,
    settings: AppConfig,
    store: tauri::State<'_, ConfigStore>,
) -> ConfigResult<()> {
    store.save(settings)?;
    crate::disk_sync::request(&app);
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip_all, fields(logs_path = %store.paths.logs_path.display()), err)]
pub fn open_logs(app: tauri::AppHandle, store: tauri::State<'_, ConfigStore>) -> ConfigResult<()> {
    use tauri_plugin_opener::OpenerExt;

    tracing::debug!("Opening logs directory");
    app.opener()
        .open_path(store.paths.logs_path.to_string_lossy(), None::<&str>)
        .map_err(|error| error.to_string())?;

    tracing::info!("Logs directory open requested");
    Ok(())
}

#[cfg(test)]
mod tests;
