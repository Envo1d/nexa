use gpui::{BoxShadow, Context, Div, Focusable, div, point, prelude::*, px, rgb, rgba};
use gpui_component::input::Input;

use super::{
    ACCENT, Gallery, LINE, MUTED, TEXT,
    shell::{control, icon},
};

impl Gallery {
    pub(super) fn toolbar(&self, cx: &mut Context<Self>) -> Div {
        let search_empty = self.search.read(cx).value().is_empty();

        div()
            .flex()
            .flex_col()
            .py(px(16.0))
            .gap(px(12.0))
            .flex_shrink_0()
            .child(
                div()
                    .id("collection-search")
                    .relative()
                    .capture_key_down(cx.listener(|this, event, window, cx| {
                        this.search_suggestion_key(event, window, cx);
                    }))
                    .track_focus(&self.search.focus_handle(cx))
                    .flex()
                    .items_center()
                    .h(px(40.0))
                    .rounded(px(6.0))
                    .bg(rgb(0x1a1827))
                    .border_1()
                    .border_color(rgb(LINE))
                    .focus(|style| {
                        style.border_color(rgb(ACCENT)).shadow(vec![BoxShadow {
                            color: rgba(0xa786ff1c).into(),
                            offset: point(px(0.0), px(0.0)),
                            blur_radius: px(0.0),
                            spread_radius: px(2.0),
                        }])
                    })
                    .child(
                        Input::new(&self.search)
                            .appearance(false)
                            .h(px(38.0))
                            .flex_shrink_0()
                            .px(px(12.0))
                            .py(px(0.0))
                            .gap(px(8.0))
                            .text_color(rgb(TEXT))
                            .prefix(icon("search").size(px(17.0)).text_color(rgb(MUTED)))
                            .text_size(px(12.0))
                            .when(!search_empty, |input| {
                                input.suffix(
                                    super::motion::button("clear-search")
                                        .w(px(18.0))
                                        .h(px(24.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .text_color(rgb(MUTED))
                                        .hover_colors(None, Some(rgb(TEXT).into()), None)
                                        .child(icon("close").size(px(14.0)))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            if !this.stage {
                                                this.search_suggestions.applied.clear();
                                            }
                                            this.search.update(cx, |state, cx| {
                                                state.set_value("", window, cx);
                                                state.focus(window, cx);
                                            });
                                            this.refilter(cx);
                                        })),
                                )
                            })
                            .when(search_empty, |input| {
                                input.suffix(
                                    div()
                                        .px_1()
                                        .py(px(2.0))
                                        .rounded(px(3.0))
                                        .bg(rgb(0x242034))
                                        .text_color(rgb(MUTED))
                                        .text_size(px(10.0))
                                        .child("Ctrl F"),
                                )
                            }),
                    )
                    .when(!self.stage && self.search_suggestions.open, |d| {
                        d.children(self.search_suggestion_popup(cx))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .items_center()
                            .gap_2()
                            .child(
                                type_control(
                                    "all",
                                    "Все файлы",
                                    "files",
                                    self.type_counts[0],
                                    !self.videos_only && !self.images_only,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.videos_only = false;
                                        this.images_only = false;
                                        this.refilter(cx);
                                    },
                                )),
                            )
                            .child(
                                type_control(
                                    "videos",
                                    "Видео",
                                    "video",
                                    self.type_counts[1],
                                    self.videos_only,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.videos_only = true;
                                        this.images_only = false;
                                        this.refilter(cx);
                                    },
                                )),
                            )
                            .child(
                                type_control(
                                    "images",
                                    "Изображения",
                                    "image",
                                    self.type_counts[2],
                                    self.images_only,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.videos_only = false;
                                        this.images_only = true;
                                        this.refilter(cx);
                                    },
                                )),
                            ),
                    )
                    .child(self.sort_select(cx))
                    .child(
                        div()
                            .flex()
                            .p(px(3.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(0x10101a))
                            .child(
                                control("grid", "", "grid", !self.list_mode)
                                    .border_0()
                                    .bg(rgb(if self.list_mode { 0x10101a } else { 0x2c263f }))
                                    .text_color(rgb(if self.list_mode {
                                        MUTED
                                    } else {
                                        super::TEXT
                                    }))
                                    .h(px(28.0))
                                    .w(px(34.0))
                                    .justify_center()
                                    .rounded(px(5.0))
                                    .px_2()
                                    .py_1()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.list_mode = false;
                                        this.marquee = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                control("list", "", "list", self.list_mode)
                                    .border_0()
                                    .bg(rgb(if self.list_mode { 0x2c263f } else { 0x10101a }))
                                    .text_color(rgb(if self.list_mode {
                                        super::TEXT
                                    } else {
                                        MUTED
                                    }))
                                    .h(px(28.0))
                                    .w(px(34.0))
                                    .justify_center()
                                    .rounded(px(5.0))
                                    .px_2()
                                    .py_1()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.list_mode = true;
                                        this.marquee = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        control("details", "", "info", self.details_open)
                            .size(px(34.0))
                            .px(px(7.0))
                            .py(px(7.0))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.details_open = !this.details_open;
                                cx.notify();
                            })),
                    ),
            )
            .when(self.loading, |view| {
                view.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(MUTED))
                        .child("Подготовка выбранных файлов…"),
                )
            })
    }
}

fn type_control(
    id: &'static str,
    label: &str,
    name: &'static str,
    count: usize,
    active: bool,
) -> super::motion::MotionDiv {
    // The collection filters have their own transparent/selected surfaces;
    // shared shell controls use a raised background and different active colors.
    super::button_with_hover(id, "", 0x242034)
        .flex()
        .items_center()
        .flex_shrink_0()
        .h(px(43.0))
        .px(px(12.0))
        .py(px(10.0))
        .gap(px(8.0))
        .rounded(px(7.0))
        .text_size(px(12.0))
        .line_height(px(15.0))
        .text_color(rgb(TEXT))
        .bg(if active { rgba(0x2c2644ff) } else { rgba(0) })
        .border_color(rgb(if active { 0x6d5aa4 } else { LINE }))
        .child(icon(name).size(px(17.0)))
        .child(label.to_owned())
        .child(
            div()
                .px(px(6.0))
                .py(px(3.0))
                .rounded(px(6.0))
                .text_size(px(12.0))
                .line_height(px(15.0))
                .text_color(rgb(if active { 0xa786ff } else { MUTED }))
                .bg(rgb(if active { 0x2c2644 } else { 0x1a1827 }))
                .child(count.to_string()),
        )
}
