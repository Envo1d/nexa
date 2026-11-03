use super::*;

#[test]
fn css_ease_matches_reference_curve_and_endpoints() {
    assert!(ease(0.0) < 0.001);
    assert!((ease(1.0) - 1.0).abs() < 0.001);
    assert!((ease(0.5) - 0.8024).abs() < 0.001);
}

#[test]
fn interrupted_motion_reverses_from_the_displayed_value_and_settles() {
    let now = Instant::now();
    let mut motion = Transition::new(0.0, 220);
    motion.retarget(1.0, now);
    let middle = now + Duration::from_millis(70);
    let before = motion.sample(middle).0;
    assert!(before > 0.0 && before < 1.0);
    motion.retarget(0.0, middle);
    assert_eq!(motion.sample(middle).0, before);
    assert!(motion.sample(middle + Duration::from_millis(50)).0 < before);
    assert_eq!(
        motion.sample(middle + Duration::from_millis(220)),
        (0.0, false)
    );
}

#[test]
fn transparent_color_interpolation_preserves_the_visible_hue() {
    let mixed = mix_color(rgba(0), rgba(0x8853e1ff), 0.5);
    assert_eq!(mixed.a, 0.5);
    assert!((mixed.r - gpui::rgb(0x8853e1).r).abs() < 0.001);
}
