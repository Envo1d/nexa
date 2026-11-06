use std::time::Instant;

use gpui::{App, IntoElement, RenderOnce, Window, div, prelude::*, px};

use super::super::{ACCENT, TEXT, motion};
use crate::appearance::rgb;

#[derive(IntoElement)]
pub(in crate::ui) struct Switch {
    pub(in crate::ui) active: bool,
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let target = if self.active { 1.0 } else { 0.0 };
        // Scoped by the enclosing toggle row's stable ID. Mount in the actual
        // initial state so enabled-by-default switches do not animate at startup.
        let transition = window.use_keyed_state("switch-motion", cx, |_, _| {
            motion::Transition::new(target, 150)
        });
        let now = Instant::now();
        transition.update(cx, |transition, _| transition.retarget(target, now));
        let (mut amount, moving) = transition.read(cx).sample(now);
        if !motion::enabled() {
            amount = target;
        } else if moving {
            window.request_animation_frame();
        }

        div()
            .relative()
            .w(px(30.0))
            .h(px(18.0))
            .flex_shrink_0()
            .border_1()
            .border_color(motion::mix_color(rgb(0x514361), rgb(ACCENT), amount))
            .rounded_full()
            .bg(motion::mix_color(rgb(0x1a1827), rgb(0x7b43d9), amount))
            .child(
                div()
                    .absolute()
                    .left(px(2.0 + 12.0 * amount))
                    .top(px(2.0))
                    .size(px(12.0))
                    .rounded_full()
                    .bg(motion::mix_color(rgb(TEXT), rgb(0xffffff), amount)),
            )
    }
}
