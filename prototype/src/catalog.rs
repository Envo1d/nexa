use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

pub(super) const DEMO_COUNT: usize = 10_000;
pub(super) const MAX_OPEN_FILES: usize = 64;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum SortMode {
    #[default]
    Newest,
    Oldest,
    NameAscending,
    NameDescending,
}

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Newest => "Сначала новые",
            Self::Oldest => "Сначала старые",
            Self::NameAscending => "По имени А → Я",
            Self::NameDescending => "По имени Я → А",
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct BrowseFilter {
    pub path_search: bool,
    pub videos_only: bool,
    pub images_only: bool,
    pub favorites_only: bool,
    pub tagged_only: bool,
    pub sort: SortMode,
}

pub(super) fn browse_with_counts(
    entries: &[Entry],
    query: &str,
    filter: BrowseFilter,
) -> (Vec<usize>, [usize; 3]) {
    let mut visible = if filter.path_search {
        let query = crate::naming::name_key(query.trim());
        entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                crate::naming::name_key(&entry.details.filename).contains(&query)
                    || crate::naming::name_key(&entry.details.relative_path).contains(&query)
            })
            .map(|(index, _)| index)
            .collect()
    } else {
        matching(entries, query, false)
    };
    visible.retain(|&index| {
        let entry = &entries[index];
        let favorite_matches = !filter.favorites_only || entry.favorite;
        let tags_match = !filter.tagged_only || !entry.tags.is_empty();

        favorite_matches && tags_match
    });
    let videos = visible
        .iter()
        .filter(|&&index| entries[index].video)
        .count();
    let counts = [visible.len(), videos, visible.len() - videos];
    visible.retain(|&index| {
        (!filter.images_only || !entries[index].video)
            && (!filter.videos_only || entries[index].video)
    });
    visible.sort_by(|&left, &right| {
        let order = match filter.sort {
            SortMode::NameAscending => crate::naming::compare_names(
                &entries[left].details.filename,
                &entries[right].details.filename,
            ),
            SortMode::NameDescending => crate::naming::compare_names(
                &entries[right].details.filename,
                &entries[left].details.filename,
            ),
            SortMode::Oldest => entries[left].modified.cmp(&entries[right].modified),
            SortMode::Newest => entries[right].modified.cmp(&entries[left].modified),
        };

        order.then_with(|| left.cmp(&right))
    });

    (visible, counts)
}

#[derive(Clone)]
pub(super) struct Entry {
    pub title: String,
    pub path: PathBuf,
    pub video: bool,
    pub demo: bool,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub extension: String,
    pub characters: String,
    pub duration: Option<String>,
    pub date_label: Option<String>,
    pub modified: Option<SystemTime>,
    pub details: EntryDetails,
}

#[derive(Clone, Default)]
pub(super) struct EntryDetails {
    pub order_position: Option<usize>,
    pub character_copyrights: Vec<(String, String)>,
    pub filename: String,
    pub relative_path: String,
    pub artist: Option<String>,
    pub copyrights: Vec<String>,
    pub size: Option<u64>,
    pub resolution: Option<(u32, u32)>,
    pub created_label: Option<String>,
}

pub(super) fn demo_entries() -> Vec<Entry> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/assets/previews");
    (0..DEMO_COUNT)
        .map(|index| Entry {
            title: demo_title(index).into(),
            path: assets.join(format!("scene-{}.webp", index % 12)),
            video: matches!(index % 12, 1 | 4 | 7 | 10 | 11),
            demo: true,
            tags: if index > 11 && index % 9 == 0 {
                Vec::new()
            } else {
                demo_tags(index).iter().map(|tag| (*tag).into()).collect()
            },
            favorite: index % 7 == 1 || index % 11 == 0,
            extension: if matches!(index % 12, 1 | 4 | 7 | 10 | 11) {
                "MP4"
            } else if index % 12 == 8 {
                "PNG"
            } else {
                "JPG"
            }
            .into(),
            characters: demo_characters(index).into(),
            duration: matches!(index % 12, 1 | 4 | 7 | 10 | 11).then(|| {
                let seconds = [21, 75, 8, 156, 42][index % 5];
                format!("{:02}:{:02}", seconds / 60, seconds % 60)
            }),
            date_label: Some(demo_date(index)),
            modified: Some(
                SystemTime::UNIX_EPOCH + Duration::from_secs(1_791_402_300 - index as u64 * 5_760),
            ),
            details: demo_details(index),
        })
        .collect()
}

pub(super) fn incoming_demo_entries() -> Vec<Entry> {
    demo_entries()
        .into_iter()
        .take(48)
        .enumerate()
        .map(|(index, mut entry)| {
            entry.title = format!(
                "incoming-{:03}.{}",
                index + 1,
                entry.extension.to_lowercase()
            );
            entry.details.filename = entry.title.clone();
            entry.details.relative_path = if index % 3 == 0 {
                entry.title.clone()
            } else {
                format!(
                    "Batch {}/{}/{}",
                    index / 12 + 1,
                    if entry.video { "Videos" } else { "Images" },
                    entry.title
                )
            };
            entry.tags.clear();
            entry.characters.clear();
            entry.details.character_copyrights.clear();
            entry.details.artist = None;
            entry.details.copyrights.clear();
            entry.favorite = false;

            entry
        })
        .collect()
}

pub(super) fn entry(path: PathBuf) -> Option<Entry> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let video = matches!(
        extension.as_str(),
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v"
    );
    let image = matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "tif" | "tiff" | "avif" | "heic" | "heif"
    );
    if !video && !image {
        return None;
    }

    let metadata = std::fs::metadata(&path).ok();
    let modified = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok());
    let filename = path.file_name()?.to_string_lossy().into_owned();
    let details = EntryDetails {
        filename: filename.clone(),
        relative_path: path.to_string_lossy().into_owned(),
        size: metadata.as_ref().map(|metadata| metadata.len()),
        created_label: metadata
            .as_ref()
            .and_then(|metadata| metadata.created().ok())
            .map(format_date),
        ..Default::default()
    };

    Some(Entry {
        title: filename,
        path,
        video,
        demo: false,
        tags: Vec::new(),
        favorite: false,
        extension: extension.to_uppercase(),
        characters: String::new(),
        duration: None,
        date_label: modified.map(format_date),
        modified,
        details,
    })
}

fn format_date(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(time)
        .format("%d.%m.%Y %H:%M")
        .to_string()
}

fn demo_details(index: usize) -> EntryDetails {
    let video = matches!(index % 12, 1 | 4 | 7 | 10 | 11);
    let group = (index + index / 12) % 6;
    let artist = format!(
        "{} {:03}",
        ["Mira", "Sora", "Aster", "Yuki"][index / 12 % 4],
        index + 1
    );
    let title = if video {
        format!(" - {}", demo_title(index))
    } else {
        String::new()
    };
    let extension = if video {
        "mp4"
    } else if index % 12 == 8 {
        "png"
    } else {
        "jpg"
    };
    let filename = format!(
        "{}{title} [{artist}].{extension}",
        demo_characters(index).replace(", ", " х ")
    );
    let copyrights: Vec<String> = if matches!(group, 2 | 5) {
        vec!["Wonderland".into(), "Lunar Tales".into()]
    } else {
        vec![
            if group < 3 {
                "Wonderland"
            } else {
                "Lunar Tales"
            }
            .into(),
        ]
    };

    EntryDetails {
        order_position: None,
        character_copyrights: demo_characters(index)
            .split(", ")
            .map(|name| (name.to_owned(), copyrights[0].clone()))
            .collect(),
        filename,
        relative_path: String::new(),
        artist: Some(artist),
        copyrights,
        size: Some(if video {
            ((128.0 + index as f64 * 1.35) * 1024.0_f64.powi(2)) as u64
        } else {
            (3 + index % 15) as u64 * 1024 * 1024
        }),
        resolution: Some(if video { (3840, 2160) } else { (6000, 4000) }),
        created_label: Some(demo_date(index + 15)),
    }
}

pub(super) fn tag_counts(entries: &[Entry]) -> Vec<(String, usize)> {
    let mut counts = BTreeMap::new();
    for entry in entries {
        for tag in &entry.tags {
            *counts.entry(tag.clone()).or_insert(0_usize) += 1;
        }
    }
    let mut tags: Vec<_> = counts.into_iter().collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    tags
}

fn demo_title(index: usize) -> &'static str {
    const TITLES: [&str; 12] = [
        "Alice",
        "Above the city",
        "Alice, Bob, Carol",
        "Luna",
        "A moment of stillness",
        "Luna, Kai, Nova",
        "Alice",
        "Golden hour in the forest",
        "Alice, Bob, Carol",
        "Luna",
        "The city never sleeps",
        "Under the northern lights",
    ];
    if matches!(index % 12, 1 | 4 | 7 | 10 | 11) {
        TITLES[index % TITLES.len()]
    } else {
        demo_characters(index)
    }
}

fn demo_characters(index: usize) -> &'static str {
    const GROUPS: [&str; 6] = [
        "Alice",
        "Alice, Bob",
        "Alice, Bob, Carol",
        "Luna",
        "Luna, Kai",
        "Luna, Kai, Nova",
    ];
    GROUPS[(index + index / 12) % GROUPS.len()]
}

// Same fixture clock as src/data/mockMedia.ts: 2026-10-07 19:45, 96 minutes apart.
fn demo_date(index: usize) -> String {
    let elapsed = index * 96;
    let start = 19 * 60 + 45;
    let days = elapsed.saturating_sub(start).div_ceil(1440);
    let minutes = (start + days * 1440).saturating_sub(elapsed);
    let (mut year, mut month, mut day) = (2026, 10_usize, 7_usize);

    for _ in 0..days {
        if day > 1 {
            day -= 1;
        } else {
            if month == 1 {
                year -= 1;
                month = 12;
            } else {
                month -= 1;
            }
            day = match month {
                4 | 6 | 9 | 11 => 30,
                2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
                2 => 28,
                _ => 31,
            };
        }
    }

    format!(
        "{day:02}.{month:02}.{year} {:02}:{:02}",
        minutes / 60,
        minutes % 60
    )
}

fn demo_tags(index: usize) -> &'static [&'static str] {
    const TAGS: [&[&str]; 12] = [
        &["Nature", "Mountains"],
        &["Drone", "City", "Sunset"],
        &["Nature", "Water"],
        &["Travel", "Sunset"],
        &["Nature", "Sea"],
        &["City", "Night"],
        &["Nature", "Mountains", "Travel"],
        &["Nature", "Forest"],
        &["Nature", "Lake"],
        &["Travel", "Summer"],
        &["City", "Timelapse"],
        &["Nature", "Night"],
    ];
    TAGS[index % TAGS.len()]
}

pub(super) fn matching(entries: &[Entry], query: &str, videos_only: bool) -> Vec<usize> {
    let terms: Vec<_> = query
        .split(';')
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(crate::naming::name_key)
        .collect();
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            let matches = terms.iter().all(|term| {
                let metadata_match = entry
                    .tags
                    .iter()
                    .chain(entry.details.copyrights.iter())
                    .chain(entry.details.artist.iter())
                    .any(|value| crate::naming::name_key(value) == *term)
                    || entry
                        .characters
                        .split(", ")
                        .any(|value| crate::naming::name_key(value) == *term);
                let local_filename_match =
                    !entry.demo && crate::naming::name_key(&entry.details.filename).contains(term);

                metadata_match || local_filename_match
            });

            (!videos_only || entry.video) && matches
        })
        .map(|(index, _)| index)
        .collect()
}

#[cfg(test)]
mod tests;
