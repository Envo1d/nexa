use super::{fit, neighbor, video::time};

#[test]
fn image_fits_both_portrait_and_landscape_without_stretching() {
    assert_eq!(fit(Some((6000, 4000)), 1200.0, 900.0), (1200.0, 800.0));
    assert_eq!(fit(Some((4000, 6000)), 1200.0, 900.0), (600.0, 900.0));
    assert_eq!(fit(Some((100, 100)), 800.0, 600.0), (600.0, 600.0));
}

#[test]
fn zero_sized_viewports_do_not_produce_invalid_image_geometry() {
    assert_eq!(fit(Some((6000, 4000)), 0.0, 0.0), (0.0, 0.0));
    assert_eq!(fit(Some((0, 0)), -1.0, 800.0), (0.0, 800.0));
}

#[test]
fn navigation_follows_filtered_order_without_wrapping_or_opening_hidden_selection() {
    let visible = [8, 2, 9];
    assert_eq!(neighbor(&visible, Some(8), -1), None);
    assert_eq!(neighbor(&visible, Some(8), 1), Some(2));
    assert_eq!(neighbor(&visible, Some(9), 1), None);
    assert_eq!(neighbor(&visible, Some(2), -1), Some(8));
    assert_eq!(neighbor(&visible, Some(1), 1), None);
    assert_eq!(neighbor(&[], None, 1), None);
}

#[test]
fn video_time_preserves_reference_padding_hours_and_flooring() {
    assert_eq!(time(0.0), "00:00");
    assert_eq!(time(2.99), "00:02");
    assert_eq!(time(3661.9), "01:01:01");
    assert_eq!(time(-20.0), "00:00");
}
