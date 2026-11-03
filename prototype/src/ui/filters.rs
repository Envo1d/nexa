use gpui::{Context, Div, FontWeight, Stateful, Window, div, prelude::*, px, rgb, rgba};
use gpui_component::scroll::{Scrollbar, ScrollbarShow};

use super::{ACCENT, Gallery, LINE, MUTED, SURFACE, shell::control};

mod switch;

impl Gallery {
    pub(super) fn filters(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let viewport = f32::from(window.viewport_size().width);
        let compact = f32::from(window.viewport_size().height) <= 760.0;
        let row_height = if compact { 36.0 } else { 44.0 };
        let section_padding = if compact {
            12.0
        } else if viewport >= 1450.0 {
            20.0
        } else {
            16.0
        };
        let mut tags = self.tag_counts.clone();
        let maximum = tags.first().map_or(1, |tag| tag.1).max(1);
        if !self.show_all_tags {
            tags.truncate(8);
        } else {
            tags.sort_by(|left, right| left.0.cmp(&right.0));
        }

        let tags_height = tags.len() as f32 * row_height;

        div()
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_hidden()
            .w(px(super::layout::filter_width(viewport)))
            .flex_shrink_0()
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(LINE))
            .bg(gpui::linear_gradient(
                120.0,
                gpui::linear_color_stop(rgb(SURFACE), 0.0),
                gpui::linear_color_stop(rgb(0x10101a), 1.0),
            ))
            .child(
                div()
                    .h(px(52.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .p_4()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_size(px(14.0))
                    .child("Теги"),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .pt(px(4.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_h_0()
                            .h(px(tags_height + 62.25))
                            .px_4()
                            .pb_3()
                            .child(
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .mb_3()
                                    .p(px(2.0))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .rounded(px(7.0))
                                    .bg(rgb(0x1a1927))
                                    .child(
                                        tag_mode("frequent", "Частые", !self.show_all_tags)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.show_all_tags = false;
                                                this.tags_scroll
                                                    .set_offset(gpui::point(px(0.0), px(0.0)));
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        tag_mode("all-tags", "Все", self.show_all_tags).on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.show_all_tags = true;
                                                this.tags_scroll
                                                    .set_offset(gpui::point(px(0.0), px(0.0)));
                                                cx.notify();
                                            }),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .relative()
                                    .flex_1()
                                    .min_h_0()
                                    .child(
                                        div()
                                            .id("filter-tags-scroll")
                                            .h_full()
                                            .track_scroll(&self.tags_scroll)
                                            .overflow_y_scroll()
                                            .pl(px(2.0))
                                            .pr(px(8.0))
                                            .children(tags.into_iter().enumerate().map(
                                                |(index, (tag, count))| {
                                                    let text = tag.clone();
                                                    super::motion::button(index)
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .h(px(row_height))
                                                        .flex_shrink_0()
                                                        .line_height(px(16.25))
                                                        .px(px(8.0))
                                                        .border_b_1()
                                                        .border_color(rgb(LINE))
                                                        .cursor_pointer()
                                                        .hover_colors(
                                                            Some(rgba(0xeeedf60a).into()),
                                                            Some(rgb(0xd0b4fa).into()),
                                                            None,
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .truncate()
                                                                .text_size(px(13.0))
                                                                .child(tag),
                                                        )
                                                        .child(
                                                            div()
                                                                .w(px(58.5))
                                                                .flex_shrink_0()
                                                                .h(px(4.0))
                                                                .rounded_md()
                                                                .bg(rgb(0x242034))
                                                                .child(
                                                                    div()
                                                                        .w(px(58.5 * count as f32
                                                                            / maximum as f32))
                                                                        .h_full()
                                                                        .rounded_md()
                                                                        .bg(rgb(ACCENT)),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .w(px(31.2))
                                                                .flex_shrink_0()
                                                                .text_right()
                                                                .text_size(px(11.0))
                                                                .text_color(rgb(MUTED))
                                                                .child(count.to_string()),
                                                        )
                                                        .on_click(cx.listener(
                                                            move |this, _, window, cx| {
                                                                this.add_search(&text, window, cx);
                                                            },
                                                        ))
                                                },
                                            ))
                                            .when(
                                                self.entries
                                                    .iter()
                                                    .all(|entry| entry.tags.is_empty()),
                                                |view| {
                                                    view.child(
                                                        div()
                                                            .py_3()
                                                            .text_color(rgb(MUTED))
                                                            .child("Нет тегов"),
                                                    )
                                                },
                                            ),
                                    )
                                    .child(
                                        div().absolute().top_0().right_0().bottom_0().child(
                                            Scrollbar::vertical(&self.tags_scroll)
                                                .scrollbar_show(ScrollbarShow::Scrolling),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .px_4()
                            .py(px(section_padding))
                            .child(
                                div()
                                    .mb_0()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .line_height(px(18.0))
                                    .child("Дополнительно"),
                            )
                            .child(
                                toggle("tagged", "Только с тегами", self.tagged_only).on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.tagged_only = !this.tagged_only;
                                        this.refilter(cx);
                                    }),
                                ),
                            )
                            .child(
                                toggle("hide-untagged", "Скрыть без тегов", self.hide_untagged)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.hide_untagged = !this.hide_untagged;
                                        this.refilter(cx);
                                    })),
                            )
                            .child(
                                toggle("previews", "Показывать превью", self.show_previews)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_previews = !this.show_previews;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .px_4()
                    .py(px(if compact { 8.0 } else { 16.0 }))
                    .gap_2()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(
                        control("reset", "Сбросить фильтры", "reset", false)
                            .h(px(34.0))
                            .py_0()
                            .justify_center()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::MEDIUM)
                            .rounded(px(6.0))
                            .bg(rgb(0x1a1827))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.videos_only = false;
                                this.images_only = false;
                                this.tagged_only = false;
                                this.hide_untagged = false;
                                this.favorites_only = false;
                                this.sort = crate::catalog::SortMode::Newest;
                                this.search
                                    .update(cx, |state, cx| state.set_value("", window, cx));
                                this.refilter(cx);
                            })),
                    )
                    .child(
                        control("duplicates", "Найти дубликаты", "copy", false)
                            .h(px(34.0))
                            .py_0()
                            .justify_center()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::MEDIUM)
                            .rounded(px(6.0))
                            .bg(rgb(0x1a1827))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.unavailable("Поиск дубликатов", cx)
                            })),
                    ),
            )
    }
}

fn tag_mode(id: &'static str, label: &str, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .flex_basis(px(0.0))
        .min_w_0()
        .flex()
        .items_center()
        .justify_center()
        .h(px(32.25))
        .px_1()
        .rounded(px(5.0))
        .text_size(px(13.0))
        .line_height(px(16.25))
        .text_color(rgb(if active { 0xffffff } else { MUTED }))
        .when(active, |view| view.bg(rgb(0x7b43d9)))
        .cursor_pointer()
        .child(label.to_owned())
}

pub(super) fn toggle(id: &'static str, label: &str, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .mt(px(12.0))
        .h(px(18.0))
        .line_height(px(16.5))
        .cursor_pointer()
        .child(switch::Switch { active })
        .child(div().text_size(px(11.0)).child(label.to_owned()))
}
