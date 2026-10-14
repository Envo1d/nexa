use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use velopack::{VelopackAsset, locator::VelopackLocatorConfig};

use crate::config::{AppLocale, AppTheme};

#[derive(Deserialize, Serialize)]
pub(super) struct Request {
    pub asset: VelopackAsset,
    pub installation: VelopackLocatorConfig,
    pub logs: PathBuf,
    pub parent_pid: u32,
    #[serde(default = "default_locale")]
    pub locale: AppLocale,
    #[serde(default)]
    pub theme: AppTheme,
}

fn default_locale() -> AppLocale {
    AppLocale::English
}

// Publish complete files, so neither process can observe partially written JSON.
pub(super) fn write<T: Serialize>(directory: &Path, name: &str, value: &T) -> Result<(), String> {
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;

    serde_json::to_writer(&mut file, value).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    file.persist(directory.join(name))
        .map_err(|error| error.to_string())?;

    Ok(())
}

pub(super) fn read<T: for<'a> Deserialize<'a>>(directory: &Path, name: &str) -> Result<T, String> {
    let file = fs::File::open(directory.join(name)).map_err(|error| error.to_string())?;

    serde_json::from_reader(file).map_err(|error| error.to_string())
}

pub(super) fn wait_for_launch(directory: &Path, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if directory.join("launched.json").exists() {
            return read::<Result<(), String>>(directory, "launched.json")?;
        }

        thread::sleep(Duration::from_millis(100));
    }

    Err("The update window did not become ready in time.".into())
}

pub(super) fn await_commit(directory: &Path, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if directory.join("cancel.json").exists() {
            return Err("Update handoff cancelled. Nexa remains open; retry from Settings.".into());
        }

        if directory.join("commit.json").exists() {
            return Ok(());
        }

        thread::sleep(Duration::from_millis(100));
    }

    Err("Nexa did not confirm the update. Retry from Settings.".into())
}
