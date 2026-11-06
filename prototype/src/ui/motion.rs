use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Background, Div, ElementId, Hsla, Interactivity, IntoElement, ParentElement,
    RenderOnce, Rgba, Stateful, StyleRefinement, Styled, Window, div, prelude::*,
};

use crate::appearance::rgba;

pub(super) fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        use windows::Win32::UI::WindowsAndMessaging::{
            SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
        };

        let mut enabled = 1_i32;
        // SAFETY: the OS writes one BOOL into this live, correctly sized local.
        let result = unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                Some((&mut enabled as *mut i32).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };

        result.is_err() || enabled != 0
    })
}

/// CSS ease: cubic-bezier(.25, .1, .25, 1), including inversion of the x axis.
pub(super) fn ease(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }

    let curve = |t: f32, a: f32, b: f32| {
        3.0 * (1.0 - t).powi(2) * t * a + 3.0 * (1.0 - t) * t * t * b + t.powi(3)
    };
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..16 {
        let middle = (low + high) * 0.5;
        if curve(middle, 0.25, 0.25) < progress {
            low = middle;
        } else {
            high = middle;
        }
    }

    curve((low + high) * 0.5, 0.1, 1.0)
}

pub(super) struct Transition {
    from: f32,
    target: f32,
    started: Instant,
    duration: Duration,
}

impl Transition {
    pub(super) fn new(value: f32, milliseconds: u64) -> Self {
        Self {
            from: value,
            target: value,
            started: Instant::now(),
            duration: Duration::from_millis(milliseconds),
        }
    }

    pub(super) fn sample(&self, now: Instant) -> (f32, bool) {
        let progress = (now.saturating_duration_since(self.started).as_secs_f32()
            / self.duration.as_secs_f32())
        .min(1.0);
        let moving = self.from != self.target && progress < 1.0;
        let value = if moving {
            self.from + (self.target - self.from) * ease(progress)
        } else {
            self.target
        };

        (value, moving)
    }

    pub(super) fn retarget(&mut self, target: f32, now: Instant) {
        if self.target == target {
            return;
        }

        self.from = self.sample(now).0;
        self.target = target;
        self.started = now;
    }
}

#[derive(Clone, Copy)]
pub(super) struct Surface {
    angle: f32,
    colors: [Rgba; 2],
}

impl Surface {
    pub(super) fn gradient(angle: f32, from: Rgba, to: Rgba) -> Self {
        Self {
            angle,
            colors: [from, to],
        }
    }

    fn mix(self, other: Self, amount: f32) -> Background {
        let colors = [
            mix_color(self.colors[0], other.colors[0], amount),
            mix_color(self.colors[1], other.colors[1], amount),
        ];
        gpui::linear_gradient(
            self.angle,
            gpui::linear_color_stop(colors[0], 0.0),
            gpui::linear_color_stop(colors[1], 1.0),
        )
    }
}

impl From<Rgba> for Surface {
    fn from(color: Rgba) -> Self {
        Self::gradient(0.0, color, color)
    }
}

pub(super) fn mix_color(from: Rgba, to: Rgba, amount: f32) -> Rgba {
    let alpha = from.a + (to.a - from.a) * amount;
    let channel = |a: f32, b: f32| {
        if alpha > 0.0 {
            (a * from.a * (1.0 - amount) + b * to.a * amount) / alpha
        } else {
            0.0
        }
    };

    Rgba {
        r: channel(from.r, to.r),
        g: channel(from.g, to.g),
        b: channel(from.b, to.b),
        a: alpha,
    }
}

/// A normal interactive Div with element-local, reversible color transitions.
#[derive(IntoElement)]
pub(super) struct MotionDiv {
    element: Stateful<Div>,
    id: ElementId,
    background: Surface,
    hover_background: Option<Surface>,
    hover_text: Option<Hsla>,
    hover_border: Option<Hsla>,
    keyboard_focus: bool,
    selected: bool,
    focus_ring_background: Option<Rgba>,
    focus_handle: Option<gpui::FocusHandle>,
}

pub(super) fn button(id: impl Into<ElementId>) -> MotionDiv {
    let id = id.into();
    MotionDiv {
        element: div().id(id.clone()),
        id,
        background: rgba(0).into(),
        hover_background: None,
        hover_text: None,
        hover_border: None,
        keyboard_focus: false,
        selected: false,
        focus_ring_background: None,
        focus_handle: None,
    }
}

impl MotionDiv {
    pub(super) fn with_focus(mut self, handle: &gpui::FocusHandle) -> Self {
        self.keyboard_focus = true;
        self.focus_handle = Some(handle.clone());
        self
    }

    pub(super) fn focus_ring(mut self, background: Rgba) -> Self {
        self.focus_ring_background = Some(background);
        self
    }

    pub(super) fn keyboard_focus(mut self, enabled: bool) -> Self {
        self.keyboard_focus = enabled;
        self
    }

    pub(super) fn bg(mut self, surface: impl Into<Surface>) -> Self {
        self.background = surface.into();
        self.element = self.element.bg(self.background.mix(self.background, 0.0));
        self
    }

    pub(super) fn hover_colors(
        mut self,
        background: Option<Surface>,
        text: Option<Hsla>,
        border: Option<Hsla>,
    ) -> Self {
        self.hover_background = background;
        self.hover_text = text;
        self.hover_border = border;
        self
    }
}

impl Styled for MotionDiv {
    fn style(&mut self) -> &mut StyleRefinement {
        self.element.style()
    }
}

impl ParentElement for MotionDiv {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.element.extend(elements);
    }
}

impl InteractiveElement for MotionDiv {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.element.interactivity()
    }
}

impl StatefulInteractiveElement for MotionDiv {}

impl gpui_component::Selectable for MotionDiv {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl gpui_component::menu::DropdownMenu for MotionDiv {}

impl RenderOnce for MotionDiv {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.keyboard_focus {
            let focus = self.focus_handle.clone().unwrap_or_else(|| {
                window
                    .use_keyed_state(self.id.clone(), cx, |_, cx| {
                        cx.focus_handle().tab_stop(true)
                    })
                    .read(cx)
                    .clone()
            });
            let ring_background = self.focus_ring_background;
            self.element = self.element.track_focus(&focus).focus(move |style| {
                let mut shadows = vec![gpui::BoxShadow {
                    color: crate::appearance::rgba(0xa786ff80).into(),
                    offset: gpui::point(gpui::px(0.0), gpui::px(0.0)),
                    blur_radius: gpui::px(0.0),
                    spread_radius: gpui::px(if ring_background.is_some() { 5.0 } else { 2.0 }),
                }];
                if let Some(background) = ring_background {
                    shadows[0].color = crate::appearance::rgb(super::ACCENT).into();
                    shadows.push(gpui::BoxShadow {
                        color: background.into(),
                        offset: gpui::point(gpui::px(0.), gpui::px(0.)),
                        blur_radius: gpui::px(0.),
                        spread_radius: gpui::px(3.),
                    });
                    style.shadow(shadows)
                } else {
                    style
                        .border_color(crate::appearance::rgb(super::ACCENT))
                        .shadow(shadows)
                }
            });
            if ring_background.is_some() && focus.is_focused(window) {
                self.element = self.element.relative().child(
                    div()
                        .absolute()
                        .top(gpui::px(-5.))
                        .right(gpui::px(-5.))
                        .bottom(gpui::px(-5.))
                        .left(gpui::px(-5.))
                        .rounded(gpui::px(11.))
                        .border_2()
                        .border_color(crate::appearance::rgb(super::ACCENT)),
                );
            }
        }

        let motion = window.use_keyed_state(self.id.clone(), cx, |_, _| Transition::new(0.0, 140));
        let (mut amount, mut moving) = motion.read(cx).sample(Instant::now());
        if !enabled() {
            amount = motion.read(cx).target;
            moving = false;
        }
        if moving {
            window.request_animation_frame();
        }

        if let Some(target) = self.hover_background {
            self.element = self.element.bg(self.background.mix(target, amount));
        }
        if let Some(target) = self.hover_text {
            let from = self
                .element
                .style()
                .text
                .as_ref()
                .and_then(|text| text.color)
                .unwrap_or(window.text_style().color);
            self.element = self
                .element
                .text_color(mix_color(from.into(), target.into(), amount));
        }
        if let Some(target) = self.hover_border {
            let from = self
                .element
                .style()
                .border_color
                .unwrap_or(gpui::transparent_black());
            self.element = self
                .element
                .border_color(mix_color(from.into(), target.into(), amount));
        }

        self.element.on_hover(move |hovered, _, cx| {
            motion.update(cx, |motion, cx| {
                motion.retarget(if *hovered { 1.0 } else { 0.0 }, Instant::now());
                cx.notify();
            });
        })
    }
}

#[cfg(test)]
mod tests;
