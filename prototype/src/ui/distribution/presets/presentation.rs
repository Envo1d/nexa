use super::matching_presets;
use crate::appearance::rgb;
use gpui::{Context, Div, Entity, Focusable, Window, div, prelude::*, px};
use gpui_component::input::{Input, InputState};

use super::super::super::motion;
use super::super::{button as base_button, note};
use crate::ui::{Gallery, LINE, MUTED, SURFACE, TEXT, shell};

impl Gallery {
    pub(in crate::ui::distribution) fn preset_control(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let w = self.distribution.as_ref().unwrap();
        div()
            .id("wizard-preset-control")
            .track_focus(&w.preset_state.control_focus)
            .relative()
            .flex_shrink_0()
            .on_mouse_down_out(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                let w = this.distribution.as_mut().unwrap();
                if w.preset_state.picker_bounds.get().contains(&event.position) {
                    return;
                }
                if w.presets {
                    w.presets = false;
                    cx.notify();
                }
            }))
            .child(
                button("wizard-presets", "")
                    .keyboard_focus(false)
                    .track_focus(&w.preset_state.trigger_focus)
                    .relative()
                    .when(w.preset_state.trigger_focus.is_focused(window), |d| {
                        d.child(focus_outline(6.))
                    })
                    .child(shell::icon("copy").size(px(15.)))
                    .child(crate::i18n::text("Пресеты"))
                    .child(shell::icon("down").size(px(14.)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        let w = this.distribution.as_mut().unwrap();
                        w.presets = !w.presets;
                        if w.presets {
                            w.preset_state.backdrop =
                                crate::ui::modal_backdrop::capture(window.scale_factor());
                            w.preset_state.search.update(cx, |s, cx| {
                                s.set_value("", window, cx);
                                s.focus(window, cx);
                            });
                        } else {
                            w.preset_state.trigger_focus.focus(window);
                        }
                        cx.notify();
                    })),
            )
            .when(w.presets, |d| {
                d.child(gpui::deferred(self.preset_picker(window, cx)).with_priority(30))
            })
    }
    fn preset_picker(&self, window: &Window, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let w = self.distribution.as_ref().unwrap();
        let state = &w.preset_state;
        let query = state.search.read(cx).value().to_lowercase();
        let matches = matching_presets(&state.entries, &query);
        let mut list = div()
            .id("preset-picker-list")
            .relative()
            .track_scroll(&state.picker_scroll.handle)
            .pr(state.picker_scroll.gutter())
            .max_h(px(240.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(4.));
        for p in &matches {
            let id = p.id;
            list = list.child(
                button(("preset-choice", id), "")
                    .h_auto()
                    .text_size(px(13.))
                    .line_height(px(16.25))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .p(px(10.))
                    .flex_col()
                    .items_start()
                    .gap(px(4.))
                    .border_color(gpui::transparent_black())
                    .bg(crate::appearance::rgba(0))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(p.name.clone()),
                    )
                    .child(note(&p.metadata.artist).line_height(px(15.)))
                    .child(
                        note(&p.metadata.summary())
                            .line_height(px(15.))
                            .line_clamp(2),
                    )
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.apply_preset(id, window, cx)),
                    ),
            );
        }
        if matches.is_empty() {
            list = list.child(
                note(if state.entries.is_empty() {
                    crate::i18n::text("Пресетов пока нет.")
                } else {
                    crate::i18n::text("Подходящих пресетов нет.")
                })
                .mt(px(5.))
                .line_height(px(18.)),
            );
        }
        let can_save = !w.artist.read(cx).value().trim().is_empty();
        let actions = div()
            .pt_3()
            .border_t_1()
            .border_color(rgb(LINE))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                button("preset-save-as", "")
                    .child(shell::icon("plus").size(px(14.)))
                    .child(crate::i18n::text("Сохранить как пресет…"))
                    .when(!can_save, disabled_button)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let artist = &this.distribution.as_ref().unwrap().artist;
                        if !artist.read(cx).value().trim().is_empty() {
                            this.open_preset_manager(true, window, cx);
                        }
                    })),
            )
            .child(
                button("preset-manage", "")
                    .with_focus(&state.manage_focus)
                    .child(shell::icon("settings").size(px(14.)))
                    .child(crate::i18n::text("Управление пресетами…"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_preset_manager(false, window, cx);
                    })),
            );
        let viewport = window.viewport_size();
        let width = 360_f32.min(f32::from(viewport.width) - 64.);
        let height = 560_f32.min(f32::from(viewport.height) - 150.);

        let help = note(crate::i18n::text(
            "Пресеты заменяют копирайты, автора, персонажей и теги. Названия видео сохраняются.",
        ))
        .truncate()
        .mt(px(5.))
        .line_height(px(18.));

        div()
            .absolute()
            .top(px(40.))
            .right_0()
            .w(px(width))
            .max_h(px(height))
            .id("preset-picker")
            .track_focus(&state.picker_focus)
            .overflow_y_scroll()
            .p_4()
            .rounded(px(10.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(0x1a1827))
            .shadow(reference_shadow())
            .occlude()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(px(16.25))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(crate::i18n::text("Выберите пресет метаданных")),
            )
            .child(plain_input(
                &state.search,
                crate::i18n::text("Поиск пресетов"),
                window,
                cx,
            ))
            .child(list.child(super::scrolling::scrollbar(
                &state.picker_scroll,
                cx.entity_id(),
            )))
            .child(help)
            .when_some(state.error.as_ref(), |d, error| {
                d.child(
                    note(error)
                        .truncate()
                        .mt(px(5.))
                        .line_height(px(18.))
                        .text_color(rgb(0xf292a7)),
                )
            })
            .child(actions)
            .child(bounds_marker(state.picker_bounds.clone()))
    }

    fn preset_field(&self, field: usize, window: &Window, cx: &mut Context<Self>) -> Div {
        let e = self
            .distribution
            .as_ref()
            .unwrap()
            .preset_state
            .editor
            .as_ref()
            .unwrap();
        let state = if field == 3 {
            &e.artist
        } else {
            &e.queries[field]
        };
        let disabled = field == 2 && e.metadata.values[0].is_empty();
        let options = self.preset_options(field, cx);
        let mut content = div().flex().flex_col().min_w_0().child(
            div()
                .mb_2()
                .flex()
                .justify_between()
                .child(
                    note(
                        [
                            crate::i18n::text("Произведения"),
                            crate::i18n::text("Теги"),
                            crate::i18n::text("Персонажи"),
                            crate::i18n::text("Автор"),
                        ][field],
                    )
                    .line_height(px(16.)),
                )
                .when(
                    field < 3 && !e.metadata.values[field.min(2)].is_empty(),
                    |d| {
                        d.child(
                            note(
                                &(if crate::i18n::english() {
                                    format!("{} selected", e.metadata.values[field].len())
                                } else {
                                    format!("{} выбрано", e.metadata.values[field].len())
                                }),
                            )
                            .text_size(px(10.)),
                        )
                    },
                ),
        );
        if field < 3 && !e.metadata.values[field].is_empty() {
            let mut chips = div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .p(px(2.))
                .pr(px(2.) + e.chip_scroll[field].gutter());
            for (index, value) in e.metadata.values[field].iter().enumerate() {
                chips = chips.child(
                    button(("preset-chip", field * 1000 + index), "")
                        .h_auto()
                        .max_w(px(280.))
                        .text_size(px(13.))
                        .line_height(px(16.25))
                        .font_weight(gpui::FontWeight::NORMAL)
                        .px(px(9.))
                        .py(px(6.))
                        .gap(px(6.))
                        .rounded(px(12.))
                        .border_0()
                        .text_size(px(if f32::from(window.viewport_size().width) >= 1450. {
                            11.
                        } else {
                            10.
                        }))
                        .line_height(px(14.))
                        .text_color(rgb(0xd0b4fa))
                        .bg(rgb(0x2c263e))
                        .child(div().min_w_0().truncate().child(value.clone()))
                        .child(shell::icon("close").size(px(12.)).flex_shrink_0())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let e = this
                                .distribution
                                .as_mut()
                                .unwrap()
                                .preset_state
                                .editor
                                .as_mut()
                                .unwrap();
                            e.metadata.values[field].remove(index);
                            e.metadata.prune_characters();
                            e.assignment = None;
                            e.refresh_characters(window, cx);
                            cx.notify();
                        })),
                );
            }
            content = content.child(
                div()
                    .id(("preset-chips", field))
                    .relative()
                    .track_scroll(&e.chip_scroll[field].handle)
                    .max_h(px(66.))
                    .overflow_y_scroll()
                    .mb_2()
                    .child(chips)
                    .child(super::scrolling::scrollbar(
                        &e.chip_scroll[field],
                        cx.entity_id(),
                    )),
            );
        }
        let mut entry = div()
            .id(("preset-field", field))
            .relative()
            .track_focus(&state.focus_handle(cx))
            .h(px(40.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(9.))
            .px_3()
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(0x10101a))
            .focus(|s| s.border_color(rgb(crate::ui::ACCENT)))
            .when(
                field != 3
                    || !self.distribution.as_ref().unwrap().preset_state.catalog[3].is_empty(),
                |d| d.child(shell::icon("search").size(px(15.)).text_color(rgb(MUTED))),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .child(
                        Input::new(state)
                            .appearance(false)
                            .h(px(35.))
                            .px_0()
                            .py(px(8.))
                            .text_size(px(13.))
                            .flex_1()
                            .disabled(disabled),
                    )
                    .when(state.read(cx).value().is_empty(), |d| {
                        let placeholder = match field {
                            0 => crate::i18n::text("Добавить: произведения…"),
                            1 => crate::i18n::text("Добавить: теги…"),
                            2 if disabled => crate::i18n::text("Сначала выберите произведение"),
                            2 => crate::i18n::text("Добавить: персонажи…"),
                            _ => crate::i18n::text("Введите: автор…"),
                        };
                        d.child(placeholder_label(placeholder).left_0().top(px(8.)))
                    }),
            )
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    let options = this.preset_options(field, cx);
                    let e = this
                        .distribution
                        .as_mut()
                        .unwrap()
                        .preset_state
                        .editor
                        .as_mut()
                        .unwrap();
                    match event.keystroke.key.as_str() {
                        "up" | "down" if !options.is_empty() => {
                            let current = e.active.unwrap_or(if event.keystroke.key == "down" {
                                options.len() - 1
                            } else {
                                0
                            });
                            e.active = Some(if event.keystroke.key == "down" {
                                (current + 1) % options.len()
                            } else {
                                (current + options.len() - 1) % options.len()
                            });
                            cx.stop_propagation();
                        }
                        "escape" => {
                            this.distribution_escape(window, cx);
                            cx.stop_propagation();
                        }
                        _ => {}
                    }
                    cx.notify();
                }),
            );
        if field < 3 && !state.read(cx).value().trim().is_empty() && !disabled {
            entry = entry.child(note("Enter").text_size(px(10.)));
        }
        if (e.focused == Some(field) || e.assignment.is_some())
            && !disabled
            && (field != 3
                || !self.distribution.as_ref().unwrap().preset_state.catalog[3].is_empty())
        {
            let mut popup = div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(44.))
                .p(px(5.))
                .rounded(px(8.))
                .bg(rgb(0x1a1827))
                .border_1()
                .border_color(rgb(LINE))
                .shadow_lg()
                .occlude()
                .flex()
                .flex_col();
            if field == 2
                && let Some(name) = &e.assignment
            {
                popup = popup.p(px(8.)).child(
                    div()
                        .mt(px(4.))
                        .mx(px(8.))
                        .mb(px(8.))
                        .flex()
                        .flex_wrap()
                        .text_size(px(12.))
                        .line_height(px(18.))
                        .text_color(rgb(MUTED))
                        .child(crate::i18n::text("К какому произведению относится "))
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(rgb(TEXT))
                                .child(name.clone()),
                        )
                        .child("?"),
                );
                for (i, c) in e.metadata.values[0].iter().enumerate() {
                    let value = format!("{name} ({c})");
                    popup = popup.child(
                        button(("preset-assignment", i), c)
                            .keyboard_focus(false)
                            .track_focus(&e.assignment_focus[i])
                            .relative()
                            .when(e.assignment_focus[i].is_focused(window), |d| {
                                d.child(focus_outline(5.))
                            })
                            .focus(|s| s.shadow(reference_focus_ring()))
                            .justify_start()
                            .h(px(35.))
                            .border_0()
                            .rounded(px(5.))
                            .bg(crate::appearance::rgba(0))
                            .text_size(px(12.))
                            .line_height(px(15.))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                                window.prevent_default()
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.preset_add(2, value.clone(), window, cx)
                            })),
                    );
                }
                popup = popup.child(
                    button("preset-assignment-cancel", crate::i18n::text("Отмена"))
                        .keyboard_focus(false)
                        .track_focus(&e.assignment_focus[e.metadata.values[0].len()])
                        .justify_start()
                        .h(px(33.75))
                        .border_0()
                        .bg(crate::appearance::rgba(0))
                        .text_color(rgb(crate::ui::ACCENT))
                        .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                            window.prevent_default()
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.distribution_escape(window, cx);
                        })),
                );
                entry = entry.child(
                    gpui::deferred(popup.child(bounds_marker(e.popup_bounds.clone())))
                        .with_priority(40),
                );
            } else if !state.read(cx).value().trim().is_empty() {
                for (i, value) in options.iter().enumerate() {
                    let value = value.clone();
                    popup = popup.child(
                        button(("preset-option", field * 10 + i), &value)
                            .justify_start()
                            .border_0()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .h(px(36.25))
                            .bg(rgb(if e.active == Some(i) {
                                0x2c263e
                            } else {
                                0x1a1827
                            }))
                            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                                window.prevent_default()
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.preset_add(field, value.clone(), window, cx)
                            })),
                    );
                }
                if options.is_empty() {
                    popup = popup.child(
                        note(if field == 3 {
                            crate::i18n::text("Подходящих значений нет.")
                        } else {
                            crate::i18n::text("Нажмите Enter, чтобы добавить значение.")
                        })
                        .px(px(12.))
                        .py(px(10.))
                        .line_height(px(18.)),
                    );
                }
                entry = entry.child(
                    gpui::deferred(popup.child(bounds_marker(e.popup_bounds.clone())))
                        .with_priority(40),
                );
            }
        }
        content.child(entry.child(bounds_marker(e.entry_bounds[field].clone())))
    }

    fn preset_library(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let state = &self.distribution.as_ref().unwrap().preset_state;
        let e = state.editor.as_ref().unwrap();
        let compact = f32::from(window.viewport_size().width) <= 680.;
        let query = state.search.read(cx).value().to_lowercase();
        let matches = matching_presets(&state.entries, &query);
        let mut list = div()
            .id("preset-library-list")
            .relative()
            .track_scroll(&e.library_scroll.handle)
            .pr(e.library_scroll.gutter())
            .max_h(px(if compact { 130. } else { 390. }))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(5.));
        for p in &matches {
            let id = p.id;
            list = list.child(
                button(("preset-library", id), "")
                    .h_auto()
                    .text_size(px(13.))
                    .line_height(px(16.25))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .p(px(10.))
                    .flex_col()
                    .items_start()
                    .gap(px(4.))
                    .rounded(px(7.))
                    .bg(crate::appearance::rgba(0))
                    .border_color(gpui::transparent_black())
                    .when(e.selected == Some(id), |d| {
                        d.bg(rgb(0x2c263e)).border_color(rgb(0x6d5aa4))
                    })
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(p.name.clone()),
                    )
                    .child(
                        note(&p.metadata.artist)
                            .text_size(px(10.833))
                            .line_height(px(13.54)),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.fill_preset_editor(Some(id), false, window, cx)
                    })),
            );
        }
        if matches.is_empty() {
            list = list.child(
                note(if state.entries.is_empty() {
                    crate::i18n::text("Пресетов пока нет.")
                } else {
                    crate::i18n::text("Подходящих пресетов нет.")
                })
                .line_height(px(18.)),
            );
        }
        div()
            .flex_shrink_0()
            .w(px(240.))
            .when(compact, |d| d.w_full())
            .p(px(18.))
            .flex()
            .flex_col()
            .gap_3()
            .bg(rgb(0x10101a))
            .border_color(rgb(LINE))
            .when(!compact, |d| d.border_r_1())
            .when(compact, |d| d.border_b_1())
            .child(
                button("preset-new", "")
                    .child(shell::icon("plus").size(px(15.)))
                    .child(crate::i18n::text("Создать из текущих данных"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.fill_preset_editor(None, false, window, cx)
                    })),
            )
            .child(plain_input(
                &state.search,
                crate::i18n::text("Поиск пресетов"),
                window,
                cx,
            ))
            .child(list.child(super::scrolling::scrollbar(
                &e.library_scroll,
                cx.entity_id(),
            )))
    }

    fn preset_editor(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let state = &self.distribution.as_ref().unwrap().preset_state;
        let e = state.editor.as_ref().unwrap();
        let mut editor = div()
            .flex_1()
            .min_w_0()
            .px(px(22.))
            .py(px(20.))
            .flex()
            .flex_col()
            .gap_4()
            .child(
                note(if e.selected.is_some() {
                    crate::i18n::text("Редактирование пресета")
                } else {
                    crate::i18n::text("Новый пресет")
                })
                .line_height(px(18.)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(7.))
                    .child(note(crate::i18n::text("Название пресета")).line_height(px(16.)))
                    .child(plain_input(&e.name, "", window, cx)),
            )
            .child(self.preset_field(3, window, cx))
            .child(self.preset_field(0, window, cx))
            .child(self.preset_field(2, window, cx))
            .child(self.preset_field(1, window, cx))
            .child(
                note(crate::i18n::text(
                    "Сохранение пресета не меняет текущую форму и файлы.",
                ))
                .line_height(px(18.)),
            )
            .child(
                note(crate::i18n::text(
                    "В деморежиме пресеты доступны только пока открыто это окно.",
                ))
                .line_height(px(18.)),
            );
        if let Some(notice) = &state.notice {
            editor = editor.child(note(notice).line_height(px(18.)));
        }
        if let Some(error) = &state.error {
            editor = editor.child(note(error).text_color(rgb(0xf292a7)).line_height(px(18.)));
        }

        editor
    }

    fn preset_footer(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let state = &self.distribution.as_ref().unwrap().preset_state;
        let e = state.editor.as_ref().unwrap();
        let mut footer = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px(px(22.))
            .py_4()
            .border_t_1()
            .border_color(rgb(LINE))
            .flex_shrink_0();
        if e.confirming_delete {
            footer = footer
                .child(
                    div().w_full().child(
                        note(crate::i18n::text(
                            "Удалить выбранный пресет? Данные формы и файлы сохранятся.",
                        ))
                        .line_height(px(18.)),
                    ),
                )
                .child(
                    button(
                        "preset-delete-confirm",
                        crate::i18n::text("Подтвердить удаление"),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        let state = &mut this.distribution.as_mut().unwrap().preset_state;
                        let selected = state.editor.as_ref().unwrap().selected;
                        state.entries.retain(|p| Some(p.id) != selected);
                        state.error = None;
                        state.notice = Some(
                            crate::i18n::text("Пресет метаданных удалён. Данные формы сохранены.")
                                .into(),
                        );
                        tracing::info!(operation = "metadata_presets", "Demo preset deleted");
                        this.fill_preset_editor(None, false, window, cx);
                        this.distribution
                            .as_ref()
                            .unwrap()
                            .preset_state
                            .editor
                            .as_ref()
                            .unwrap()
                            .focus
                            .focus(window);
                    })),
                )
                .child(
                    button("preset-delete-cancel", crate::i18n::text("Отмена"))
                        .with_focus(&e.cancel_focus)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.distribution
                                .as_mut()
                                .unwrap()
                                .preset_state
                                .editor
                                .as_mut()
                                .unwrap()
                                .confirming_delete = false;
                            this.distribution
                                .as_ref()
                                .unwrap()
                                .preset_state
                                .editor
                                .as_ref()
                                .unwrap()
                                .focus
                                .focus(window);
                            cx.notify();
                        })),
                );
        } else {
            let selected = e.selected.is_some();
            let can_save = !e.name.read(cx).value().trim().is_empty()
                && !e.artist.read(cx).value().trim().is_empty();
            footer = footer
                .child(
                    button("preset-delete", "")
                        .child(shell::icon("trash").size(px(15.)))
                        .child(crate::i18n::text("Удалить пресет"))
                        .when(!selected, disabled_button)
                        .on_click(cx.listener(|this, _, window, cx| {
                            let e = this
                                .distribution
                                .as_mut()
                                .unwrap()
                                .preset_state
                                .editor
                                .as_mut()
                                .unwrap();
                            if e.selected.is_some() {
                                e.confirming_delete = true;
                                e.focus.focus(window);
                            }
                            cx.notify();
                        })),
                )
                .child(
                    button("preset-duplicate", "")
                        .child(shell::icon("copy").size(px(15.)))
                        .child(crate::i18n::text("Дублировать пресет"))
                        .when(!selected, disabled_button)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this
                                .distribution
                                .as_ref()
                                .unwrap()
                                .preset_state
                                .editor
                                .as_ref()
                                .unwrap()
                                .selected
                                .is_some()
                            {
                                this.fill_preset_editor(None, true, window, cx);
                            }
                        })),
                )
                .child(div().flex_1())
                .child(
                    button("preset-cancel", crate::i18n::text("Отмена"))
                        .keyboard_focus(false)
                        .track_focus(&e.cancel_focus)
                        .relative()
                        .when(e.cancel_focus.is_focused(window), |d| {
                            d.child(focus_outline(6.))
                        })
                        .focus(|s| s.shadow(reference_focus_ring()))
                        .on_click(
                            cx.listener(|this, _, window, cx| {
                                this.close_preset_manager(window, cx)
                            }),
                        ),
                )
                .child(
                    button(
                        "preset-save",
                        if selected {
                            crate::i18n::text("Обновить выбранный пресет")
                        } else {
                            crate::i18n::text("Сохранить новый пресет")
                        },
                    )
                    .bg(motion::Surface::gradient(
                        120.,
                        rgb(0x7b43d9),
                        rgb(0x6839ce),
                    ))
                    .border_color(crate::appearance::rgba(0))
                    .hover_colors(Some(rgb(0x8853e1).into()), None, None)
                    .when(!can_save, disabled_button)
                    .keyboard_focus(false)
                    .track_focus(&e.save_focus.clone().tab_stop(can_save))
                    .relative()
                    .when(e.save_focus.is_focused(window), |d| {
                        d.child(focus_outline(6.))
                    })
                    .focus(|s| s.shadow(reference_focus_ring()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if can_save {
                            this.save_preset(window, cx);
                        }
                    })),
                );
        }

        footer
    }

    fn preset_header(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let state = &self.distribution.as_ref().unwrap().preset_state;
        let e = state.editor.as_ref().unwrap();
        div()
            .flex()
            .items_center()
            .gap_4()
            .px(px(22.))
            .py(px(18.))
            .border_b_1()
            .border_color(rgb(LINE))
            .flex_shrink_0()
            .child(
                div()
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(18.))
                            .line_height(px(24.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .mb(px(6.))
                            .child(crate::i18n::text("Управление пресетами метаданных")),
                    )
                    .child(
                        note(crate::i18n::text(
                            "Редактируйте наборы метаданных независимо от текущего файла.",
                        ))
                        .line_height(px(18.)),
                    ),
            )
            .child(
                button("preset-close", "")
                    .keyboard_focus(false)
                    .track_focus(&e.close_focus)
                    .relative()
                    .when(e.close_focus.is_focused(window), |d| {
                        d.child(focus_outline(5.))
                    })
                    .focus(|s| s.shadow(reference_focus_ring()))
                    .size(px(28.))
                    .p_0()
                    .border_0()
                    .bg(crate::appearance::rgba(0))
                    .text_color(rgb(MUTED))
                    .hover_colors(
                        Some(crate::appearance::rgba(0xa786ff1a).into()),
                        Some(rgb(TEXT).into()),
                        None,
                    )
                    .child(shell::icon("close").size(px(18.)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_preset_manager(window, cx);
                    })),
            )
    }

    pub(in crate::ui::distribution) fn preset_manager(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let state = &self.distribution.as_ref().unwrap().preset_state;
        let e = state.editor.as_ref().unwrap();
        let compact = f32::from(window.viewport_size().width) <= 680.;
        let library = self.preset_library(window, cx);
        let editor = self.preset_editor(window, cx);
        let footer = self.preset_footer(window, cx);
        let header = self.preset_header(window, cx);
        let body = div()
            .relative()
            .id("preset-manager-body")
            .track_scroll(&e.scroll.handle)
            .overflow_y_scroll()
            .min_h_0()
            .child(
                div()
                    .pr(e.scroll.gutter())
                    .flex()
                    .when(compact, |d| d.flex_col())
                    .child(library)
                    .child(editor),
            )
            .child(super::scrolling::scrollbar(&e.scroll, cx.entity_id()));
        let viewport = window.viewport_size();
        let width = 900_f32.min(f32::from(viewport.width) - 40.);
        let height = f32::from(viewport.height) - 40.;
        let panel = div()
            .id("preset-manager")
            .track_focus(&e.focus)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    let Some(w) = &mut this.distribution else {
                        return;
                    };
                    let Some(e) = &mut w.preset_state.editor else {
                        return;
                    };
                    let outside_assignment = !e.popup_bounds.get().contains(&event.position)
                        && !e.entry_bounds[2].get().contains(&event.position);
                    if e.assignment.is_some() && outside_assignment {
                        e.assignment = None;
                        e.assignment_enter_pending = false;
                        cx.notify();
                    }
                }),
            )
            .capture_key_up(cx.listener(|this, event: &gpui::KeyUpEvent, window, cx| {
                let Some(w) = &mut this.distribution else {
                    return;
                };
                let Some(e) = &mut w.preset_state.editor else {
                    return;
                };
                if event.keystroke.key == "enter" && e.assignment_enter_pending {
                    e.assignment_enter_pending = false;
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .w(px(width))
            .max_h(px(height))
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .text_color(rgb(TEXT))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(14.))
            .shadow(reference_shadow())
            .on_mouse_down_out(
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    let e = this
                        .distribution
                        .as_ref()
                        .unwrap()
                        .preset_state
                        .editor
                        .as_ref()
                        .unwrap();
                    let state = &this.distribution.as_ref().unwrap().preset_state;
                    let popup_open = e.assignment.is_some()
                        || e.focused.is_some_and(|field| {
                            let input = if field == 3 {
                                &e.artist
                            } else {
                                &e.queries[field]
                            };
                            !input.read(cx).value().trim().is_empty()
                                && (field != 3 || !state.catalog[3].is_empty())
                        });
                    if popup_open && e.popup_bounds.get().contains(&event.position) {
                        return;
                    }
                    this.close_preset_manager(window, cx);
                }),
            )
            .child(header)
            .child(body)
            .child(footer);

        div()
            .absolute()
            .inset_0()
            .bg(crate::appearance::rgba(0x05040cab))
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .when_some(state.backdrop.clone(), |d, backdrop| {
                d.child(
                    gpui::img(backdrop)
                        .absolute()
                        .inset_0()
                        .size_full()
                        .object_fit(gpui::ObjectFit::Fill),
                )
            })
            .child(panel)
    }
}

fn disabled_button(d: motion::MotionDiv) -> motion::MotionDiv {
    d.keyboard_focus(false)
        .opacity(0.35)
        .cursor_default()
        .hover_colors(None, None, None)
}

fn plain_input(
    state: &Entity<InputState>,
    placeholder: &str,
    window: &Window,
    cx: &Context<Gallery>,
) -> gpui::Stateful<Div> {
    div()
        .id(("preset-text", state.entity_id()))
        .track_focus(&state.focus_handle(cx))
        .focus(|s| {
            s.border_color(rgb(crate::ui::ACCENT))
                .shadow(reference_focus_ring())
        })
        .relative()
        .when(state.focus_handle(cx).is_focused(window), |d| {
            d.child(focus_outline(7.))
        })
        .h(px(35.))
        .flex_shrink_0()
        .rounded(px(7.))
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(0x1a1927))
        .child(
            Input::new(state)
                .appearance(false)
                .h(px(33.))
                .px(px(10.))
                .py(px(7.))
                .text_size(px(13.)),
        )
        .when(
            state.read(cx).value().is_empty() && !placeholder.is_empty(),
            |d| d.child(placeholder_label(placeholder).left(px(10.)).top(px(8.))),
        )
}

fn placeholder_label(text: &str) -> Div {
    div()
        .absolute()
        .right_0()
        .truncate()
        .text_size(px(13.))
        .line_height(px(16.25))
        .text_color(rgb(0x757575))
        .child(text.to_owned())
}

fn reference_shadow() -> Vec<gpui::BoxShadow> {
    vec![gpui::BoxShadow {
        color: crate::appearance::rgba(0x00000080).into(),
        offset: gpui::point(px(0.), px(20.)),
        blur_radius: px(70.),
        spread_radius: px(0.),
    }]
}

fn button(id: impl Into<gpui::ElementId>, title: &str) -> motion::MotionDiv {
    base_button(id, title)
        .flex_shrink_0()
        .keyboard_focus(true)
        .focus_ring(rgb(SURFACE))
        .h(px(32.))
        .hover_colors(Some(rgb(0x242034).into()), None, Some(rgb(0x6d5aa4).into()))
}

fn bounds_marker(
    bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
) -> impl IntoElement {
    gpui::canvas(move |measured, _, _| bounds.set(measured), |_, _, _, _| {})
        .absolute()
        .inset_0()
        .size_full()
}

fn reference_focus_ring() -> Vec<gpui::BoxShadow> {
    [(crate::ui::ACCENT, 5.), (SURFACE, 3.)]
        .into_iter()
        .map(|(color, spread)| gpui::BoxShadow {
            color: rgb(color).into(),
            offset: gpui::point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(spread),
        })
        .collect()
}

fn focus_outline(radius: f32) -> Div {
    div()
        .absolute()
        .top(px(-5.))
        .right(px(-5.))
        .bottom(px(-5.))
        .left(px(-5.))
        .rounded(px(radius + 5.))
        .border_2()
        .border_color(rgb(crate::ui::ACCENT))
}
