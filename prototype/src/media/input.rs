//! Map GPUI window coordinates to physical mpv surface pixels.
pub(super) fn local_position(bounds: [i32; 4], position: [f32; 2], scale: f32) -> Option<[i32; 2]> {
    let x = (position[0] * scale).round() as i32 - bounds[0];
    let y = (position[1] * scale).round() as i32 - bounds[1];
    (x >= 0 && y >= 0 && x < bounds[2] && y < bounds[3]).then_some([x, y])
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct MotionGate {
    last: Option<(std::time::Instant, [i32; 2])>,
}

impl MotionGate {
    pub(super) fn admit(&mut self, now: std::time::Instant, point: [i32; 2], force: bool) -> bool {
        let limited = self.last.is_some_and(|(last, previous)| {
            previous == point || now.duration_since(last) < std::time::Duration::from_millis(33)
        });
        if !force && limited {
            return false;
        }

        self.last = Some((now, point));
        true
    }
}
