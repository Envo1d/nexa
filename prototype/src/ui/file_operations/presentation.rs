use gpui::{Context, Div, Window, div, prelude::*, px};
use gpui_component::input::Input;

use super::{Action, Gallery};
use crate::appearance::{rgb, rgba};
use crate::ui::{ACCENT, LINE, MUTED, SURFACE, TEXT, motion, shell::icon};

fn operation_button(id: impl Into<gpui::ElementId>, label: &str) -> motion::MotionDiv {
    super::super::button(id, label)
        .keyboard_focus(true)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .py(px(0.))
        .gap(px(8.))
        .line_height(px(13.75))
        .h(px(34.))
        .px(px(18.))
        .rounded(px(6.))
        .text_size(px(11.))
        .font_weight(gpui::FontWeight::MEDIUM)
}

impl Gallery {
    pub(in crate::ui) fn file_operation_dialog(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let draft = self.file_operation.as_ref().unwrap();
        let delete = draft.action == Action::Delete;
        let replace = draft.action == Action::Replace;
        let title = match draft.action {
            Action::Delete if draft.indices.len() == 1 => crate::i18n::text("Удалить файл?").into(),
            Action::Delete => {
                if crate::i18n::english() {
                    format!("Delete {} files?", draft.indices.len())
                } else {
                    format!("Удалить {} файлов?", draft.indices.len())
                }
            }
            Action::Replace => crate::i18n::text("Заменить в библиотеке").into(),
            _ => crate::i18n::text("Изменить порядок файлов").into(),
        };
        let width = (f32::from(window.viewport_size().width) - 48.).min(if delete {
            500.
        } else if replace {
            780.
        } else {
            720.
        });
        let mut body = div()
            .id("file-operation-body")
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(if delete { 0. } else { 14. }))
            .when(!delete, |body| body.px(px(18.)))
            .when(replace, |body| body.pb(px(16.)));
        if delete {
            body = body.child(div().text_color(rgb(MUTED)).mt(px(8.)).mb(px(16.)).child(
                crate::i18n::text("Файл будет удалён из Nexa в текущей сессии."),
            ));
            let mut files = div()
                .id("delete-files")
                .max_h(px(260.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(8.));
            for &index in draft.indices.iter().take(5) {
                files = files.child(self.operation_file_row(index));
            }
            body = body.child(files);
            if draft.indices.len() > 5 {
                body = body.child(if crate::i18n::english() {
                    format!("And {} more file(s).", draft.indices.len() - 5)
                } else {
                    format!("И ещё {} файл(ов).", draft.indices.len() - 5)
                });
            }
            body = body.child(div().mt(px(16.)).p_3().line_height(px(20.4)).bg(rgba(0x38212833)).border_1().border_color(rgb(0x824254)).rounded(px(8.)).text_color(rgb(0xf2a0b1)).child(crate::i18n::text("Изолированный прототип: оригиналы на диске сохраняются. Корзина и удаление из рабочего каталога ещё не подключены.")));
        } else if replace {
            body = body.child(version_panel(
                crate::i18n::text("Новая версия · Входящие"),
                &self.entries[draft.indices[0]],
                self.show_previews,
                false,
                cx,
            ));
            if let Some(target) = draft.target {
                let main = &self.inactive_workspace.entries[target];
                body = body
                    .child(version_panel(crate::i18n::text("Текущая версия · Библиотека"), main, self.show_previews, true, cx))
                    .child(operation_button("replacement-another", crate::i18n::text("Выбрать другой файл")).w_full().on_click(cx.listener(|this, _, _, cx| {
                        let d = this.file_operation.as_mut().unwrap();
                        d.target = None;
                        d.acknowledged = false;
                        cx.notify();
                    })))
                    .child(crate::i18n::text("Библиотека сохраняет автора, произведения, персонажей, теги, избранное, название видео и порядок файлов. Новое содержимое, формат и технические сведения заменяют текущую версию."));

                let source = &self.entries[draft.indices[0]];
                let has_annotations = source.favorite
                    || !source.tags.is_empty()
                    || source.details.artist.is_some()
                    || !source.details.copyrights.is_empty()
                    || !source.details.character_copyrights.is_empty();
                if has_annotations {
                    body = body.child(div().text_color(rgb(0xefbc79)).child(crate::i18n::text("Существующие аннотации файла из входящих будут отброшены. Аннотации библиотеки имеют приоритет.")));
                }

                body = body
                    .child(div().text_color(rgb(MUTED)).child(crate::i18n::text("Синхронизация с диском выключена. Новая версия остаётся на своём месте и становится файлом библиотеки.")))
                    .child(div().text_color(rgb(0xefbc79)).child(crate::i18n::text("После подтверждения старая версия заменяется в текущей сессии. В изолированном прототипе оригиналы на диске сохраняются; безвозвратное удаление и очистка резервных копий не подключены.")))
                    .child(motion::button("replacement-ack")
                        .keyboard_focus(true).cursor_pointer().flex().items_center().gap(px(10.))
                        .line_height(px(19.2)).py(px(4.))
                        .child(div().size(px(13.)).flex_shrink_0().border_1().border_color(rgb(MUTED)).rounded(px(2.))
                            .bg(rgb(if draft.acknowledged { ACCENT } else { 0x393939 }))
                            .flex().items_center().justify_center()
                            .when(draft.acknowledged, |d| d.child(icon("check").size(px(11.)).text_color(rgb(0xffffff)))))
                        .child(crate::i18n::text("Заменить этот файл библиотеки выбранной версией из входящих."))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let d = this.file_operation.as_mut().unwrap();
                            d.acknowledged = !d.acknowledged;
                            cx.notify();
                        })));
            } else {
                body = body.child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(crate::i18n::text("Выберите файл для замены в библиотеке")),
                );
                let mut search = div().relative().child(Input::new(&draft.query).h(px(38.)));
                if draft.suggestions_open {
                    let options = super::super::search_suggestions::matches(
                        &draft.query.read(cx).value(),
                        &draft.catalog.records,
                    );
                    let mut suggestions = div()
                        .id("replacement-suggestions")
                        .max_h(px(240.))
                        .overflow_y_scroll()
                        .p(px(6.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(0x1a1927));
                    for (i, record) in options.iter().enumerate() {
                        let name = record.name.clone();
                        let label = if record.copyright.is_empty() {
                            name.clone()
                        } else {
                            format!("{} ({})", name, record.copyright)
                        };
                        suggestions = suggestions.child(
                            operation_button(("replacement-option", i), "")
                                .justify_between()
                                .px(px(8.))
                                .h(px(30.))
                                .border_0()
                                .child(div().flex_1().child(label))
                                .child(
                                    div()
                                        .text_size(px(9.))
                                        .text_color(rgb(MUTED))
                                        .child(record.category.singular()),
                                )
                                .w_full()
                                .when(draft.suggestion_active == Some(i), |d| d.bg(rgb(0x372f56)))
                                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                                    window.prevent_default()
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let d = this.file_operation.as_mut().unwrap();
                                    let value = super::super::search_suggestions::choose(
                                        &d.query.read(cx).value(),
                                        &name,
                                    );
                                    d.query.update(cx, |input, cx| {
                                        input.set_value(value.clone(), window, cx)
                                    });
                                    d.applied_query = value;
                                    d.suggestions_open = false;
                                    d.candidate_limit = 100;
                                    d.acknowledged = false;
                                    cx.notify();
                                })),
                        );
                    }
                    if !options.is_empty() {
                        search = search.child(
                            gpui::deferred(
                                div()
                                    .absolute()
                                    .top(px(42.))
                                    .left_0()
                                    .right_0()
                                    .occlude()
                                    .child(suggestions),
                            )
                            .with_priority(30),
                        );
                    }
                }
                body = body.child(search).child(div().text_color(rgb(MUTED)).child(crate::i18n::text("Введите имена авторов, персонажей, названия произведений или теги через точку с запятой. Должны совпасть все значения. Нажмите Enter или выберите подсказку для поиска по всей библиотеке.")));
                let query = &draft.applied_query;
                let source_video = self.entries[draft.indices[0]].video;
                let candidates: Vec<_> =
                    crate::catalog::matching(&self.inactive_workspace.entries, query, false)
                        .into_iter()
                        .filter(|&i| self.inactive_workspace.entries[i].video == source_video)
                        .collect();
                let mut candidate_list = div().flex().flex_col().gap(px(6.));
                for &index in candidates.iter().take(draft.candidate_limit) {
                    let entry = &self.inactive_workspace.entries[index];
                    candidate_list = candidate_list.child(
                        operation_button(("replacement-candidate", index), "")
                            .justify_start()
                            .hover_colors(
                                Some(rgb(0x221d32).into()),
                                Some(rgb(ACCENT).into()),
                                None,
                            )
                            .w_full()
                            .h_auto()
                            .p(px(10.))
                            .rounded(px(8.))
                            .child(replacement_file(entry, self.show_previews).w_full())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.file_operation.as_mut().unwrap().target = Some(index);
                                cx.notify();
                            })),
                    );
                }
                body = body.child(candidate_list);
                if candidates.is_empty() {
                    body = body.child(crate::i18n::text("Совпадений в Main нет."));
                }
                if candidates.len() > draft.candidate_limit {
                    body = body.child(
                        operation_button(
                            "replacement-more",
                            crate::i18n::text("Загрузить ещё файлы"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.file_operation.as_mut().unwrap().candidate_limit += 100;
                            cx.notify();
                        })),
                    );
                }
            }
        } else {
            body = body.child(div().text_color(rgb(MUTED)).child(crate::i18n::text("Переместите файлы стрелками, чтобы изменить их индексы. Изменения применяются только после сохранения; файлы на диске не переименовываются.")));
            if draft.order.len() < 2 {
                body = body.child(crate::i18n::text(
                    "У файла нет совпадающих файлов для изменения порядка.",
                ));
            }
            let mut list = div()
                .id("file-order-list")
                .min_h_0()
                .overflow_y_scroll()
                .p(px(4.))
                .border_1()
                .border_color(rgb(LINE))
                .rounded(px(8.))
                .flex()
                .flex_col()
                .gap(px(4.));
            for (position, &index) in draft.order.iter().enumerate() {
                let total = draft.order.len();
                list = list.child(
                    div()
                        .id(("order-row", position))
                        .p(px(10.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, _, _, _| {
                                this.file_operation.as_mut().unwrap().dragging = Some(position);
                            }),
                        )
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                let d = this.file_operation.as_mut().unwrap();
                                if let Some(source) = d.dragging.take() {
                                    super::move_order(&mut d.order, source, position);
                                }
                                cx.notify();
                            }),
                        )
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .w(px(26.))
                                .text_color(rgb(ACCENT))
                                .child((position + 1).to_string()),
                        )
                        .child(
                            div()
                                .w(px(112.))
                                .h(px(64.))
                                .rounded(px(6.))
                                .overflow_hidden()
                                .flex_shrink_0()
                                .relative()
                                .child(self.picture_fitted(index, true))
                                .child(file_type_badge(&self.entries[index])),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(self.entries[index].details.filename.clone()),
                        )
                        .child(order_arrow(position, total, false, cx))
                        .child(order_arrow(position, total, true, cx)),
                );
            }
            body = body.child(list);
        }

        let enabled = delete
            || if replace {
                draft.target.is_some() && draft.acknowledged
            } else {
                draft.indices != draft.order
            };
        let save_label = if delete {
            if draft.indices.len() == 1 {
                crate::i18n::text("Удалить файл")
            } else {
                crate::i18n::text("Удалить файлы")
            }
        } else if replace {
            crate::i18n::text("Заменить файл")
        } else {
            crate::i18n::text("Сохранить порядок")
        };
        let panel = div()
            .id("file-operation-panel")
            .track_focus(&draft.focus)
            .occlude()
            .w(px(width))
            .max_h(px(f32::from(window.viewport_size().height) - 48.))
            .flex()
            .flex_col()
            .when(delete, |panel| panel.px(px(20.)).pt(px(6.)).pb(px(20.)))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(SURFACE))
            .when(delete, |panel| {
                panel.bg(gpui::linear_gradient(
                    130.,
                    gpui::linear_color_stop(rgb(0x1a1827), 0.),
                    gpui::linear_color_stop(rgb(SURFACE), 1.),
                ))
            })
            .shadow(vec![gpui::BoxShadow {
                color: rgba(0x00000066).into(),
                offset: gpui::point(px(0.), px(24.)),
                blur_radius: px(80.),
                spread_radius: px(0.),
            }])
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "up" | "down") {
                    let d = this.file_operation.as_ref().unwrap();
                    if d.suggestions_open {
                        let count = super::super::search_suggestions::matches(
                            &d.query.read(cx).value(),
                            &d.catalog.records,
                        )
                        .len();
                        let active = super::super::search_suggestions::next_index(
                            d.suggestion_active,
                            count,
                            event.keystroke.key == "down",
                        );
                        this.file_operation.as_mut().unwrap().suggestion_active = active;
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                }
                if event.keystroke.key == "escape" {
                    if this
                        .file_operation
                        .as_ref()
                        .is_some_and(|d| d.suggestions_open)
                    {
                        this.file_operation.as_mut().unwrap().suggestions_open = false;
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    this.close_file_operation(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .flex_shrink_0()
                    .p(px(if delete { 10. } else { 16. }))
                    .when(!delete, |h| h.py(px(9.)))
                    .when(delete, |header| {
                        header.px(px(0.)).child(
                            div()
                                .size(px(40.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .border_1()
                                .border_color(rgb(0x824254))
                                .rounded(px(10.))
                                .bg(rgba(0x38212854))
                                .text_color(rgb(0xf292a7))
                                .child(icon("trash").size(px(22.))),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(title),
                    )
                    .child(
                        operation_button("operation-close", "")
                            .w(px(30.))
                            .px(px(0.))
                            .child(icon("close").size(px(18.)))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_file_operation(window, cx)
                            })),
                    ),
            )
            .child(body)
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(10.))
                    .flex_shrink_0()
                    .when(delete, |footer| footer.pt(px(23.)))
                    .when(!delete, |footer| footer.py(px(16.)).px(px(18.)))
                    .when(replace, |footer| {
                        footer.border_t_1().border_color(rgb(LINE))
                    })
                    .when(!delete && !replace, |footer| {
                        footer.child(div().flex_1().text_color(rgb(MUTED)).child(
                            if crate::i18n::english() {
                                format!(
                                    "{} files{}",
                                    draft.order.len(),
                                    if draft.indices != draft.order {
                                        crate::i18n::text(" · Порядок изменён")
                                    } else {
                                        ""
                                    },
                                )
                            } else {
                                format!(
                                    "{} файлов{}",
                                    draft.order.len(),
                                    if draft.indices != draft.order {
                                        crate::i18n::text(" · Порядок изменён")
                                    } else {
                                        ""
                                    },
                                )
                            },
                        ))
                    })
                    .child(
                        operation_button("operation-cancel", crate::i18n::text("Отмена")).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.close_file_operation(window, cx)
                            }),
                        ),
                    )
                    .child(
                        operation_button("operation-save", save_label)
                            .min_w(px(128.))
                            .bg(motion::Surface::gradient(
                                120.,
                                rgb(if delete { 0xa53d5a } else { 0x7b43d9 }),
                                rgb(if delete { 0x85324b } else { 0x6839ce }),
                            ))
                            .text_color(rgb(0xffffff))
                            .opacity(if enabled { 1. } else { 0.45 })
                            .when(enabled, |view| {
                                view.hover_colors(
                                    Some(rgb(if delete { 0xb74766 } else { 0x8853e1 }).into()),
                                    None,
                                    None,
                                )
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if enabled {
                                    this.save_file_operation(window, cx);
                                }
                            })),
                    ),
            );

        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x05040cab))
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .line_height(px(19.2))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_file_operation(window, cx)),
            )
            .when_some(draft.backdrop.clone(), |d, image| {
                d.child(
                    gpui::img(image)
                        .absolute()
                        .inset_0()
                        .size_full()
                        .object_fit(gpui::ObjectFit::Fill),
                )
            })
            .child(panel)
    }

    fn operation_file_row(&self, index: usize) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .p(px(9.))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(8.))
            .bg(rgb(0x10101a))
            .child(
                div()
                    .w(px(96.))
                    .h(px(64.))
                    .flex_shrink_0()
                    .relative()
                    .child(self.picture_fitted(index, true))
                    .child(file_type_badge(&self.entries[index])),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(self.entries[index].details.filename.clone()),
            )
    }
}

fn order_arrow(
    position: usize,
    total: usize,
    down: bool,
    cx: &mut Context<Gallery>,
) -> impl IntoElement {
    let enabled = if down {
        position + 1 < total
    } else {
        position > 0
    };
    operation_button(
        (if down { "order-down" } else { "order-up" }, position),
        if down { "↓" } else { "↑" },
    )
    .opacity(if enabled { 1. } else { 0.45 })
    .on_click(cx.listener(move |this, _, _, cx| {
        if enabled {
            let d = this.file_operation.as_mut().unwrap();
            super::move_order(
                &mut d.order,
                position,
                if down { position + 1 } else { position - 1 },
            );
            cx.notify();
        }
    }))
}

fn version_panel(
    title: &str,
    entry: &crate::catalog::Entry,
    previews: bool,
    main: bool,
    cx: &mut Context<Gallery>,
) -> Div {
    div()
        .p(px(14.))
        .border_1()
        .border_color(rgb(LINE))
        .rounded(px(8.))
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(
            div()
                .text_color(rgb(ACCENT))
                .text_size(px(13.))
                .font_weight(gpui::FontWeight::BOLD)
                .child(title.to_owned()),
        )
        .child(
            replacement_file(entry, previews).child(
                operation_button(
                    if main {
                        "preview-main-version"
                    } else {
                        "preview-new-version"
                    },
                    "",
                )
                .child(icon("arrow").size(px(14.)))
                .child(crate::i18n::text("Открыть файл"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.preview_replacement(main, window, cx)
                })),
            ),
        )
}

fn replacement_file(entry: &crate::catalog::Entry, previews: bool) -> Div {
    let resolution = entry
        .details
        .resolution
        .map(|(w, h)| format!("{w} × {h}"))
        .unwrap_or_else(|| crate::i18n::text("Разрешение недоступно").into());
    let size = entry
        .details
        .size
        .map(|bytes| format!("{:.2} MB", bytes as f64 / 1024. / 1024.))
        .unwrap_or_else(|| crate::i18n::text("Размер недоступен").into());
    let mut description = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .font_weight(gpui::FontWeight::BOLD)
                .text_size(px(13.))
                .child(entry.details.filename.clone()),
        );
    if let Some(artist) = &entry.details.artist {
        description = description.child(div().text_color(rgb(MUTED)).child(artist.clone()));
    }
    let technical = format!(
        "{resolution} · {size} · {}{}",
        entry.extension,
        if entry.video {
            format!(
                " · {}",
                entry
                    .duration
                    .as_deref()
                    .unwrap_or(crate::i18n::text("Длительность недоступна"))
            )
        } else {
            String::new()
        }
    );
    description = description.child(div().text_color(rgb(MUTED)).child(technical));

    div()
        .flex()
        .items_center()
        .min_w_0()
        .gap(px(12.))
        .child(
            div()
                .w(px(112.))
                .h(px(72.))
                .flex_shrink_0()
                .rounded(px(6.))
                .overflow_hidden()
                .relative()
                .bg(rgb(0x10101a))
                .when(previews, |d| {
                    d.child(
                        gpui::img(entry.path.clone())
                            .size_full()
                            .object_fit(gpui::ObjectFit::Contain),
                    )
                })
                .child(file_type_badge(entry)),
        )
        .child(description)
}

fn file_type_badge(entry: &crate::catalog::Entry) -> Div {
    div()
        .absolute()
        .top(px(5.))
        .left(px(5.))
        .px(px(4.))
        .py(px(2.))
        .rounded(px(4.))
        .bg(rgba(0x080910cc))
        .text_color(rgb(0xffffff))
        .text_size(px(9.))
        .line_height(px(14.))
        .child(entry.extension.to_uppercase())
}
