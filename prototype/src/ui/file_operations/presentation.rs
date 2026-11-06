use gpui::{Context, Div, Window, div, prelude::*, px, rgb, rgba};
use gpui_component::input::Input;

use super::{Action, Gallery};
use crate::ui::{ACCENT, LINE, MUTED, SURFACE, TEXT, motion, shell::icon};

fn operation_button(id: impl Into<gpui::ElementId>, label: &str) -> motion::MotionDiv {
    super::super::button(id, label)
        .keyboard_focus(true)
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
            Action::Delete if draft.indices.len() == 1 => "Удалить файл?".into(),
            Action::Delete => format!("Удалить {} файлов?", draft.indices.len()),
            Action::Replace => "Заменить файл в Main Library".into(),
            _ => "Изменить порядок файлов".into(),
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
            .gap(px(if delete { 8. } else { 14. }));
        if delete {
            body = body.child(
                div()
                    .text_color(rgb(MUTED))
                    .child("Файлы будут удалены из Nexa в текущей сессии."),
            );
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
                body = body.child(format!("И ещё {} файл(ов).", draft.indices.len() - 5));
            }
            body = body.child(div().p_3().border_1().border_color(rgb(0x824254)).rounded(px(8.)).text_color(rgb(0xf2a0b1)).child("Изолированный прототип: оригиналы на диске сохраняются. Корзина и удаление из рабочего каталога ещё не подключены."));
        } else if replace {
            body = body.child(version_panel(
                "Новая версия · Incoming",
                &self.entries[draft.indices[0]],
                self.show_previews,
            ));
            if let Some(target) = draft.target {
                let main = &self.inactive_workspace.entries[target];
                body = body.child(version_panel("Текущая версия · Main", main, self.show_previews))
                    .child(operation_button("replacement-another", "Выбрать другой файл").on_click(cx.listener(|this, _, _, cx| {
                        let d = this.file_operation.as_mut().unwrap();
                        d.target = None;
                        d.acknowledged = false;
                        cx.notify();
                    })))
                    .child("Main сохраняет автора, копирайты, персонажей, теги, избранное, название видео и порядок. Новая версия заменяет содержимое, формат и технические сведения.")
                    .child(div().text_color(rgb(MUTED)).child("Замена действует только в сессии. Оригиналы и аннотации на диске не изменяются."))
                    .child(operation_button("replacement-ack", if draft.acknowledged { "☑ Заменить выбранной версией Incoming" } else { "☐ Заменить выбранной версией Incoming" }).on_click(cx.listener(|this, _, _, cx| {
                        let d = this.file_operation.as_mut().unwrap();
                        d.acknowledged = !d.acknowledged;
                        cx.notify();
                    })));
            } else {
                body = body
                    .child("Выберите файл в Main Library")
                    .child(Input::new(&draft.query));
                let query = draft.query.read(cx).value();
                let candidates =
                    crate::catalog::matching(&self.inactive_workspace.entries, &query, false);
                for &index in candidates.iter().take(draft.candidate_limit) {
                    let entry = &self.inactive_workspace.entries[index];
                    body = body.child(
                        operation_button(("replacement-candidate", index), "")
                            .w_full()
                            .h_auto()
                            .p(px(10.))
                            .rounded(px(8.))
                            .child(replacement_file(entry, self.show_previews))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.file_operation.as_mut().unwrap().target = Some(index);
                                cx.notify();
                            })),
                    );
                }
                if candidates.is_empty() {
                    body = body.child("Совпадений в Main нет.");
                }
                if candidates.len() > draft.candidate_limit {
                    body = body.child(
                        operation_button("replacement-more", "Загрузить ещё файлы").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.file_operation.as_mut().unwrap().candidate_limit += 100;
                                cx.notify();
                            }),
                        ),
                    );
                }
            }
        } else {
            body = body.child(div().text_color(rgb(MUTED)).child("Переместите файлы стрелками, чтобы изменить их индексы. Изменения применяются только после сохранения; файлы на диске не переименовываются."));
            if draft.order.len() < 2 {
                body = body.child("У файла нет совпадающих файлов для изменения порядка.");
            }
            for (position, &index) in draft.order.iter().enumerate() {
                let total = draft.order.len();
                body = body.child(
                    div()
                        .id(("order-row", position))
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
                        .child(div().flex_1().child(self.operation_file_row(index)))
                        .child(order_arrow(position, total, false, cx))
                        .child(order_arrow(position, total, true, cx)),
                );
            }
        }
        let enabled = delete
            || if replace {
                draft.target.is_some() && draft.acknowledged
            } else {
                draft.indices != draft.order
            };
        let save_label = if delete {
            "Удалить файлы"
        } else if replace {
            "Заменить файл"
        } else {
            "Сохранить порядок"
        };
        let panel = div()
            .id("file-operation-panel")
            .track_focus(&draft.focus)
            .occlude()
            .w(px(width))
            .max_h(px(f32::from(window.viewport_size().height) - 48.))
            .flex()
            .flex_col()
            .p(px(18.))
            .gap(px(16.))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(SURFACE))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.close_file_operation(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().text_size(px(16.)).child(title))
                    .child(
                        operation_button("operation-close", "")
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
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .pt(px(16.))
                    .child(operation_button("operation-cancel", "Отмена").on_click(
                        cx.listener(|this, _, window, cx| this.close_file_operation(window, cx)),
                    ))
                    .child(
                        operation_button("operation-save", save_label)
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
                    .child(self.picture_fitted(index, true)),
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

fn version_panel(title: &str, entry: &crate::catalog::Entry, previews: bool) -> Div {
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
                .child(title.to_owned()),
        )
        .child(replacement_file(entry, previews))
}

fn replacement_file(entry: &crate::catalog::Entry, previews: bool) -> Div {
    let resolution = entry
        .details
        .resolution
        .map(|(w, h)| format!("{w} × {h}"))
        .unwrap_or_else(|| "Разрешение недоступно".into());
    let size = entry
        .details
        .size
        .map(|bytes| format!("{:.1} MB", bytes as f64 / 1024. / 1024.))
        .unwrap_or_else(|| "Размер недоступен".into());
    let mut description = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .font_weight(gpui::FontWeight::BOLD)
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
                    .unwrap_or("Длительность недоступна")
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
                .bg(rgb(0x10101a))
                .when(previews, |d| {
                    d.child(
                        gpui::img(entry.path.clone())
                            .size_full()
                            .object_fit(gpui::ObjectFit::Contain),
                    )
                }),
        )
        .child(description)
}
