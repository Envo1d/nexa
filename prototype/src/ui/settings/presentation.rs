use gpui::{Context, Div, Focusable, FontWeight, Window, div, point, prelude::*, px};
use gpui_component::input::Input;

use super::{
    ACCENT, DEFAULT_FIELDS, Gallery, LINE, MUTED, SECTIONS, SURFACE, TEXT, icon, motion, text,
};
use crate::appearance::rgb;

pub(super) fn small(value: impl Into<String>) -> Div {
    div()
        .text_size(px(12.0))
        .line_height(px(19.2))
        .text_color(rgb(MUTED))
        .child(value.into())
}

pub(super) fn label(title: &str, description: &str) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(17.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(text(title)),
        )
        .child(small(text(description)).mt(px(6.0)))
}

pub(super) fn row(last: bool) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(24.0))
        .py(px(16.0))
        .min_w_0()
        .when(!last, |view| view.border_b_1().border_color(rgb(LINE)))
}

pub(super) fn action(
    id: impl Into<gpui::ElementId>,
    title: &str,
    name: Option<&'static str>,
) -> motion::MotionDiv {
    motion::button(id)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .min_h(px(32.0))
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(0x1a1827))
        .text_color(rgb(TEXT))
        .text_size(px(11.0))
        .line_height(px(13.75))
        .font_weight(FontWeight::MEDIUM)
        .flex_shrink_0()
        .cursor_pointer()
        .hover_colors(Some(rgb(0x242034).into()), None, Some(rgb(0x6d5aa4).into()))
        .when_some(name, |view, name| {
            let size = if matches!(title, "resetDefaults" | "saveChanges") {
                15.0
            } else {
                16.0
            };
            view.child(icon(name).size(px(size)))
        })
        .child(text(title))
}

impl Gallery {
    pub(super) fn settings_toggle(
        &self,
        index: usize,
        title: &str,
        description: &str,
        last: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        row(last)
            .id(("settings-toggle", index))
            .cursor_pointer()
            .child(label(title, description))
            .child(crate::ui::filters::Switch {
                active: self.settings.toggles[index],
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.settings.toggles[index] = !this.settings.toggles[index];
                this.settings.saved = false;
                cx.notify();
            }))
    }

    pub(super) fn settings_field(
        &self,
        index: usize,
        title: &str,
        description: &str,
        width: f32,
        cx: &Context<Self>,
    ) -> Div {
        row(false).child(label(title, description)).child(
            div()
                .id(("settings-field", index))
                .track_focus(&self.settings.fields[index].focus_handle(cx))
                .w(px(width))
                .flex_shrink_0()
                .h(px(36.0))
                .border_1()
                .border_color(rgb(0x514361))
                .rounded(px(6.0))
                .bg(rgb(0x10101a))
                .focus(|style| style.border_color(rgb(ACCENT)))
                .child(
                    Input::new(&self.settings.fields[index])
                        .appearance(false)
                        .h(px(34.0))
                        .text_size(px(12.0))
                        .line_height(px(15.0))
                        .px(px(11.0))
                        .py(px(9.5))
                        .text_color(rgb(TEXT)),
                ),
        )
    }

    pub(super) fn settings_action_row(
        &self,
        title: &str,
        description: &str,
        button: &'static str,
        name: Option<&'static str>,
        last: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        row(last).child(label(title, description)).child(
            action(button, button, name).on_click(
                cx.listener(move |this, _, _, cx| this.settings_preview_action(button, cx)),
            ),
        )
    }

    fn settings_card(&self, section: usize, width: f32, cx: &mut Context<Self>) -> Div {
        match section {
            0 | 1 => self.settings_path_card(section, cx),
            2 => self.settings_structure_card(width, cx),
            3 => self.settings_indexing_card(cx),
            4 => self.settings_cache_card(cx),
            5 => self.settings_catalog_card(cx),
            6 => self.settings_logs_card(cx),
            7 => self.settings_updates_card(cx),
            _ => self.settings_appearance_card(cx),
        }
    }

    pub(in crate::ui) fn settings_workspace(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let compact = f32::from(window.viewport_size().width) <= 1100.0;
        let active = self.settings.active_section();
        let mut navigation = div().flex().flex_col().gap(px(4.0));
        let mut sections = div()
            .w_full()
            .max_w(px(1040.0))
            .mx_auto()
            .px(px(24.0))
            .pt(px(16.0))
            .pb(px(24.0))
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .text_size(px(11.0))
                    .line_height(px(15.0))
                    .mb(px(16.0))
                    .child(self.settings.message.clone().unwrap_or_else(|| {
                        text(if self.settings.saved {
                            "saved"
                        } else {
                            "previewNotice"
                        })
                        .into()
                    })),
            );

        for (index, (title, name, description)) in SECTIONS.iter().copied().enumerate() {
            navigation = navigation.child(
                motion::button(("settings-section-link", index))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .min_h(px(36.0))
                    .p(px(8.0))
                    .rounded(px(6.0))
                    .text_size(px(13.0))
                    .line_height(px(16.25))
                    .text_color(rgb(if active == index { TEXT } else { MUTED }))
                    .cursor_pointer()
                    .hover_colors(Some(rgb(0x1a1827).into()), None, None)
                    .when(active == index, |view| {
                        view.bg(rgb(0x242034)).font_weight(FontWeight::SEMIBOLD)
                    })
                    .child(
                        icon(name)
                            .size(px(18.0))
                            .text_color(rgb(if active == index { ACCENT } else { MUTED })),
                    )
                    .child(text(title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let top = this.settings.positions[index].get() - 24.0;
                        this.settings
                            .scroll
                            .set_offset(point(px(0.0), px(-top.max(0.0))));
                        cx.notify();
                    })),
            );
            let position = self.settings.positions[index].clone();
            let scroll = self.settings.scroll.clone();
            sections = sections.child(
                div()
                    .relative()
                    .when(index > 0, |view| view.pt(px(20.0)))
                    .pb(px(20.0))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        gpui::canvas(
                            move |bounds, _, _| {
                                position.set(
                                    f32::from(
                                        bounds.origin.y
                                            - scroll.bounds().origin.y
                                            - scroll.offset().y,
                                    ) - if index > 0 { 20.0 } else { 0.0 },
                                );
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(
                        div()
                            .mb(px(16.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(20.0))
                                    .line_height(px(27.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(icon(name).size(px(20.0)).text_color(rgb(ACCENT)))
                                    .child(text(title)),
                            )
                            .child(small(text(description)).mt(px(8.0))),
                    )
                    .child(self.settings_card(index, if compact { 210.0 } else { 260.0 }, cx)),
            );
        }
        sections = sections.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .pt(px(20.0))
                .child(
                    action("settings-reset", "resetDefaults", Some("reset"))
                        .bg(crate::appearance::rgba(0))
                        .border_color(crate::appearance::rgba(0))
                        .px(px(0.0))
                        .text_color(rgb(ACCENT))
                        .border_0()
                        .py_0()
                        .font_weight(FontWeight::NORMAL)
                        .hover_colors(None, None, None)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.settings.toggles = [true, true, false, false, false];
                            this.settings.paths = [None, None];
                            this.settings.language = 0;
                            this.settings.theme = 0;
                            this.settings.saved = false;
                            this.settings.message = None;
                            for (field, value) in this.settings.fields.iter().zip(DEFAULT_FIELDS) {
                                field.update(cx, |field, cx| field.set_value(value, window, cx));
                            }
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            action("settings-cancel", "cancel", None).on_click(cx.listener(
                                |this, _, window, cx| {
                                    if this.dialog_open {
                                        return;
                                    }
                                    this.settings.paths = this.settings.saved_paths.clone();
                                    this.switch_workspace(false, window, cx);
                                },
                            )),
                        )
                        .child(
                            action("settings-save", "saveChanges", Some("check"))
                                .bg(motion::Surface::gradient(
                                    120.0,
                                    rgb(0x7b43d9),
                                    rgb(0x6839ce),
                                ))
                                .border_color(crate::appearance::rgba(0))
                                .text_color(rgb(0xffffff))
                                .hover_colors(Some(rgb(0x8853e1).into()), None, None)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if this.dialog_open {
                                        return;
                                    }
                                    this.settings.saved_paths = this.settings.paths.clone();
                                    this.apply_interface_preferences(window, cx);
                                    this.settings.saved = true;
                                    this.settings.message = None;
                                    tracing::info!(
                                        operation = "save_settings",
                                        storage = "demo_session",
                                        "Settings draft accepted for preview only"
                                    );
                                    cx.notify();
                                })),
                        ),
                ),
        );

        div()
            .ml(px(1.0))
            .mr(px(1.0))
            .mt(px(1.0))
            .mb(px(1.0))
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(10.0))
            .overflow_hidden()
            .bg(gpui::linear_gradient(
                110.0,
                gpui::linear_color_stop(rgb(SURFACE), 0.0),
                gpui::linear_color_stop(rgb(0x10101a), 1.0),
            ))
            .child(
                div()
                    .w(px(if compact { 188.0 } else { 220.0 }))
                    .h_full()
                    .flex_shrink_0()
                    .p(px(16.0))
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .pb(px(16.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(19.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(text("settings")),
                            )
                            .child(
                                div()
                                    .mt(px(8.0))
                                    .text_size(px(11.0))
                                    .line_height(px(15.0))
                                    .text_color(rgb(MUTED))
                                    .child(text("preview")),
                            ),
                    )
                    .child(navigation),
            )
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.settings.scroll)
                    .on_scroll_wheel(cx.listener(|_, _, _, cx| {
                        let id = cx.entity_id();
                        cx.defer(move |cx| cx.notify(id));
                    }))
                    .child(sections),
            )
    }
}

pub(super) fn badge(value: &str, info: bool) -> Div {
    div()
        .px(px(8.0))
        .py(px(3.0))
        .rounded_full()
        .bg(rgb(if info { 0x152b46 } else { 0x242034 }))
        .text_color(rgb(if info { 0x8bbbff } else { MUTED }))
        .text_size(px(10.0))
        .line_height(px(12.5))
        .child(value.to_owned())
}
