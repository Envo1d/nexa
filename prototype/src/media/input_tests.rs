use super::local_position;

#[test]
fn maps_scaled_window_positions_and_excludes_other_controls() {
    let bounds = [300, 150, 900, 600];
    assert_eq!(local_position(bounds, [200.0, 100.0], 1.5), Some([0, 0]));
    assert_eq!(
        local_position(bounds, [400.0, 300.0], 1.5),
        Some([300, 300])
    );
    assert_eq!(local_position(bounds, [800.0, 300.0], 1.5), None);
    assert_eq!(local_position(bounds, [400.0, 500.0], 1.5), None);
    assert_eq!(local_position(bounds, [100.0, 300.0], 1.5), None);
}

#[test]
fn bounds_high_rate_motion_but_preserves_button_coordinates() {
    let mut gate = super::MotionGate::default();
    let start = std::time::Instant::now();
    let mut admitted = 0;
    for tick in 0..1000 {
        admitted += usize::from(gate.admit(
            start + std::time::Duration::from_millis(tick),
            [tick as i32, 0],
            false,
        ));
    }
    assert!(admitted <= 31);
    assert!(gate.admit(
        start + std::time::Duration::from_millis(1000),
        [1000, 0],
        true
    ));
    assert!(!gate.admit(
        start + std::time::Duration::from_millis(1100),
        [1000, 0],
        false
    ));
}
