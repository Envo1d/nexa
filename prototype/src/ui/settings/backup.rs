use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::catalog::Entry;

const LIMIT: u64 = 32 * 1024 * 1024;
const FORMAT: &str = "nexa-gpui-session-backup";

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Snapshot {
    pub format: String,
    pub version: u32,
    pub created_at: i64,
    pub roots: [Option<String>; 2],
    pub library: Vec<Entry>,
    pub incoming: Vec<Entry>,
}

pub(super) struct Preview {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub snapshot: Snapshot,
}

impl Snapshot {
    pub fn new(roots: [Option<String>; 2], library: Vec<Entry>, incoming: Vec<Entry>) -> Self {
        Self {
            format: FORMAT.into(),
            version: 1,
            created_at: chrono::Utc::now().timestamp_millis(),
            roots,
            library,
            incoming,
        }
    }
}

pub(super) fn inspect(path: PathBuf) -> Result<Preview, String> {
    let bytes = read(&path)?;
    let snapshot = decode(&bytes)?;

    Ok(Preview {
        path,
        bytes,
        snapshot,
    })
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|_| "Не удалось открыть резервную копию.")?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Не удалось прочитать резервную копию.")?;
    if bytes.len() as u64 > LIMIT {
        return Err("Резервная копия превышает 32 МБ.".into());
    }

    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<Snapshot, String> {
    let snapshot: Snapshot = serde_json::from_slice(bytes).map_err(|_| {
        "Некорректная копия сеанса GPUI. Производственные копии Tauri не поддерживаются.".to_owned()
    })?;
    if snapshot.format != FORMAT
        || snapshot.version != 1
        || snapshot.library.len() + snapshot.incoming.len() > 100_000
    {
        return Err("Неподдерживаемый формат или размер копии сеанса GPUI.".into());
    }

    Ok(snapshot)
}

pub(super) fn write(path: &Path, snapshot: &Snapshot, portable: bool) -> Result<(), String> {
    let mut value = serde_json::to_value(snapshot).map_err(|_| "Не удалось подготовить копию.")?;
    if portable {
        value["format"] = "nexa-gpui-session-metadata".into();
    }
    let bytes = serde_json::to_vec_pretty(&value).map_err(|_| "Не удалось подготовить копию.")?;
    if bytes.len() as u64 > LIMIT {
        return Err("Резервная копия превышает 32 МБ.".into());
    }
    let parent = path.parent().ok_or("Не выбрана папка назначения.")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "Не удалось создать временный файл.")?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| "Не удалось записать копию.")?;
    temporary.persist_noclobber(path).map_err(|error| {
        format!("Не удалось сохранить копию ({:?}, {:?}). Выберите новое имя: существующие файлы не заменяются.", error.error.kind(), error.error.raw_os_error())
    })?;

    Ok(())
}

pub(super) fn prepare_restore(preview: &Preview, current: &Snapshot) -> Result<PathBuf, String> {
    if read(&preview.path)? != preview.bytes {
        return Err("Файл изменился после предпросмотра. Выберите копию повторно.".into());
    }
    let root = std::env::var_os("LOCALAPPDATA").ok_or("Не найдена папка данных приложения.")?;
    let directory = PathBuf::from(root).join("NexaNew/gpui-prototype/catalog-backups");
    fs::create_dir_all(&directory).map_err(|_| "Не удалось создать папку защитной копии.")?;
    let path = directory.join(format!(
        "before-restore-{}.json",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    write(&path, current, false)?;

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_round_trip_rejects_portable_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backup.json");
        let mut entries = crate::catalog::incoming_demo_entries();
        entries[0].tags = vec!["custom".into()];
        let snapshot = Snapshot::new([Some("demo".into()), None], entries, vec![]);
        write(&path, &snapshot, false).unwrap();
        let preview = inspect(path.clone()).unwrap();
        assert_eq!(preview.snapshot.library[0].tags, ["custom"]);
        assert!(write(&path, &snapshot, false).is_err());
        assert_eq!(fs::read(&path).unwrap(), preview.bytes);
        let portable = dir.path().join("metadata.json");
        write(&portable, &snapshot, true).unwrap();
        assert!(inspect(portable).is_err());
        assert!(decode(br#"{"format":"nexa-catalog-backup"}"#).is_err());
    }

    #[test]
    fn changed_backup_is_rejected_before_creating_safety_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backup.json");
        let snapshot = Snapshot::new([None, None], vec![], vec![]);
        write(&path, &snapshot, false).unwrap();
        let preview = inspect(path.clone()).unwrap();
        fs::write(path, b"changed").unwrap();
        assert!(
            prepare_restore(&preview, &snapshot)
                .unwrap_err()
                .contains("изменился")
        );
    }
}
