//! Logical Main contracts and feature facade; disk work is queued separately.

mod metadata;
mod queries;

pub(crate) use metadata::write_tags;

pub use metadata::{
    __cmd__distribute_files, __cmd__save_library_metadata, __cmd__save_library_tags,
    __cmd__set_library_favorite, __tauri_command_name_distribute_files,
    __tauri_command_name_save_library_metadata, __tauri_command_name_save_library_tags,
    __tauri_command_name_set_library_favorite, distribute_files, save_library_metadata,
    save_library_tags, set_library_favorite,
};
pub(crate) use queries::viewer_sequence;
pub use queries::{
    __cmd__get_library_files, __tauri_command_name_get_library_files, get_library_files,
};
pub(crate) use queries::{apply_metadata_search, metadata_rank};

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{config::ConfigStore, stage};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filters {
    scope: String,
    search: String,
    kind: String,
    pub(crate) sort: String,
    tagged: bool,
    offset: i64,
    #[serde(skip)]
    stage_prefix: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Character {
    name: String,
    copyright: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    #[serde(flatten)]
    technical: stage::StageFile,
    favorite: bool,
    copyrights: Vec<String>,
    artist: String,
    character_copyrights: Vec<Character>,
    tags: Vec<String>,
    video_title: Option<String>,
}

#[derive(Serialize)]
pub struct Count {
    name: String,
    count: i64,
}

#[derive(Serialize)]
pub struct Suggestion {
    name: String,
    category: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    files: Vec<File>,
    total: i64,
    type_counts: [i64; 3],
    tags: Vec<Count>,
    suggestions: Vec<Suggestion>,
    characters: Vec<Character>,
}

fn configured_root(config: &ConfigStore) -> Result<Option<String>, String> {
    Ok(config.snapshot()?.settings.library_path.map(|p| {
        p.components()
            .collect::<PathBuf>()
            .to_string_lossy()
            .into_owned()
    }))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use queries::browse;
