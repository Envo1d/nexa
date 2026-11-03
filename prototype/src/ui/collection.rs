use gpui::{Context, Div, IntoElement, Stateful, Window, div, prelude::*, px, rgb, uniform_list};

use gpui_component::{
    IconName, Sizable,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
};

use super::{ACCENT, Gallery, LINE, MUTED, shell::icon};

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
        let wide = f32::from(window.viewport_size().width) >= 1450.0;
        let gallery = cx.weak_entity();
        let pointer_gallery = gallery.clone();
        let release_gallery = gallery.clone();
        let inset = if self.stage {
            super::layout::COLLECTION_INSET
        } else {
            0.0
        };
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
                                    this.card(index, layout.preview_height, wide, cx)
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
            .id("collection-viewport")
            .relative()
            .w_full()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .pl(px(inset))
            .pr(px(inset + 6.0))
            .overflow_hidden()
            .capture_any_mouse_down(cx.listener(move |this, event, window, cx| {
                this.start_marquee(event, layout, window, cx);
            }))
            .child(list)
            .when(rows == 0, |view| {
                view.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .text_color(rgb(MUTED))
                        .child(icon("search").size(px(32.0)))
                        .child(if self.stage {
                            "Входящие файлы не найдены"
                        } else {
                            "Файлы не найдены"
                        })
                        .child("Попробуйте изменить поиск или сбросить фильтры."),
                )
            })
            .child(
                gpui::canvas(
                    move |bounds, _, cx| {
                        let width = (f32::from(bounds.size.width) - 2.0 * inset - 6.0).max(1.0);
                        let _ = gallery.update(cx, |this, cx| {
                            if this.collection_bounds != bounds {
                                this.marquee = None;
                                this.collection_bounds = bounds;
                            }
                            if (this.collection_width - width).abs() > 0.5 {
                                this.collection_width = width;
                                cx.notify();
                            }
                        });
                    },
                    move |_, (), window, _| {
                        window.on_mouse_event::<gpui::MouseMoveEvent>(
                            move |event, phase, _, cx| {
                                if !phase.capture() {
                                    return;
                                }

                                let _ = pointer_gallery.update(cx, |this, cx| {
                                    if let Some(marquee) = this.marquee.as_mut() {
                                        if event.pressed_button != Some(gpui::MouseButton::Left) {
                                            this.finish_marquee(cx);
                                        } else {
                                            marquee.pointer = event.position;
                                            this.update_marquee(cx);
                                        }
                                    }
                                });
                            },
                        );
                        window.on_mouse_event::<gpui::MouseUpEvent>(move |event, phase, _, cx| {
                            if !phase.capture() || event.button != gpui::MouseButton::Left {
                                return;
                            }

                            let _ = release_gallery.update(cx, |this, cx| {
                                if let Some(marquee) = this.marquee.as_mut() {
                                    marquee.pointer = event.position;
                                    this.update_marquee(cx);
                                    this.finish_marquee(cx);
                                    cx.stop_propagation();
                                }
                            });
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .when_some(self.marquee.as_ref(), |view, marquee| {
                let offset = self.scroll.0.borrow().base_handle.offset().y;
                let rectangle = marquee.rectangle(offset);

                view.child(
                    div()
                        .absolute()
                        .left(rectangle.origin.x)
                        .top(rectangle.origin.y + offset)
                        .w(rectangle.size.width)
                        .h(rectangle.size.height)
                        .border_1()
                        .border_color(rgb(ACCENT))
                        .bg(gpui::rgba(0xa786ff2e)),
                )
            })
            .child(gpui_component::scroll::Scrollbar::vertical(&self.scroll))
    }

    fn card(
        &self,
        index: usize,
        preview_height: f32,
        wide: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let entry = &self.entries[index];
        let list = self.list_mode;
        let selected = if self.stage {
            self.stage_selection.contains(&index)
        } else {
            self.selected == Some(index)
        };
        let characters = entry.video && !entry.characters.is_empty();
        let metadata = if characters {
            entry.characters.clone()
        } else if entry.tags.is_empty() {
            "Без тегов".into()
        } else {
            entry.tags.join(", ")
        };
        let title = entry.title.clone();
        let folder = entry
            .details
            .relative_path
            .rsplit_once('/')
            .map(|(folder, _)| folder.to_owned());
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
                    .bottom_0()
                    .w_full()
                    .h(gpui::relative(0.35))
                    .bg(gpui::linear_gradient(
                        180.0,
                        gpui::linear_color_stop(gpui::rgba(0x0a0a1200), 0.0),
                        gpui::linear_color_stop(gpui::rgba(0x0a0a1226), 1.0),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .top(px(if list { 5.0 } else { 7.0 }))
                    .left(px(if list { 5.0 } else { 7.0 }))
                    .px(px(5.0))
                    .py(px(3.0))
                    .rounded(px(4.0))
                    .bg(gpui::rgba(0x0a0a12c2))
                    .text_size(px(if list { 8.0 } else { 10.0 }))
                    .line_height(px(if list { 10.0 } else { 12.5 }))
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
                        .rounded(px(4.0))
                        .bg(gpui::rgba(0x0a0a12c2))
                        .text_size(px(if list { 8.0 } else { 10.0 }))
                        .line_height(px(if list { 10.0 } else { 12.5 }))
                        .child(duration),
                )
            });
        let title_row = div()
            .h(px(if list {
                if self.stage { 34.0 } else { 19.0 }
            } else {
                39.0
            }))
            .flex_shrink_0()
            .flex()
            .items_center()
            .when(self.stage, |view| {
                view.w_full().flex_col().items_start().justify_center()
            })
            .when(!list, |view| view.pl(px(9.75)).pr(px(39.0)))
            .child(
                div()
                    .id("card-title")
                    .min_w_0()
                    .when(!self.stage, |view| view.truncate())
                    .when(self.stage, |view| {
                        view.w_full().overflow_hidden().whitespace_nowrap()
                    })
                    .text_size(px(if wide { 12.0 } else { 13.0 }))
                    .line_height(px(if list || self.stage {
                        if wide { 16.8 } else { 18.2 }
                    } else {
                        39.0
                    }))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(entry.title.clone())
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(title.clone()).build(window, cx)
                    }),
            )
            .when(self.stage, |view| {
                view.when_some(folder, |view, folder| {
                    view.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .min_w_0()
                            .w_full()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(10.4))
                            .line_height(px(16.9))
                            .text_color(rgb(MUTED))
                            .child(icon("folder").size(px(12.0)))
                            .child(folder),
                    )
                })
            });
        let footer = div()
            .flex()
            .when(self.stage, |view| view.w_full())
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
            .text_size(px(if wide { 11.0 } else { 10.4 }))
            .line_height(px(if wide { 13.75 } else { 13.0 }))
            .child(div().flex_shrink_0().child(date))
            .when(self.stage, |view| {
                view.child(div().flex_1()).child(
                    entry
                        .details
                        .size
                        .map(super::details::format_size)
                        .unwrap_or_else(|| "Недоступно".into()),
                )
            })
            .when(!self.stage, |view| {
                view.child(
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
                        .line_height(px(13.8125))
                        .child(
                            icon(if characters { "person" } else { "tag" })
                                .size(px(15.0))
                                .text_color(rgb(if characters { 0xb0dfc7 } else { ACCENT })),
                        )
                        .child(div().min_w_0().truncate().child(metadata))
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(metadata_tip.clone())
                                .build(window, cx)
                        }),
                )
            });
        let body = div()
            .flex()
            .flex_col()
            .min_w_0()
            .when(self.stage && !list, |view| view.w_full())
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
            .border_color(rgb(if selected { ACCENT } else { LINE }))
            .bg(rgb(0x10101a))
            .when(list, |view| view.flex_row())
            .cursor_pointer()
            .hover(move |view| {
                view.border_color(rgb(if selected { ACCENT } else { 0x514361 }))
                    .bg(rgb(0x242034))
            })
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    this.select_card(index, event.modifiers());
                    if event.click_count() == 2 {
                        this.open_selected(window, cx);
                    }
                    cx.notify();
                }),
            )
            .child(preview)
            .child(body)
            .when(!self.stage, |view| {
                view.child(
                    div()
                        .id("favorite")
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.0))
                        .cursor_pointer()
                        .child(icon("heart").size(px(17.0)))
                        .absolute()
                        .top(px(if list { 10.0 } else { 4.0 }))
                        .right(px(if list { 8.0 } else { 4.0 }))
                        .size(px(28.0))
                        .px(px(5.0))
                        .py(px(5.0))
                        .border_0()
                        .bg(gpui::rgba(0x0a0a1273))
                        .text_color(rgb(if entry.favorite { ACCENT } else { 0xffffff }))
                        .hover(|view| view.bg(gpui::rgba(0x0a0a1273)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.entries[index].favorite = !this.entries[index].favorite;
                            tracing::debug!(
                                operation = "local_favorite",
                                "Session favorite toggled"
                            );
                            this.refilter(cx);
                        })),
                )
            })
            .child(
                div()
                    .absolute()
                    .right(px(if list { 8.0 } else { 5.2 }))
                    .bottom(px(if list { 8.0 } else { 40.3 }))
                    .child(self.card_actions(index, cx)),
            )
    }

    fn card_actions(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let gallery = cx.weak_entity();
        let source = self.entries[index].path.clone();
        let stage = self.stage;

        Button::new("card-actions")
            .child(
                super::motion::button("card-actions-content")
                    .size(px(28.6))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.0))
                    .text_color(rgb(MUTED))
                    .hover_colors(
                        Some(rgb(0x242034).into()),
                        Some(rgb(super::TEXT).into()),
                        None,
                    )
                    .child(gpui_component::Icon::new(IconName::Ellipsis).size(px(17.0))),
            )
            .text()
            .with_size(gpui_component::Size::Size(px(28.6)))
            .w(px(28.6))
            .h(px(28.6))
            .p_0()
            .rounded(px(5.0))
            .text_color(rgb(MUTED))
            .dropdown_menu(move |menu, _, _| {
                let open = gallery.clone();
                let details = gallery.clone();
                let open_source = source.clone();
                let details_source = source.clone();

                menu.item(
                    PopupMenuItem::new("Открыть").on_click(move |_, window, cx| {
                        let _ = open.update(cx, |this, cx| {
                            if this.stage == stage
                                && this
                                    .entries
                                    .get(index)
                                    .is_some_and(|entry| entry.path == open_source)
                            {
                                this.select_card(index, gpui::Modifiers::default());
                                this.open_selected(window, cx);
                            }
                        });
                    }),
                )
                .item(
                    PopupMenuItem::new("Информация о файле").on_click(move |_, _, cx| {
                        let _ = details.update(cx, |this, cx| {
                            if this.stage == stage
                                && this
                                    .entries
                                    .get(index)
                                    .is_some_and(|entry| entry.path == details_source)
                            {
                                this.select_card(index, gpui::Modifiers::default());
                                this.details_open = true;
                                cx.notify();
                            }
                        });
                    }),
                )
                .when(stage, |menu| {
                    let distribute = gallery.clone();
                    let source = source.clone();
                    menu.item(
                        PopupMenuItem::new("Распределить").on_click(move |_, _, cx| {
                            let _ = distribute.update(cx, |this, cx| {
                                if this.stage
                                    && this
                                        .entries
                                        .get(index)
                                        .is_some_and(|entry| entry.path == source)
                                {
                                    if !this.stage_selection.contains(&index) {
                                        this.select_card(index, gpui::Modifiers::default());
                                    }
                                    this.request_distribution(cx);
                                }
                            });
                        }),
                    )
                })
            })
    }
}
