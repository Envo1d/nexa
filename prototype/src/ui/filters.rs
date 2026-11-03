use gpui::{Context, Div, Stateful, Window, div, prelude::*, px, rgb};

use super::{ACCENT, Gallery, LINE, MUTED, SURFACE, TEXT, button, shell::control};

impl Gallery {
    pub(super) fn filters(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let viewport = f32::from(window.viewport_size().width);
        let mut tags = self.tag_counts.clone();
        let maximum = tags.first().map_or(1, |tag| tag.1).max(1);
        if !self.show_all_tags {
            tags.truncate(8);
        } else {
            tags.sort_by(|left, right| left.0.cmp(&right.0));
        }

        div()
            .flex()
            .flex_col()
            .w(px(super::layout::filter_width(viewport)))
            .flex_shrink_0()
            .rounded_lg()
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(SURFACE))
            .child(
                div()
                    .h(px(52.0))
                    .p_4()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_size(px(14.0))
                    .child("Теги"),
            )
            .child(
                div()
                    .flex()
                    .mx_4()
                    .p(px(2.0))
                    .border_1()
                    .border_color(rgb(LINE))
                    .rounded_md()
                    .bg(rgb(0x1a1827))
                    .child(
                        button("frequent", "Частые")
                            .flex_1()
                            .flex_basis(px(0.0))
                            .min_w_0()
                            .h(px(32.0))
                            .py_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_0()
                            .when(!self.show_all_tags, |view| view.bg(rgb(0x7b43d9)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_all_tags = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        button("all-tags", "Все")
                            .flex_1()
                            .flex_basis(px(0.0))
                            .min_w_0()
                            .h(px(32.0))
                            .py_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_0()
                            .when(self.show_all_tags, |view| view.bg(rgb(0x7b43d9)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_all_tags = true;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .id("filters-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_4()
                    .py_3()
                    .children(tags.into_iter().enumerate().map(|(index, (tag, count))| {
                        let text = tag.clone();
                        div()
                            .id(index)
                            .flex()
                            .items_center()
                            .gap_2()
                            .h(px(44.0))
                            .px(px(8.0))
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .cursor_pointer()
                            .hover(|view| view.bg(rgb(0x242034)))
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
                                            .w(px(58.5 * count as f32 / maximum as f32))
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
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_search(&text, window, cx);
                            }))
                    }))
                    .when(
                        self.entries.iter().all(|entry| entry.tags.is_empty()),
                        |view| view.child(div().py_3().text_color(rgb(MUTED)).child("Нет тегов")),
                    )
                    .child(
                        div()
                            .mt(px(if viewport >= 1450.0 { 32.0 } else { 24.0 }))
                            .mb_3()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
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
                        toggle("hide-untagged", "Скрыть без тегов", self.hide_untagged).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.hide_untagged = !this.hide_untagged;
                                this.refilter(cx);
                            }),
                        ),
                    )
                    .child(
                        toggle("previews", "Показывать миниатюры", self.show_previews).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.show_previews = !this.show_previews;
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p_4()
                    .gap_2()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(
                        control("reset", "Сбросить фильтры", "reset", false)
                            .h(px(34.0))
                            .py_0()
                            .justify_center()
                            .text_size(px(11.0))
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
                            .bg(rgb(0x1a1827))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.unavailable("Поиск дубликатов", cx)
                            })),
                    ),
            )
    }
}

fn toggle(id: &'static str, label: &str, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .h(px(28.0))
        .cursor_pointer()
        .child(
            div()
                .w(px(30.0))
                .h(px(18.0))
                .border_1()
                .border_color(rgb(LINE))
                .rounded_full()
                .p(px(2.0))
                .bg(rgb(if active { 0x7b43d9 } else { 0x252434 }))
                .flex()
                .when(active, |view| view.justify_end())
                .child(div().size(px(12.0)).rounded_full().bg(rgb(TEXT))),
        )
        .child(div().text_size(px(11.0)).child(label.to_owned()))
}
