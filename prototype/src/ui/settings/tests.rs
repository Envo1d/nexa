use super::section_at_offset;

#[test]
fn section_tracking_follows_the_reading_position_and_last_section_at_scroll_end() {
    let positions = [
        30.0, 220.0, 410.0, 1000.0, 1400.0, 1700.0, 2150.0, 2350.0, 2700.0,
    ];

    assert_eq!(section_at_offset(0.0, 2000.0, &positions), 0);
    assert_eq!(section_at_offset(139.0, 2000.0, &positions), 0);
    assert_eq!(section_at_offset(140.0, 2000.0, &positions), 1);
    assert_eq!(section_at_offset(970.0, 2000.0, &positions), 3);
    assert_eq!(section_at_offset(1997.0, 2000.0, &positions), 5);
    assert_eq!(section_at_offset(1998.0, 2000.0, &positions), 8);
}

#[test]
fn unmeasured_sections_do_not_select_the_last_navigation_item() {
    assert_eq!(section_at_offset(0.0, 0.0, &[f32::INFINITY; 9]), 0);
}
