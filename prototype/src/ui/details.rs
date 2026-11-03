use gpui::{ClipboardItem, Context, Div, SharedString, Stateful, div, prelude::*, px, rgb};

use super::{
    Gallery, LINE, MUTED, SURFACE,
    shell::{control, icon},
};

impl Gallery {
    pub(super) fn details(&self, cx: &mut Context<Self>) -> Div {
        let panel = div()
            .flex()
            .flex_col()
            .h_full()
            .w_full()
            .bg(rgb(SURFACE))
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(LINE))
            .shadow_lg()
            .overflow_hidden()
            .occlude()
            .child(
                div()
                    .h(px(52.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .pl(px(16.0))
                    .pr(px(10.0))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Информация о файле"),
                    )
                    .child(
                        control("close-details", "", "close", false)
                            .size(px(28.0))
                            .px_1()
                            .py_1()
                            .border_0()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.details_open = false;
                                cx.notify();
                            })),
                    ),
            );
        let Some(index) = self.selected else {
            return panel.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p_6()
                    .gap_3()
                    .text_color(rgb(MUTED))
                    .child(icon("info").size(px(32.0)))
                    .child("Выберите файл")
                    .child("Здесь появится информация о выбранном файле"),
            );
        };
        let entry = &self.entries[index];
        let artist: Vec<_> = entry.details.artist.iter().cloned().collect();
        let characters: Vec<_> = entry
            .characters
            .split(", ")
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();
        let mut fields = vec![
            (
                "Тип файла",
                format!(
                    "{} ({})",
                    entry.extension,
                    if entry.video {
                        "Видео"
                    } else {
                        "Изображение"
                    }
                ),
            ),
            (
                "Размер",
                entry
                    .details
                    .size
                    .map(format_size)
                    .unwrap_or_else(|| "Недоступно".into()),
            ),
            (
                "Разрешение",
                entry
                    .details
                    .resolution
                    .map(|(width, height)| format!("{width} × {height}"))
                    .unwrap_or_else(|| "Недоступно".into()),
            ),
        ];
        if entry.video {
            fields.push((
                "Длительность",
                super::layout::full_duration(entry.duration.as_deref()),
            ));
            fields.push((
                "FPS",
                if entry.demo {
                    "29,97"
                } else {
                    "Недоступно"
                }
                .into(),
            ));
        }
        fields.push((
            "Изменён",
            entry
                .date_label
                .clone()
                .unwrap_or_else(|| "Недоступно".into()),
        ));
        fields.push((
            "Создан",
            entry
                .details
                .created_label
                .clone()
                .unwrap_or_else(|| "Недоступно".into()),
        ));
        if entry.video {
            fields.push((
                "Кодек",
                if entry.demo {
                    "H.264 / AVC"
                } else {
                    "Недоступно"
                }
                .into(),
            ));
            fields.push((
                "Битрейт",
                if entry.demo {
                    "50,20 Mbit/s"
                } else {
                    "Недоступно"
                }
                .into(),
            ));
        }

        panel.child(
            div()
                .id("details-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(15.0))
                .pr(px(21.0))
                .pb(px(14.0))
                .child(
                    div()
                        .my(px(14.0))
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(entry.details.filename.clone()),
                )
                .when(self.show_previews, |view| {
                    view.child(
                        div()
                            .relative()
                            .h(px(201.25))
                            .w_full()
                            .rounded(px(6.0))
                            .overflow_hidden()
                            .border_1()
                            .border_color(rgb(LINE))
                            .child(self.picture_fitted(index, true))
                            .child(preview_badge(
                                if entry.video {
                                    "Предпросмотр видео"
                                } else {
                                    "Предпросмотр изображения"
                                },
                                false,
                            ))
                            .when_some(entry.duration.clone(), |view, duration| {
                                view.child(preview_badge(&duration, true))
                            }),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .pt(px(20.0))
                        .pb(px(16.0))
                        .border_b_1()
                        .border_color(rgb(LINE))
                        .child(self.metadata_group(
                            "copyrights",
                            "Произведение",
                            &entry.details.copyrights,
                            0x382128,
                            0xf0c2cd,
                            cx,
                        ))
                        .child(self.metadata_group(
                            "artist",
                            "Автор",
                            &artist,
                            0x262544,
                            0xd6d1ff,
                            cx,
                        ))
                        .child(self.metadata_group(
                            "characters",
                            "Персонажи",
                            &characters,
                            0x203b32,
                            0xb0dfc7,
                            cx,
                        ))
                        .child(self.metadata_group(
                            "tags",
                            "Теги",
                            &entry.tags,
                            0x30263e,
                            0xd0b4fa,
                            cx,
                        )),
                )
                .child(
                    div()
                        .mt(px(10.0))
                        .mb(px(17.0))
                        .children(fields.into_iter().map(|(label, value)| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .py(px(10.0))
                                .text_size(px(11.0))
                                .line_height(px(16.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .flex_1()
                                        .gap(px(10.0))
                                        .text_color(rgb(MUTED))
                                        .child(
                                            icon(match label {
                                                "Размер" | "Тип файла" => "files",
                                                "Разрешение" => "scan",
                                                "Длительность" => "clock",
                                                "Изменён" | "Создан" => "calendar",
                                                _ => "film",
                                            })
                                            .size(px(16.0))
                                            .flex_shrink_0(),
                                        )
                                        .child(label),
                                )
                                .child(div().min_w_0().text_right().child(value))
                        })),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(7.0))
                        .child(
                            control("open-detail", "Открыть файл", "arrow", true)
                                .justify_center()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.selected = Some(index);
                                    this.open_selected(window, cx);
                                })),
                        )
                        .child(
                            control("reveal-detail", "Открыть папку", "folder", false)
                                .justify_center()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    tracing::info!(
                                        operation = "reveal_file",
                                        "File location reveal requested"
                                    );
                                    cx.reveal_path(&this.entries[index].path);
                                })),
                        )
                        .child(
                            control("copy-detail", "Копировать путь", "copy", false)
                                .justify_center()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        this.entries[index].path.to_string_lossy().into_owned(),
                                    ));
                                    tracing::debug!(
                                        operation = "copy_path",
                                        "Selected path copied"
                                    );
                                })),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(6.0))
                                .child(
                                    control("detail-favorite", "", "heart", entry.favorite)
                                        .flex_1()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.entries[index].favorite =
                                                !this.entries[index].favorite;
                                            tracing::debug!(
                                                operation = "local_favorite",
                                                "Session favorite toggled"
                                            );
                                            this.refilter(cx);
                                        })),
                                )
                                .child(control("detail-tags", "", "tag", false).flex_1().on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.unavailable("Редактор тегов", cx)
                                    }),
                                ))
                                .child(
                                    control("detail-more", "", "more", false).flex_1().on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.unavailable("Расширенные операции", cx)
                                        }),
                                    ),
                                ),
                        ),
                )
                .when(entry.demo, |view| {
                    view.child(
                        div()
                            .mt_3()
                            .text_size(px(10.0))
                            .text_color(rgb(MUTED))
                            .child("Демонстрационные данные"),
                    )
                }),
        )
    }

    fn metadata_group(
        &self,
        id: &'static str,
        label: &str,
        values: &[String],
        background: u32,
        foreground: u32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(14.0))
                    .text_color(rgb(MUTED))
                    .child(label.to_owned()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(5.0))
                    .children(values.iter().enumerate().map(|(index, value)| {
                        let search = value.clone();
                        div()
                            .id(index)
                            .rounded(px(12.0))
                            .px(px(7.0))
                            .py(px(3.0))
                            .border_1()
                            .border_color(rgb(match id {
                                "copyrights" => 0x71333d,
                                "artist" => 0x403e80,
                                "characters" => 0x285b4a,
                                _ => 0x514361,
                            }))
                            .bg(rgb(background))
                            .text_color(rgb(foreground))
                            .text_size(px(11.0))
                            .line_height(px(14.0))
                            .cursor_pointer()
                            .child(SharedString::from(value.clone()))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_search(&search, window, cx);
                            }))
                    }))
                    .when(values.is_empty(), |view| {
                        view.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(MUTED))
                                .child("Недоступно"),
                        )
                    }),
            )
    }
}

fn preview_badge(label: &str, right: bool) -> Div {
    div()
        .absolute()
        .bottom(px(7.0))
        .when(right, |view| view.right(px(7.0)))
        .when(!right, |view| view.left(px(7.0)))
        .px(px(5.0))
        .py(px(3.0))
        .rounded(px(4.0))
        .bg(gpui::rgba(0x0a0a12c2))
        .text_color(rgb(0xffffff))
        .text_size(px(9.0))
        .child(label.to_owned())
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} ГБ", bytes as f64 / 1024.0_f64.powi(3))
    } else if bytes >= 1024 * 1024 {
        format!("{:.2} МБ", bytes as f64 / 1024.0_f64.powi(2))
    } else {
        format!("{:.2} КБ", bytes as f64 / 1024.0)
    }
}
