use std::time::{Duration, Instant};

use gpui::{Context, ScrollWheelEvent, Window, point, px};

use super::super::Gallery;

const SCROLL_DURATION: Duration = Duration::from_millis(220);

pub(in super::super) struct ScrollMotion {
    from: f32,
    target: f32,
    written: f32,
    started: Instant,
    selected: Option<usize>,
}

impl ScrollMotion {
    fn new(current: f32, target: f32, now: Instant, selected: Option<usize>) -> Self {
        Self {
            from: current,
            target,
            written: current,
            started: now,
            selected,
        }
    }

    fn retarget(&mut self, current: f32, delta: f32, max: f32, now: Instant) {
        let remaining = self.target - current;
        let same_direction = remaining.signum() == delta.signum();
        let base = if same_direction { self.target } else { current };

        self.from = current;
        self.target = (base + delta).clamp(-max, 0.0);
        self.written = current;
        self.started = now;
    }

    fn sample(&self, now: Instant, max: f32) -> (f32, bool) {
        let progress = (now.duration_since(self.started).as_secs_f32()
            / SCROLL_DURATION.as_secs_f32())
        .min(1.0);
        let eased = 1.0 - (1.0 - progress).powi(3);
        let target = self.target.clamp(-max, 0.0);
        let value = (self.from + (target - self.from) * eased).clamp(-max, 0.0);
        let finished = progress >= 1.0 || (value - target).abs() < 0.25;

        (if finished { target } else { value }, finished)
    }
}

impl Gallery {
    pub(super) fn scroll_details(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = f32::from(event.delta.pixel_delta(window.line_height()).y);
        if delta == 0.0 {
            return;
        }

        // Consume before the parent Div's immediate scroll listener runs.
        cx.stop_propagation();

        let current = f32::from(self.details_scroll.offset().y);
        let max = f32::from(self.details_scroll.max_offset().height).max(0.0);
        if event.delta.precise() {
            self.details_motion = None;
            self.details_scroll
                .set_offset(point(px(0.0), px((current + delta).clamp(-max, 0.0))));
            cx.notify();
            return;
        }

        let now = Instant::now();
        let motion = self
            .details_motion
            .get_or_insert_with(|| ScrollMotion::new(current, current, now, self.selected));
        if motion.selected != self.selected || (motion.written - current).abs() > 0.5 {
            *motion = ScrollMotion::new(current, current, now, self.selected);
        }

        motion.retarget(current, delta, max, now);
        self.schedule_details_frame(window, cx);
    }

    fn schedule_details_frame(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.details_frame_pending {
            return;
        }

        self.details_frame_pending = true;
        let entity = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = entity.update(cx, |this, cx| {
                this.details_frame_pending = false;
                let Some(motion) = this.details_motion.as_ref() else {
                    return;
                };
                let current = f32::from(this.details_scroll.offset().y);
                let interrupted = !this.details_open
                    || this.viewer
                    || this.selected != motion.selected
                    || (current - motion.written).abs() > 0.5;
                if interrupted {
                    this.details_motion = None;
                    return;
                }

                let max = f32::from(this.details_scroll.max_offset().height).max(0.0);
                let (value, finished) = motion.sample(Instant::now(), max);
                this.details_scroll.set_offset(point(px(0.0), px(value)));
                if finished {
                    this.details_motion = None;
                } else if let Some(motion) = this.details_motion.as_mut() {
                    motion.written = value;
                    this.schedule_details_frame(window, cx);
                }

                cx.notify();
            });
        });
        // Event/frame callbacks run outside GPUI's paint scope. Notify the
        // owning entity instead of using the paint-only animation-frame API.
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
