use gpui::{
    App, BoxShadow, Context, Focusable, IntoElement, RenderOnce, WeakEntity, Window, div, point,
    prelude::*, px, rgb, rgba,
};
use gpui_component::{Selectable, popover::Popover};

use super::super::{ACCENT, Gallery, LINE, MUTED, TEXT, motion, shell::icon};
use crate::taxonomy::Category;

impl Gallery {
    pub(super) fn copyright_select(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let gallery = cx.weak_entity();
        let open_gallery = gallery.clone();
        let values: Vec<_> = self
            .taxonomy
            .catalog
            .records
            .iter()
            .filter(|record| record.category == Category::Copyright)
            .map(|record| record.name.clone())
            .collect();
        let selected = self.taxonomy.copyright.clone();

        Popover::new("dictionary-copyright")
            .appearance(false)
            .open(self.taxonomy.select_open)
            .on_open_change(move |open, window, cx| {
                let _ = open_gallery.update(cx, |this, cx| {
                    this.taxonomy.select_open = *open;
                    this.taxonomy.select_active = this
                        .taxonomy
                        .catalog
                        .records
                        .iter()
                        .filter(|record| record.category == Category::Copyright)
                        .position(|record| record.name == this.taxonomy.copyright)
                        .unwrap_or(0);
                    if !open {
                        this.taxonomy.select_focus.focus(window);
                    }
                    cx.notify();
                });
            })
            .trigger(CopyrightTrigger {
                gallery: gallery.clone(),
                label: selected.clone(),
                open: self.taxonomy.select_open,
                focus: self.taxonomy.select_focus.clone(),
            })
            .pt(px(2.0))
            .content(move |state, window, cx| {
                state.focus_handle(cx).focus(window);
                let active = gallery
                    .upgrade()
                    .map(|view| view.read(cx).taxonomy.select_active)
                    .unwrap_or(0);
                let keys = gallery.clone();
                let key_values = values.clone();

                div()
                    .id("copyright-options")
                    .track_focus(&state.focus_handle(cx))
                    .flex()
                    .flex_col()
                    .w(px(280.0))
                    .max_h(px(240.0))
                    .overflow_y_scroll()
                    .p(px(5.0))
                    .rounded(px(9.0))
                    .border_1()
                    .border_color(rgb(0x6d5aa4))
                    .bg(rgb(0x1a1827))
                    .text_size(px(11.0))
                    .line_height(px(13.75))
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
                            if key_values.is_empty()
                                || !matches!(
                                    key,
                                    "up" | "down" | "home" | "end" | "enter" | "space"
                                )
                            {
                                return;
                            }

                            let choose = matches!(key, "enter" | "space");
                            let _ = keys.update(cx, |this, cx| {
                                let count = key_values.len();
                                this.taxonomy.select_active = match key {
                                    "up" => (this.taxonomy.select_active + count - 1) % count,
                                    "down" => (this.taxonomy.select_active + 1) % count,
                                    "home" => 0,
                                    "end" => count - 1,
                                    _ => this.taxonomy.select_active.min(count - 1),
                                };
                                if choose {
                                    this.taxonomy.copyright =
                                        key_values[this.taxonomy.select_active].clone();
                                }
                                cx.notify();
                            });
                            if choose {
                                state.dismiss(window, cx);
                            }
                            cx.stop_propagation();
                        },
                    ))
                    .children(values.iter().enumerate().map(|(index, value)| {
                        let click = gallery.clone();
                        let hover = gallery.clone();
                        let value = value.clone();

                        div()
                            .id(("copyright-option", index))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(6.0))
                            .px(px(7.0))
                            .py(px(9.0))
                            .rounded(px(5.0))
                            .text_color(rgb(if value == selected { ACCENT } else { TEXT }))
                            .when(index == active, |view| view.bg(rgb(0x2c2644)))
                            .cursor_pointer()
                            .child(value.clone())
                            .when(value == selected, |view| {
                                view.child(
                                    gpui_component::Icon::new(gpui_component::IconName::Check)
                                        .size(px(14.0)),
                                )
                            })
                            .on_hover(move |hovered, _, cx| {
                                if *hovered {
                                    let _ = hover.update(cx, |this, cx| {
                                        this.taxonomy.select_active = index;
                                        cx.notify();
                                    });
                                }
                            })
                            .on_click(cx.listener(move |state, _, window, cx| {
                                let _ = click.update(cx, |this, cx| {
                                    this.taxonomy.copyright = value.clone();
                                    cx.notify();
                                });
                                state.dismiss(window, cx);
                            }))
                    }))
                    .into_any_element()
            })
    }
}

#[derive(IntoElement)]
struct CopyrightTrigger {
    gallery: WeakEntity<Gallery>,
    label: String,
    open: bool,
    focus: gpui::FocusHandle,
}

impl Selectable for CopyrightTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.open = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.open
    }
}

impl RenderOnce for CopyrightTrigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        motion::button("copyright-trigger")
            .track_focus(&self.focus)
            .tab_index(0)
            .flex()
            .items_center()
            .justify_between()
            .w_full()
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
            .focus(|style| style.border_color(rgb(ACCENT)))
            .hover_colors(
                Some(rgb(0x242034).into()),
                None,
                Some(rgb(if self.open { ACCENT } else { 0x6d5aa4 }).into()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(self.label),
            )
            .child(
                icon("down")
                    .size(px(15.0))
                    .text_color(rgb(MUTED))
                    .rotate(gpui::Radians(if self.open {
                        std::f32::consts::PI
                    } else {
                        0.0
                    })),
            )
            .on_key_down(move |event, _, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "up" | "down"
                ) {
                    let _ = self.gallery.update(cx, |this, cx| {
                        this.taxonomy.select_open = true;
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
            })
    }
}
