use super::{CollectionLayout, GAP};

#[test]
fn sidebar_breakpoints_preserve_collection_space_and_detail_duration_keeps_hours() {
    for (viewport, expected) in [
        (960.0, 196.0),
        (1100.0, 196.0),
        (1101.0, 220.0),
        (1449.0, 220.0),
        (1450.0, 244.0),
        (1680.0, 244.0),
    ] {
        assert_eq!(super::filter_width(viewport), expected);
    }

    assert_eq!(super::full_duration(Some("01:15")), "00:01:15");
    assert_eq!(super::full_duration(Some("01:02:03")), "01:02:03");
    assert_eq!(super::full_duration(None), "Недоступно");
}

#[test]
fn rows_fit_actual_width_at_narrow_desktop_and_expanded_navigation_sizes() {
    for width in [360.0, 540.0, 626.0, 1036.0, 1122.0, 1346.0, 1920.0] {
        for list in [false, true] {
            let layout = CollectionLayout::new(width, list);
            let occupied =
                layout.card_width * layout.columns as f32 + GAP * (layout.columns - 1) as f32;
            assert!(occupied + 3.9 <= width + 0.01);
            assert!(layout.preview_height >= 0.0);
            if list {
                assert_eq!(layout.columns, 1);
                assert_eq!(layout.row_height, 88.0);
                assert!((layout.card_width - (width - 3.9)).abs() < 0.01);
            }
        }
    }
    assert_eq!(CollectionLayout::new(1122.0, false).columns, 4);
    assert_eq!(CollectionLayout::new(1346.0, false).columns, 5);
}

#[test]
fn grid_uses_compact_localized_dates_and_preserves_unavailable_values() {
    assert_eq!(
        super::compact_date(Some("07.10.2026 19:45")),
        "07 окт., 19:45"
    );
    assert_eq!(
        super::compact_date(Some("01.01.2026 00:00")),
        "01 янв., 00:00"
    );
    assert_eq!(super::compact_date(None), "Дата недоступна");
    assert_eq!(super::compact_date(Some("unknown")), "unknown");
}
