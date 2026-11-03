use gpui::{Context, Div, IntoElement, Stateful, Window, div, prelude::*, px, rgb, uniform_list};

use gpui_component::{
    IconName, Sizable,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
};

use super::{
    ACCENT, Gallery, LINE, MUTED,
    shell::{control, icon},
};

impl Gallery {
    pub(super) fn collection(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let navigation = if self.sidebar_expanded { 146.0 } else { 60.0 };
        let width = if self.collection_width > 0.0 {
            self.collection_width
        } else {
            f32::from(window.viewport_size().width) - navigation - 274.0
        };
        let layout = super::layout::CollectionLayout::new(width, self.list_mode);
        let columns = layout.columns;
        let rows = self.visible.len().div_ceil(columns);
        let gallery = cx.weak_entity();
        let list = uniform_list(
            "media-grid",
            rows,
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|row| {
                        div()
                            .flex()
                            .w_full()
                            .h(px(layout.row_height))
                            .pb(px(super::layout::GAP))
                            .px(px(1.95))
                            .gap(px(super::layout::GAP))
                            .children((0..columns).filter_map(|column| {
                                let &index = this.visible.get(row * columns + column)?;
                                Some(
                                    this.card(index, layout.preview_height, cx)
                                        .w(px(layout.card_width))
                                        .flex_none(),
                                )
                            }))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.scroll.clone())
        .w_full()
        .h_full();

        div()
            .relative()
            .w_full()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .pr(px(6.0))
            .overflow_hidden()
            .child(list)
            .child(
                gpui::canvas(
                    move |bounds, _, cx| {
                        let width = (f32::from(bounds.size.width) - 6.0).max(1.0);
                        let _ = gallery.update(cx, |this, cx| {
                            if (this.collection_width - width).abs() > 0.5 {
                                this.collection_width = width;
                                cx.notify();
                            }
                        });
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(gpui_component::scroll::Scrollbar::vertical(&self.scroll))
    }

    fn card(&self, index: usize, preview_height: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let entry = &self.entries[index];
        let list = self.list_mode;
        let selected = self.selected == Some(index);
        let characters = entry.video && !entry.characters.is_empty();
        let metadata = if characters {
            entry.characters.clone()
        } else if entry.tags.is_empty() {
            "Без тегов".into()
        } else {
            entry.tags.join(", ")
        };
        let title = entry.title.clone();
        let metadata_tip = metadata.clone();
        let date = if list {
            entry
                .date_label
                .clone()
                .unwrap_or_else(|| "Дата недоступна".into())
        } else {
            super::layout::compact_date(entry.date_label.as_deref())
        };
        let preview = div()
            .relative()
            .overflow_hidden()
            .flex_shrink_0()
            .when(list, |view| view.w(px(112.0)).h_full())
            .when(!list, |view| view.w_full().h(px(preview_height)))
            .when(self.show_previews, |view| {
                view.child(self.picture_with_corners(index, true, Some(list)))
            })
            .when(!self.show_previews, |view| {
                view.flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(0x242034))
                    .child(icon(if entry.video { "video" } else { "image" }))
            })
            .child(
                div()
                    .absolute()
                    .top(px(if list { 5.0 } else { 7.0 }))
                    .left(px(if list { 5.0 } else { 7.0 }))
                    .px(px(5.0))
                    .py(px(3.0))
                    .rounded_sm()
                    .bg(gpui::rgba(0x0a0a12c2))
                    .text_size(px(if list { 8.0 } else { 10.0 }))
                    .child(entry.extension.clone()),
            )
            .when_some(entry.duration.clone(), |view, duration| {
                view.child(
                    div()
                        .absolute()
                        .bottom(px(if list { 5.0 } else { 7.0 }))
                        .right(px(if list { 5.0 } else { 7.0 }))
                        .px(px(5.0))
                        .py(px(3.0))
                        .rounded_sm()
                        .bg(gpui::rgba(0x0a0a12c2))
                        .text_size(px(if list { 8.0 } else { 10.0 }))
                        .child(duration),
                )
            });
        let title_row = div()
            .h(px(if list { 19.0 } else { 39.0 }))
            .flex_shrink_0()
            .flex()
            .items_center()
            .when(!list, |view| view.pl(px(9.75)).pr(px(39.0)))
            .child(
                div()
                    .id("card-title")
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(entry.title.clone())
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(title.clone()).build(window, cx)
                    }),
            );
        let footer = div()
            .flex()
            .items_center()
            .gap(px(8.45))
            .min_w_0()
            .when(!list, |view| {
                view.h(px(35.75))
                    .bg(rgb(0x10101a))
                    .flex_shrink_0()
                    .px(px(7.8))
                    .border_t_1()
                    .border_color(rgb(LINE))
            })
            .text_color(rgb(MUTED))
            .text_size(px(10.4))
            .child(div().flex_shrink_0().child(date))
            .child(
                div()
                    .id("card-metadata")
                    .flex()
                    .items_center()
                    .gap(px(5.85))
                    .min_w_0()
                    .flex_1()
                    .when(!list, |view| view.justify_end())
                    .text_color(rgb(if characters { 0xb0dfc7 } else { 0xd0b4fa }))
                    .text_size(px(11.05))
                    .child(icon(if characters { "person" } else { "tag" }).size(px(15.0)))
                    .child(div().min_w_0().truncate().child(metadata))
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(metadata_tip.clone())
                            .build(window, cx)
                    }),
            );
        let body = div()
            .flex()
            .flex_col()
            .min_w_0()
            .when(list, |view| {
                view.flex_1()
                    .justify_center()
                    .pl(px(12.0))
                    .pr(px(76.0))
                    .py(px(7.0))
                    .gap(px(5.0))
            })
            .child(title_row)
            .child(footer);

        div()
            .id(index)
            .relative()
            .flex()
            .flex_col()
            .min_w_0()
            .h_full()
            .rounded(px(8.0))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(if self.selected == Some(index) {
                ACCENT
            } else {
                LINE
            }))
            .bg(rgb(0x10101a))
            .when(list, |view| view.flex_row())
            .cursor_pointer()
            .hover(move |view| {
                view.border_color(rgb(if selected { ACCENT } else { 0x514361 }))
                    .bg(rgb(0x242034))
            })
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    this.selected = Some(index);
                    if event.click_count() == 2 {
                        this.open_selected(window, cx);
                    }
                    cx.notify();
                }),
            )
            .child(preview)
            .child(body)
            .child(
                control("favorite", "", "heart", false)
                    .absolute()
                    .top(px(if list { 10.0 } else { 4.0 }))
                    .right(px(if list { 8.0 } else { 4.0 }))
                    .size(px(28.0))
                    .px(px(5.0))
                    .py(px(5.0))
                    .border_0()
                    .bg(gpui::rgba(0x0a0a1273))
                    .text_color(rgb(if entry.favorite { ACCENT } else { 0xffffff }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.entries[index].favorite = !this.entries[index].favorite;
                        tracing::debug!(operation = "local_favorite", "Session favorite toggled");
                        this.refilter(cx);
                    })),
            )
            .child(
                div()
                    .absolute()
                    .right(px(if list { 8.0 } else { 4.0 }))
                    .top(px(if list { 40.0 } else { preview_height + 5.0 }))
                    .child(self.card_actions(index, cx)),
            )
    }

    fn card_actions(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let gallery = cx.weak_entity();
        let source = self.entries[index].path.clone();

        Button::new("card-actions")
            .icon(IconName::Ellipsis)
            .ghost()
            .small()
            .text_color(rgb(MUTED))
            .dropdown_menu(move |menu, _, _| {
                let open = gallery.clone();
                let details = gallery.clone();
                let open_source = source.clone();
                let details_source = source.clone();

                menu.item(
                    PopupMenuItem::new("Открыть").on_click(move |_, window, cx| {
                        let _ = open.update(cx, |this, cx| {
                            if this
                                .entries
                                .get(index)
                                .is_some_and(|entry| entry.path == open_source)
                            {
                                this.selected = Some(index);
                                this.open_selected(window, cx);
                            }
                        });
                    }),
                )
                .item(
                    PopupMenuItem::new("Информация о файле").on_click(move |_, _, cx| {
                        let _ = details.update(cx, |this, cx| {
                            if this
                                .entries
                                .get(index)
                                .is_some_and(|entry| entry.path == details_source)
                            {
                                this.selected = Some(index);
                                this.details_open = true;
                                cx.notify();
                            }
                        });
                    }),
                )
            })
    }
}
