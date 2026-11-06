use gpui::{Context, Div, Focusable, FontWeight, MouseButton, Window, div, prelude::*, px};
use gpui_component::input::Input;

use super::{Gallery, Panel, presentation::sized_icon_button};
use crate::appearance::rgb;
use crate::ui::{LINE, MUTED, TEXT, shell::icon};

impl Gallery {
    fn viewer_tags_panel(&self, index: usize, cx: &mut Context<Self>) -> Div {
        let entry = &self.entries[index];
        let tags = div()
            .flex()
            .flex_wrap()
            .gap(px(8.0))
            .when(entry.tags.is_empty(), |view| {
                view.child(
                    div()
                        .text_color(rgb(MUTED))
                        .child(crate::i18n::text("Без тегов")),
                )
            })
            .children(entry.tags.iter().enumerate().map(|(tag_index, tag)| {
                crate::ui::motion::button(("viewer-tag", tag_index))
                    .keyboard_focus(true)
                    .px(px(7.0))
                    .py(px(3.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(12.0))
                    .bg(rgb(0x2c263f))
                    .text_color(rgb(0xd0b4fa))
                    .text_size(px(10.0))
                    .line_height(px(14.0))
                    .cursor_pointer()
                    .child(tag.clone())
                    .child(icon("close").size(px(12.0)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.entries[index].tags.remove(tag_index);
                        this.tag_counts = crate::catalog::tag_counts(&this.entries);
                        tracing::info!(
                            operation = "viewer_annotations",
                            scope = "session",
                            "Viewer tag removed"
                        );
                        cx.notify();
                    }))
            }));
        let has_options = self
            .tag_counts
            .iter()
            .any(|(tag, _)| !entry.tags.contains(tag));
        let input = Input::new(&self.viewer_state.tag_input)
            .appearance(false)
            .h(px(38.0))
            .px(px(12.0))
            .py_0()
            .text_size(px(12.0))
            .when(has_options, |input| {
                input.prefix(icon("search").size(px(15.0)).text_color(rgb(MUTED)))
            });
        let field = div()
            .id("viewer-tag-field")
            .relative()
            .track_focus(&self.viewer_state.tag_input.focus_handle(cx))
            .flex_1()
            .min_w_0()
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(0x10101a))
            .focus(|style| style.border_color(rgb(crate::ui::ACCENT)))
            .child(input)
            .child({
                let gallery = cx.weak_entity();
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, (), _, cx| {
                        let _ = gallery.update(cx, |this, _| this.viewer_state.tag_bounds = bounds);
                    },
                )
                .absolute()
                .size_full()
            });
        let can_add = !self
            .viewer_state
            .tag_input
            .read(cx)
            .value()
            .trim()
            .is_empty();
        let add_button = sized_icon_button("viewer-tag-add", "plus", false, 18.0)
            .w(px(38.0))
            .h(px(40.0))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(0x1a1827))
            .when(!can_add, |view| {
                view.opacity(0.4).cursor_default().keyboard_focus(false)
            })
            .on_click(cx.listener(|this, _, window, cx| this.add_viewer_tag(window, cx)));
        let form = div()
            .mt(px(18.0))
            .flex()
            .gap(px(8.0))
            .child(field)
            .child(add_button);

        div().flex().flex_col().child(tags).child(form)
            .when_some(self.viewer_state.save_error.clone(), |view, error| view.child(div().mt(px(12.0)).text_color(rgb(0xf292a7)).child(error)))
            .child(note(crate::i18n::text("В демо изменения сохраняются только на время сеанса. Фото отображаются из демонстрационных превью.")))
    }

    pub(super) fn viewer_tag_matches(&self, cx: &Context<Self>) -> Vec<String> {
        let query = self
            .viewer_state
            .tag_input
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        let Some(index) = self.selected.filter(|_| !query.is_empty()) else {
            return Vec::new();
        };
        let selected = &self.entries[index].tags;
        let mut matches: Vec<_> = self
            .tag_counts
            .iter()
            .filter_map(|(tag, _)| {
                let name = tag.to_lowercase();
                let position = name.find(&query)?;
                (!selected
                    .iter()
                    .any(|selected| selected.to_lowercase() == name))
                .then(|| (position != 0, tag.clone()))
            })
            .collect();
        matches.sort_by_key(|(infix, _)| *infix);
        matches.into_iter().take(30).map(|(_, name)| name).collect()
    }

    pub(super) fn viewer_tag_suggestions(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let bounds = self.viewer_state.tag_bounds;
        div()
            .id("viewer-tag-suggestions")
            .absolute()
            .left(bounds.origin.x)
            .top(bounds.bottom() - px(60.0) + px(4.0))
            .w(bounds.size.width)
            .max_h(px(220.0))
            .overflow_y_scroll()
            .p(px(5.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(0x1a1827))
            .occlude()
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when(self.viewer_tag_matches(cx).is_empty(), |view| {
                view.child(
                    div()
                        .px(px(12.0))
                        .py(px(10.0))
                        .text_size(px(12.0))
                        .text_color(rgb(MUTED))
                        .child(crate::i18n::text("Подходящих значений нет.")),
                )
            })
            .children(
                self.viewer_tag_matches(cx)
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| {
                        let value = name.clone();
                        div()
                            .id(("viewer-suggestion", index))
                            .px(px(12.0))
                            .py(px(10.0))
                            .cursor_pointer()
                            .rounded(px(5.0))
                            .when(self.viewer_state.tag_active == Some(index), |view| {
                                view.bg(rgb(0x2c263f))
                            })
                            .hover(|view| view.bg(rgb(0x242034)))
                            .child(name)
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.viewer_state.tag_input.update(cx, |input, cx| {
                                    input.set_value(value.clone(), window, cx)
                                });
                                this.add_viewer_tag(window, cx);
                            }))
                    }),
            )
    }

    pub(super) fn viewer_panel(
        &self,
        panel: Panel,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let title = match panel {
            Panel::Tags => crate::i18n::text("Теги"),
            Panel::Info => crate::i18n::text("Информация о файле"),
            Panel::Help => crate::i18n::text("Подсказки"),
        };
        let index = self.selected.unwrap_or(0);
        let entry = &self.entries[index];
        let mut content = div().flex().flex_col();

        match panel {
            Panel::Tags => content = self.viewer_tags_panel(index, cx),
            Panel::Info => {
                if entry.video {
                    content = content.child(info_row(
                        crate::i18n::text("Название видео"),
                        vec![self.viewer_video_title(cx)],
                        false,
                        false,
                    ));
                }
                content = content
                    .child(info_row(
                        crate::i18n::text("Произведение"),
                        entry.details.copyrights.clone(),
                        false,
                        false,
                    ))
                    .child(info_row(
                        crate::i18n::text("Автор"),
                        entry.details.artist.iter().cloned().collect(),
                        false,
                        false,
                    ))
                    .child(info_row(
                        crate::i18n::text("Персонажи"),
                        entry
                            .details
                            .character_copyrights
                            .iter()
                            .map(|(name, copyright)| {
                                if entry.demo {
                                    name.clone()
                                } else {
                                    format!("{name} · {copyright}")
                                }
                            })
                            .collect(),
                        true,
                        false,
                    ));
                let dimensions = self
                    .player
                    .as_ref()
                    .and_then(|player| player.read(cx).video_size())
                    .or(if entry.demo {
                        entry.details.resolution
                    } else {
                        self.viewer_state.natural
                    });
                content = content.child(info_row(
                    crate::i18n::text("Разрешение"),
                    dimensions
                        .map(|(w, h)| format!("{w} × {h}"))
                        .into_iter()
                        .collect(),
                    false,
                    true,
                ));
                if entry.video {
                    content = content.child(info_row("FPS", Vec::new(), false, false));
                }
            }
            Panel::Help => {
                let mut rows = vec![("← / →", crate::i18n::text("Предыдущий / следующий файл"))];
                if entry.video {
                    rows.extend([
                        ("Space / K", crate::i18n::text("Воспроизведение / пауза")),
                        ("F", crate::i18n::text("Полный экран")),
                        ("M", crate::i18n::text("Громкость")),
                        ("R", crate::i18n::text("Повтор видео")),
                        ("T", crate::i18n::text("Диагностика воспроизведения")),
                    ]);
                } else {
                    rows.extend([
                        ("+ / −", crate::i18n::text("Масштаб")),
                        ("0", crate::i18n::text("Вписать")),
                    ]);
                }
                rows.push(("Esc", crate::i18n::text("Закрыть панель / просмотрщик")));
                let help = if entry.video {
                    crate::i18n::text(
                        "В полном экране: Влево / Вправо — перемотка на 5 секунд; Вверх / Вниз — громкость с шагом 5%. На паузе панель видима постоянно, после перемотки, изменения громкости или движения мыши — ненадолго.",
                    )
                } else {
                    crate::i18n::text(
                        "Колесо меняет масштаб, перетаскивание перемещает фото. Двойной щелчок или 0 вписывают изображение в окно.",
                    )
                };
                let shortcut_count = rows.len();
                content = content
                    .children(rows.into_iter().enumerate().map(|(index, (key, label))| {
                        shortcut_row(key, label)
                            .when(index + 1 == shortcut_count, |view| view.mb_0())
                    }))
                    .child(note(help));
            }
        }

        div()
            .id("viewer-popover")
            .absolute()
            .top(px(12.0))
            .right(px(20.0))
            .w(px(360.0))
            .max_h(px(f32::from(window.viewport_size().height) - 170.0))
            .overflow_y_scroll()
            .occlude()
            .p(px(18.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgb(0x514361))
            .bg(rgb(0x141420))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .mb(px(16.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .child(title),
                    )
                    .child(
                        sized_icon_button("viewer-panel-close", "close", false, 16.0).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.viewer_state.panel = None;
                                this.viewer_state.focus.focus(window);
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(content)
    }
}

fn shortcut_row(key: &str, label: &str) -> Div {
    let keycap = div()
        .px(px(6.0))
        .py(px(1.0))
        .rounded(px(5.0))
        .bg(rgb(0x1a1827))
        .border_1()
        .border_color(rgb(LINE))
        .text_size(px(12.0))
        .line_height(px(19.2))
        .child(key.to_owned());

    div()
        .mb(px(12.0))
        .flex()
        .items_start()
        .gap(px(12.0))
        .line_height(px(20.8))
        .child(
            div()
                .w(px(84.0))
                .flex_shrink_0()
                .flex()
                .items_start()
                .child(keycap),
        )
        .child(div().flex_1().child(label.to_owned()))
}

fn note(message: &str) -> Div {
    div()
        .mt(px(18.0))
        .text_size(px(12.0))
        .line_height(px(20.4))
        .text_color(rgb(MUTED))
        .child(message.to_owned())
}

fn info_row(label: &str, values: Vec<String>, characters: bool, technical: bool) -> Div {
    div()
        .mb(px(14.0))
        .flex()
        .items_start()
        .gap(px(12.0))
        .when(technical, |view| {
            view.border_t_1().border_color(rgb(LINE)).pt(px(16.0))
        })
        .child(
            div()
                .w(px(105.0))
                .flex_shrink_0()
                .text_color(rgb(MUTED))
                .child(label.to_owned()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .when(values.is_empty(), |view| {
                    view.child(
                        div()
                            .text_color(rgb(MUTED))
                            .child(crate::i18n::text("Недоступно")),
                    )
                })
                .children(
                    values
                        .into_iter()
                        .filter(|value| !value.is_empty())
                        .map(|value| {
                            div()
                                .text_color(rgb(TEXT))
                                .when(characters, |view| {
                                    view.text_color(rgb(0xb0dfc7))
                                        .bg(rgb(0x203b32))
                                        .border_1()
                                        .border_color(rgb(0x285b4a))
                                        .px(px(7.0))
                                        .py(px(3.0))
                                        .rounded(px(5.0))
                                })
                                .child(value)
                        }),
                ),
        )
}
