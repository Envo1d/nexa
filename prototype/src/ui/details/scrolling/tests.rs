use super::{SCROLL_DURATION, ScrollMotion};
use std::time::{Duration, Instant};

#[test]
fn wheel_motion_has_intermediate_frames_and_finishes_at_target() {
    let now = Instant::now();
    let motion = ScrollMotion::new(0.0, -100.0, now, Some(0));
    let (first, _) = motion.sample(now + Duration::from_millis(16), 500.0);
    let (middle, finished) = motion.sample(now + SCROLL_DURATION / 2, 500.0);

    assert!(first < 0.0 && first > middle && middle > -100.0);
    assert!(!finished);
    assert_eq!(motion.sample(now + SCROLL_DURATION, 500.0), (-100.0, true));
}

#[test]
fn repeated_wheel_steps_accumulate_but_reversal_starts_at_visible_position() {
    let now = Instant::now();
    let mut motion = ScrollMotion::new(0.0, 0.0, now, Some(0));
    motion.retarget(0.0, -60.0, 500.0, now);
    motion.retarget(-20.0, -60.0, 500.0, now);
    assert_eq!(motion.target, -120.0);

    motion.retarget(-40.0, 30.0, 500.0, now);
    assert_eq!(motion.target, -10.0);
    assert_eq!(motion.sample(now, 500.0).0, -40.0);
}

#[test]
fn targets_and_frames_stay_bounded_after_content_shrinks() {
    let now = Instant::now();
    let mut motion = ScrollMotion::new(-200.0, -300.0, now, Some(0));
    motion.retarget(-200.0, -1000.0, 350.0, now);
    assert_eq!(motion.target, -350.0);
    assert_eq!(motion.sample(now + SCROLL_DURATION, 50.0), (-50.0, true));

    motion.retarget(-30.0, 1000.0, 50.0, now);
    assert_eq!(motion.target, 0.0);
    assert_eq!(motion.sample(now + SCROLL_DURATION, 0.0), (0.0, true));
}
