use gpui::{
    BoxShadow, ClipboardItem, Context, Div, SharedString, Stateful, Window, div, point, prelude::*,
    px,
};
use gpui_component::scroll::{Scrollbar, ScrollbarShow};

use super::{Gallery, LINE, MUTED, SURFACE, TEXT, shell::icon};
use crate::appearance::{rgb, rgba};

mod scrolling;

pub(super) use scrolling::ScrollMotion;

impl Gallery {
    pub(super) fn details(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let chip_size = if f32::from(window.viewport_size().width) >= 1450.0 {
            11.0
        } else {
            10.0
        };
        let panel = div()
            .flex()
            .flex_col()
            .h_full()
            .w_full()
            .bg(gpui::linear_gradient(
                120.0,
                gpui::linear_color_stop(rgb(SURFACE), 0.0),
                gpui::linear_color_stop(rgb(0x10101a), 1.0),
            ))
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(LINE))
            .shadow(vec![BoxShadow {
                color: rgba(0x00000080).into(),
                offset: point(px(-12.0), px(0.0)),
                blur_radius: px(36.0),
                spread_radius: px(0.0),
            }])
            .overflow_hidden()
            .occlude()
            .child(
                div()
                    .h(px(56.0))
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
                            .child(crate::i18n::text("Информация о файле")),
                    )
                    .child(
                        super::motion::button("close-details")
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.0))
                            .text_color(rgb(MUTED))
                            .cursor_pointer()
                            .hover_colors(
                                Some(rgba(0xa786ff1a).into()),
                                Some(rgb(TEXT).into()),
                                None,
                            )
                            .child(icon("close").size(px(18.0)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.details_open = false;
                                cx.notify();
                            })),
                    ),
            );
        let selected = if self.stage && self.stage_selection.len() != 1 {
            None
        } else {
            self.selected
        };
        let Some(index) = selected else {
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
                    .text_center()
                    .child(icon("info").size(px(32.0)))
                    .child(
                        div()
                            .text_color(rgb(TEXT))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(if self.stage && self.stage_selection.len() > 1 {
                                crate::i18n::text("Выбрано несколько файлов")
                            } else {
                                crate::i18n::text("Файл не выбран")
                            }),
                    )
                    .child(crate::i18n::text(
                        "Выберите один файл, чтобы просмотреть информацию.",
                    )),
            );
        };
        let entry = &self.entries[index];
        let entry_favorite = entry.favorite;
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
                        crate::i18n::text("Видео")
                    } else {
                        crate::i18n::text("Изображение")
                    }
                ),
            ),
            (
                "Размер",
                entry
                    .details
                    .size
                    .map(format_size)
                    .unwrap_or_else(|| crate::i18n::text("Недоступно").into()),
            ),
            (
                "Разрешение",
                entry
                    .details
                    .resolution
                    .map(|(width, height)| format!("{width} × {height}"))
                    .unwrap_or_else(|| crate::i18n::text("Недоступно").into()),
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
                    crate::i18n::text("Недоступно")
                }
                .into(),
            ));
        }
        fields.push((
            "Изменён",
            entry
                .date_label
                .clone()
                .unwrap_or_else(|| crate::i18n::text("Недоступно").into()),
        ));
        fields.push((
            "Создан",
            entry
                .details
                .created_label
                .clone()
                .unwrap_or_else(|| crate::i18n::text("Недоступно").into()),
        ));
        if entry.video {
            fields.push((
                "Кодек",
                if entry.demo {
                    "H.264 / AVC"
                } else {
                    crate::i18n::text("Недоступно")
                }
                .into(),
            ));
            fields.push((
                "Битрейт",
                if entry.demo {
                    crate::i18n::text("50.20 Мбит/с")
                } else {
                    crate::i18n::text("Недоступно")
                }
                .into(),
            ));
        }

        let content = div()
            .id("details-scroll-content")
            .on_scroll_wheel(
                cx.listener(|this, event: &gpui::ScrollWheelEvent, window, cx| {
                    this.scroll_details(event, window, cx);
                }),
            )
            .flex()
            .flex_col()
            .w_full()
            .px(px(15.0))
            .pr(px(21.0))
            .pb(px(14.0))
            .child(
                div()
                    .my(px(14.0))
                    .text_size(px(13.0))
                    .line_height(px(16.0))
                    .whitespace_normal()
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
                                crate::i18n::text("Предпросмотр видео")
                            } else {
                                crate::i18n::text("Предпросмотр изображения")
                            },
                            false,
                        ))
                        .when_some(entry.duration.clone(), |view, duration| {
                            view.child(preview_badge(&duration, true))
                        }),
                )
            })
            .when(!self.stage, |view| {
                view.child(
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
                            crate::i18n::text("Произведение"),
                            &entry.details.copyrights,
                            (0x382128, 0xf0c2cd, chip_size),
                            cx,
                        ))
                        .child(self.metadata_group(
                            "artist",
                            crate::i18n::text("Автор"),
                            &artist,
                            (0x262544, 0xd6d1ff, chip_size),
                            cx,
                        ))
                        .child(self.metadata_group(
                            "characters",
                            crate::i18n::text("Персонажи"),
                            &characters,
                            (0x203b32, 0xb0dfc7, chip_size),
                            cx,
                        ))
                        .child(self.metadata_group(
                            "tags",
                            crate::i18n::text("Теги"),
                            &entry.tags,
                            (0x2c2644, 0xd0b4fa, chip_size),
                            cx,
                        )),
                )
            })
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
                                    .child(crate::i18n::text(label)),
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
                        detail_button(
                            "open-detail",
                            crate::i18n::text("Открыть файл"),
                            "arrow",
                            true,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.selected = Some(index);
                                this.open_selected(window, cx);
                            },
                        )),
                    )
                    .child(
                        detail_button(
                            "reveal-detail",
                            crate::i18n::text("Открыть папку"),
                            "folder",
                            false,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            tracing::info!(
                                operation = "reveal_file",
                                "File location reveal requested"
                            );
                            cx.reveal_path(&this.entries[index].path);
                        })),
                    )
                    .when(!self.stage, |view| {
                        view.child(
                            detail_button(
                                "copy-detail",
                                crate::i18n::text("Копировать путь"),
                                "copy",
                                false,
                            )
                            .h(px(34.0))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        this.entries[index].path.to_string_lossy().into_owned(),
                                    ));
                                    tracing::debug!(
                                        operation = "copy_path",
                                        "Selected path copied"
                                    );
                                },
                            )),
                        )
                    })
                    .when(self.stage, |view| {
                        view.child(self.distribution_button(
                            "detail-distribute",
                            crate::i18n::text("Распределить"),
                            cx,
                        ))
                    })
                    .when(!self.stage, |view| {
                        view.child(
                            div()
                                .flex()
                                .gap(px(6.0))
                                .child(
                                    detail_button("detail-favorite", "", "heart", false)
                                        .flex_1()
                                        .flex_basis(px(0.0))
                                        .when(entry.favorite, |view| view.text_color(rgb(0xd0b4fa)))
                                        .tooltip(move |window, cx| {
                                            gpui_component::tooltip::Tooltip::new(
                                                if entry_favorite {
                                                    crate::i18n::text("Убрать из избранного")
                                                } else {
                                                    crate::i18n::text("Добавить в избранное")
                                                },
                                            )
                                            .build(window, cx)
                                        })
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
                                .child(
                                    detail_button("detail-share", "", "share", false)
                                        .flex_1()
                                        .flex_basis(px(0.0))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.run_file_action(
                                                index,
                                                super::file_operations::Action::Share,
                                                window,
                                                cx,
                                            )
                                        })),
                                )
                                .child(
                                    detail_button("detail-tags", "", "tag", false)
                                        .flex_1()
                                        .flex_basis(px(0.0))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_file_tags(index, window, cx)
                                        })),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .flex_basis(px(0.))
                                        .child(self.file_actions(index, true, cx)),
                                ),
                        )
                    }),
            );

        panel.child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, _| {
                        this.details_motion = None;
                    }),
                )
                .child(
                    div()
                        .id("details-scroll")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.details_scroll)
                        .child(content),
                )
                .child(
                    div().absolute().top_0().left_0().size_full().child(
                        Scrollbar::vertical(&self.details_scroll)
                            .scrollbar_show(ScrollbarShow::Scrolling),
                    ),
                ),
        )
    }

    fn metadata_group(
        &self,
        id: &'static str,
        label: &str,
        values: &[String],
        style: (u32, u32, f32),
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let (background, foreground, chip_size) = style;

        div()
            .id(id)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
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
                        let brighten = |shift: u32| {
                            (((background >> shift) & 0xff) as f32 * 1.3).min(255.0) as u32
                        };
                        let hover_background =
                            (brighten(16) << 16) | (brighten(8) << 8) | brighten(0);

                        super::motion::button(index)
                            .min_w_0()
                            .max_w_full()
                            .whitespace_normal()
                            .rounded(px(12.0))
                            .px(px(7.0))
                            .py(px(3.0))
                            .border_1()
                            .border_color(rgb(match id {
                                "copyrights" => 0x71333d,
                                "artist" => 0x403e80,
                                "characters" => 0x285b4a,
                                _ => 0x6d5aa4,
                            }))
                            .bg(rgb(background))
                            .text_color(rgb(foreground))
                            .text_size(px(chip_size))
                            .line_height(px(14.0))
                            .cursor_pointer()
                            .hover_colors(Some(rgb(hover_background).into()), None, None)
                            .child(SharedString::from(value.clone()))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_search(&search, window, cx);
                            }))
                    }))
                    .when(values.is_empty(), |view| {
                        view.child(div().text_size(px(12.0)).text_color(rgb(MUTED)).child(
                            if id == "tags" {
                                crate::i18n::text("Тегов пока нет")
                            } else {
                                crate::i18n::text("Недоступно")
                            },
                        ))
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
        .bg(crate::appearance::rgba(0x0a0a12c2))
        .text_color(rgb(0xffffff))
        .text_color(rgb(0xffffff))
        .text_size(px(if right { 10.0 } else { 9.0 }))
        .line_height(px(if right { 12.0 } else { 11.0 }))
        .child(label.to_owned())
}

pub(super) fn detail_button(
    id: &'static str,
    label: &str,
    name: &'static str,
    primary: bool,
) -> super::motion::MotionDiv {
    let icon_only = label.is_empty();
    let tooltip = if icon_only {
        match name {
            "share" => crate::i18n::text("Поделиться файлом"),
            "tag" => crate::i18n::text("Редактировать теги"),
            "more" => crate::i18n::text("Другие действия"),
            _ => "",
        }
    } else {
        ""
    };

    super::motion::button(id)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .h(px(if icon_only { 34.0 } else { 35.0 }))
        .px(px(if icon_only { 0.0 } else { 12.0 }))
        .rounded(px(6.0))
        .border_1()
        .border_color(if primary { rgba(0) } else { rgb(LINE) })
        .bg(rgb(0x1a1827))
        .when(primary, |view| {
            view.bg(super::motion::Surface::gradient(
                120.0,
                rgb(0x7b43d9),
                rgb(0x6839ce),
            ))
        })
        .text_color(rgb(if primary { 0xffffff } else { TEXT }))
        .text_size(px(11.0))
        .line_height(px(13.75))
        .font_weight(gpui::FontWeight::MEDIUM)
        .cursor_pointer()
        .hover_colors(
            Some(rgb(if primary { 0x8853e1 } else { 0x242034 }).into()),
            None,
            Some(if primary {
                rgba(0).into()
            } else {
                rgb(0x6d5aa4).into()
            }),
        )
        .child(icon(name).size(px(if icon_only {
            18.0
        } else if name == "copy" {
            16.0
        } else {
            17.0
        })))
        .when(!icon_only, |view| view.child(label.to_owned()))
        .when(!tooltip.is_empty(), |view| {
            view.tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip).build(window, cx)
            })
        })
}

pub(super) fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        if crate::i18n::english() {
            format!("{:.2} GB", bytes as f64 / 1024.0_f64.powi(3))
        } else {
            format!("{:.2} ГБ", bytes as f64 / 1024.0_f64.powi(3))
        }
    } else {
        if crate::i18n::english() {
            format!("{:.2} MB", bytes as f64 / 1024.0_f64.powi(2))
        } else {
            format!("{:.2} МБ", bytes as f64 / 1024.0_f64.powi(2))
        }
    }
}

#[cfg(test)]
mod tests;
