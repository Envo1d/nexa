use gpui::{
    Animation, AnimationExt, App, BoxShadow, Context, Focusable, IntoElement, RenderOnce,
    WeakEntity, Window, div, point, prelude::*, px,
};
use gpui_component::{Selectable, popover::Popover};

use crate::appearance::{rgb, rgba};
use crate::catalog::SortMode;

use super::{ACCENT, Gallery, LINE, MUTED, TEXT, shell::icon};

const MODES: [SortMode; 4] = [
    SortMode::Newest,
    SortMode::Oldest,
    SortMode::NameAscending,
    SortMode::NameDescending,
];

impl Gallery {
    pub(super) fn sort_select(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let gallery = cx.weak_entity();
        let open_gallery = gallery.clone();

        Popover::new("collection-sort")
            .appearance(false)
            .open(self.sort_open)
            .on_open_change(move |open, window, cx| {
                let _ = open_gallery.update(cx, |this, cx| {
                    this.sort_open = *open;
                    this.sort_active = MODES
                        .iter()
                        .position(|mode| *mode == this.sort)
                        .unwrap_or(0);

                    if !open {
                        this.sort_focus.focus(window);
                    }

                    cx.notify();
                });
            })
            .trigger(SortTrigger {
                gallery: gallery.clone(),
                sort: self.sort,
                open: self.sort_open,
                focus: self.sort_focus.clone(),
                rotation: if super::motion::enabled() {
                    self.sort_transition.sample(std::time::Instant::now()).0
                } else if self.sort_open {
                    1.0
                } else {
                    0.0
                },
            })
            // Popover adds a 4px anchor offset; the reference gap is 6px.
            .pt(px(2.0))
            .content(move |state, window, cx| {
                state.focus_handle(cx).focus(window);

                let Some(entity) = gallery.upgrade() else {
                    return div().id("sort-unavailable").into_any_element();
                };
                let view = entity.read(cx);
                let active = view.sort_active;
                let selected = view.sort;
                let key_gallery = gallery.clone();

                div()
                    .id("sort-options")
                    .track_focus(&state.focus_handle(cx))
                    .flex()
                    .flex_col()
                    .w(px(150.0))
                    .p(px(5.0))
                    .rounded(px(9.0))
                    .border_1()
                    .border_color(rgb(0x6d5aa4))
                    .bg(rgb(0x1a1827))
                    .shadow(vec![BoxShadow {
                        color: rgba(0x00000080).into(),
                        offset: point(px(0.0), px(10.0)),
                        blur_radius: px(28.0),
                        spread_radius: px(0.0),
                    }])
                    .on_key_down(cx.listener(
                        move |state, event: &gpui::KeyDownEvent, window, cx| {
                            let key = event.keystroke.key.as_str();
                            if key == "tab" {
                                state.dismiss(window, cx);
                                return;
                            }

                            if !matches!(key, "up" | "down" | "home" | "end" | "enter" | "space") {
                                return;
                            }

                            let choose = matches!(key, "enter" | "space");
                            let _ = key_gallery.update(cx, |this, cx| {
                                this.sort_active = match key {
                                    "up" => (this.sort_active + MODES.len() - 1) % MODES.len(),
                                    "down" => (this.sort_active + 1) % MODES.len(),
                                    "home" => 0,
                                    "end" => MODES.len() - 1,
                                    _ => this.sort_active,
                                };

                                if choose {
                                    this.sort = MODES[this.sort_active];
                                    this.refilter(cx);
                                }

                                cx.notify();
                            });

                            if choose {
                                state.dismiss(window, cx);
                            }

                            cx.stop_propagation();
                        },
                    ))
                    .children(MODES.into_iter().enumerate().map(|(index, mode)| {
                        let hover_gallery = gallery.clone();
                        let click_gallery = gallery.clone();

                        div()
                            .id(("sort-option", index))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(6.0))
                            .px(px(7.0))
                            .py(px(9.0))
                            .rounded(px(5.0))
                            .text_size(px(11.0))
                            .line_height(px(15.0))
                            .text_color(rgb(if mode == selected { ACCENT } else { TEXT }))
                            .when(index == active, |view| view.bg(rgb(0x2c2644)))
                            .cursor_pointer()
                            .child(mode.label())
                            .when(mode == selected, |view| {
                                view.child(
                                    gpui_component::Icon::new(gpui_component::IconName::Check)
                                        .size(px(14.0)),
                                )
                            })
                            .on_hover(move |hovered, _, cx| {
                                if !hovered {
                                    return;
                                }

                                let _ = hover_gallery.update(cx, |this, cx| {
                                    if this.sort_active != index {
                                        this.sort_active = index;
                                        cx.notify();
                                    }
                                });
                            })
                            .on_click(cx.listener(move |state, _, window, cx| {
                                let _ = click_gallery.update(cx, |this, cx| {
                                    this.sort = mode;
                                    this.refilter(cx);
                                });
                                state.dismiss(window, cx);
                            }))
                    }))
                    .with_animation(
                        "sort-entrance",
                        Animation::new(std::time::Duration::from_millis(
                            if super::motion::enabled() { 150 } else { 1 },
                        ))
                        .with_easing(super::motion::ease),
                        |view, progress| view.opacity(progress).top(px(-4.0 * (1.0 - progress))),
                    )
                    .into_any_element()
            })
    }
}

#[derive(IntoElement)]
struct SortTrigger {
    gallery: WeakEntity<Gallery>,
    sort: SortMode,
    open: bool,
    focus: gpui::FocusHandle,
    rotation: f32,
}

impl Selectable for SortTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.open = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.open
    }
}

impl RenderOnce for SortTrigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        super::motion::button("sort-trigger")
            .track_focus(&self.focus)
            .tab_index(0)
            .flex()
            .items_center()
            .justify_between()
            .w(px(150.0))
            .h(px(36.0))
            .px(px(10.0))
            .gap(px(10.0))
            .border_1()
            .border_color(rgb(if self.open { ACCENT } else { LINE }))
            .rounded(px(7.0))
            .bg(rgb(0x1a1927))
            .text_color(rgb(TEXT))
            .text_size(px(11.0))
            .line_height(px(13.75))
            .cursor_pointer()
            .hover_colors(
                Some(rgb(0x242034).into()),
                None,
                Some(rgb(if self.open { ACCENT } else { 0x6d5aa4 }).into()),
            )
            .child(self.sort.label())
            .child(
                icon("down")
                    .size(px(15.0))
                    .text_color(rgb(MUTED))
                    .rotate(gpui::Radians(std::f32::consts::PI * self.rotation)),
            )
            .on_key_down(move |event, _, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "up" | "down"
                ) {
                    let _ = self.gallery.update(cx, |this, cx| {
                        this.sort_open = true;
                        this.sort_active = MODES
                            .iter()
                            .position(|mode| *mode == this.sort)
                            .unwrap_or(0);
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
            })
    }
}
