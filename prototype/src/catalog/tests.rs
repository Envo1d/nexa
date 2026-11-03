use super::*;

#[test]
fn incoming_search_matches_filename_and_relative_folder_before_kind_counts() {
    let entries = incoming_demo_entries();
    let (all, counts) = browse_with_counts(
        &entries,
        " bAtCh 1 ",
        BrowseFilter {
            path_search: true,
            ..Default::default()
        },
    );
    assert_eq!(entries.len(), 48);
    assert_eq!(all.len(), 8);
    assert_eq!(counts[0], counts[1] + counts[2]);
    let (videos, video_counts) = browse_with_counts(
        &entries,
        "batch 1",
        BrowseFilter {
            path_search: true,
            videos_only: true,
            ..Default::default()
        },
    );
    assert_eq!(counts, video_counts);
    assert_eq!(videos.len(), counts[1]);
    assert!(videos.iter().all(|&index| entries[index].video));
    assert_eq!(
        browse(
            &entries,
            "INCOMING-001",
            BrowseFilter {
                path_search: true,
                ..Default::default()
            }
        ),
        vec![0]
    );
    assert!(
        browse(
            &entries,
            "Nature",
            BrowseFilter {
                path_search: true,
                ..Default::default()
            }
        )
        .is_empty()
    );
    assert!(entries.iter().all(|entry| entry.tags.is_empty()
        && entry.characters.is_empty()
        && entry.details.artist.is_none()
        && entry.details.copyrights.is_empty()));
}

fn browse(entries: &[Entry], query: &str, filter: BrowseFilter) -> Vec<usize> {
    browse_with_counts(entries, query, filter).0
}

#[test]
fn demo_has_ten_thousand_records_and_twelve_existing_previews() {
    let entries = demo_entries();
    assert_eq!(entries.len(), DEMO_COUNT);
    let paths: std::collections::HashSet<_> = entries.iter().map(|entry| &entry.path).collect();
    assert_eq!(paths.len(), 12);
    assert!(paths.into_iter().all(|path| path.is_file()));
}

#[test]
fn search_handles_cyrillic_case_and_video_filter() {
    let entries = vec![
        entry(PathBuf::from("Тест.MP4")).unwrap(),
        entry(PathBuf::from("Тест.png")).unwrap(),
    ];
    assert_eq!(matching(&entries, " ТЕСТ ", false), vec![0, 1]);
    assert_eq!(matching(&entries, "тест", true), vec![0]);
    assert!(matching(&entries, "missing", false).is_empty());
}

#[test]
fn rejects_unsupported_and_extensionless_files() {
    assert!(entry(PathBuf::from("file.exe")).is_none());
    assert!(entry(PathBuf::from("file")).is_none());
}

#[test]
fn tag_search_and_kind_filter_use_the_same_demo_records() {
    let entries = demo_entries();
    let nature = matching(&entries, " NATURE ", false);
    assert!(!nature.is_empty());
    assert!(
        nature
            .iter()
            .all(|&index| entries[index].tags.iter().any(|tag| tag == "Nature"))
    );

    let nature_videos = matching(&entries, "nature", true);
    assert!(!nature_videos.is_empty());
    assert!(
        nature_videos
            .iter()
            .all(|index| nature.contains(index) && entries[*index].video)
    );
    assert_eq!(entries[1].duration.as_deref(), Some("01:15"));
    assert_eq!(entries[8].extension, "PNG");
}

#[test]
fn demo_clock_rolls_over_midnight_month_and_year() {
    assert_eq!(demo_date(0), "07.10.2026 19:45");
    assert_eq!(demo_date(13), "06.10.2026 22:57");
    assert_eq!(demo_date(120), "29.09.2026 19:45");
    assert_eq!(demo_date(4200), "31.12.2025 19:45");
}

#[test]
fn browsing_combines_filters_sorts_dates_and_reflects_favorite_removal() {
    let mut entries = vec![
        entry("old.png".into()).unwrap(),
        entry("new.mp4".into()).unwrap(),
        entry("bare.png".into()).unwrap(),
    ];
    for (index, entry) in entries.iter_mut().enumerate() {
        entry.favorite = true;
        entry.modified = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(index as u64));
    }
    entries[0].tags.push("Nature".into());
    entries[1].tags.push("Nature".into());
    assert_eq!(browse(&entries, "", BrowseFilter::default()), vec![2, 1, 0]);
    assert_eq!(
        browse(
            &entries,
            "",
            BrowseFilter {
                tagged_only: true,
                sort: SortMode::Oldest,
                ..Default::default()
            }
        ),
        vec![0, 1]
    );
    let favorites = || BrowseFilter {
        favorites_only: true,
        images_only: true,
        tagged_only: true,
        ..Default::default()
    };
    assert_eq!(browse(&entries, "nature", favorites()), vec![0]);
    entries[0].favorite = false;
    assert!(browse(&entries, "nature", favorites()).is_empty());
    assert_eq!(
        browse(
            &entries,
            "nature",
            BrowseFilter {
                videos_only: true,
                ..Default::default()
            }
        ),
        vec![1]
    );
}

#[test]
fn type_counts_remain_search_scoped_when_the_selected_kind_changes() {
    let mut entries = vec![
        entry("image.png".into()).unwrap(),
        entry("video.mp4".into()).unwrap(),
        entry("other.mp4".into()).unwrap(),
    ];
    entries[0].tags.push("Nature".into());
    entries[1].tags.push("Nature".into());
    entries[2].tags.push("City".into());
    entries[0].favorite = true;
    entries[1].favorite = true;
    let (videos, counts) = browse_with_counts(
        &entries,
        "nature",
        BrowseFilter {
            favorites_only: true,
            videos_only: true,
            ..Default::default()
        },
    );
    assert_eq!(videos, vec![1]);
    assert_eq!(counts, [2, 1, 1]);
    let (images, image_counts) = browse_with_counts(
        &entries,
        "nature",
        BrowseFilter {
            favorites_only: true,
            images_only: true,
            ..Default::default()
        },
    );
    assert_eq!(images, vec![0]);
    assert_eq!(image_counts, counts);
    assert_eq!(
        browse_with_counts(
            &entries,
            "city",
            BrowseFilter {
                favorites_only: true,
                ..Default::default()
            }
        )
        .1,
        [0, 0, 0]
    );
}

#[test]
fn metadata_terms_are_exact_combined_and_canonically_normalized() {
    let entries = demo_entries();
    assert!(matching(&entries, "Nat", false).is_empty());
    let matched = matching(&entries, " nature ; MOUNTAINS ; ", false);
    assert!(!matched.is_empty());
    assert!(matched.iter().all(
        |&index| entries[index].tags.iter().any(|tag| tag == "Nature")
            && entries[index].tags.iter().any(|tag| tag == "Mountains")
    ));
    assert!(!matching(&entries, "Mira 001; Alice; Wonderland", false).is_empty());
    let mut sample = entries[0].clone();
    sample.tags = vec!["Café".into()];
    assert_eq!(matching(&[sample], "CAFE\u{301}", false), vec![0]);
}

#[test]
fn name_sort_uses_full_filenames_and_shared_natural_ordering() {
    let entries = vec![
        entry("Photo 10.png".into()).unwrap(),
        entry("photo 2.png".into()).unwrap(),
        entry("photo 1.png".into()).unwrap(),
    ];
    assert_eq!(
        browse(
            &entries,
            "",
            BrowseFilter {
                sort: SortMode::NameAscending,
                ..Default::default()
            }
        ),
        vec![2, 1, 0]
    );
    assert_eq!(
        browse(
            &entries,
            "",
            BrowseFilter {
                sort: SortMode::NameDescending,
                ..Default::default()
            }
        ),
        vec![0, 1, 2]
    );
}

#[test]
fn demo_details_follow_the_reference_annotations_and_technical_fixture() {
    let entries = demo_entries();
    let first = &entries[0];
    assert_eq!(first.details.filename, "Alice [Mira 001].jpg");
    assert_eq!(first.details.artist.as_deref(), Some("Mira 001"));
    assert_eq!(first.details.size, Some(3 * 1024 * 1024));
    assert_eq!(first.details.resolution, Some((6000, 4000)));
    assert_eq!(
        first.details.created_label.as_deref(),
        Some("06.10.2026 19:45")
    );
    assert_eq!(
        entries[1].details.filename,
        "Alice х Bob - Above the city [Mira 002].mp4"
    );
    assert_eq!(entries[1].details.resolution, Some((3840, 2160)));
    assert_eq!(
        entries[2].details.copyrights,
        vec!["Wonderland", "Lunar Tales"]
    );
}
