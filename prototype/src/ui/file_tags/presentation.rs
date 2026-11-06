use gpui::Focusable;
use gpui::{BoxShadow, Context, Div, FontWeight, Window, div, img, point, prelude::*, px};
use gpui_component::input::Input;

use super::super::{ACCENT, Gallery, LINE, MUTED, SURFACE, TEXT, motion, shell::icon};
use crate::appearance::{rgb, rgba};

fn button(id: &'static str, label: &str) -> motion::MotionDiv {
    super::super::button(id, label)
        .keyboard_focus(true)
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
        .font_weight(FontWeight::MEDIUM)
}

impl Gallery {
    pub(in crate::ui) fn file_tags_dialog(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let e = self.tag_editor.as_ref().unwrap();
        let options = self.file_tag_matches(cx);
        let input_bounds = e.input_bounds.clone();
        let width = (f32::from(window.viewport_size().width) - 32.).min(480.);
        let mut chips = div()
            .id("file-tag-chips")
            .min_h(px(28.))
            .max_h(px(160.))
            .overflow_y_scroll()
            .flex()
            .flex_wrap()
            .gap_2();
        for (index, tag) in e.draft.iter().enumerate() {
            chips = chips.child(
                motion::button(("file-tag-chip", index))
                    .keyboard_focus(true)
                    .cursor_pointer()
                    .max_w_full()
                    .whitespace_normal()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(rgb(0x6d5aa4))
                    .bg(rgb(0x2c2644))
                    .text_color(rgb(0xd0b4fa))
                    .text_size(px(12.))
                    .hover_colors(Some(rgb(0x393159).into()), None, None)
                    .child(tag.clone())
                    .child(icon("close").size(px(12.)).flex_shrink_0())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tag_editor.as_mut().unwrap().draft.remove(index);
                        cx.notify();
                    })),
            );
        }
        if e.draft.is_empty() {
            chips = chips.child(
                div()
                    .text_color(rgb(MUTED))
                    .child(crate::i18n::text("Тегов пока нет")),
            );
        }

        let mut search = div().relative().flex().flex_col().gap_2().child(
            div()
                .flex()
                .items_end()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .relative()
                        .h(px(40.))
                        .flex()
                        .items_center()
                        .px_3()
                        .gap(px(9.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(0x10101a))
                        .track_focus(&e.query.focus_handle(cx))
                        .focus(|s| s.border_color(rgb(ACCENT)))
                        .child(
                            gpui::canvas(
                                move |bounds, _, _| input_bounds.set(bounds),
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .inset_0()
                            .size_full(),
                        )
                        .child(
                            icon("search")
                                .size(px(15.))
                                .text_color(rgb(MUTED))
                                .flex_shrink_0(),
                        )
                        .child(
                            Input::new(&e.query)
                                .appearance(false)
                                .h(px(38.))
                                .p_0()
                                .text_size(px(13.))
                                .flex_1(),
                        ),
                )
                .child(
                    button("file-tag-add", "")
                        .px(px(12.))
                        .h(px(40.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(icon("plus").size(px(16.)))
                        .child(crate::i18n::text("Добавить"))
                        .when(e.query.read(cx).value().trim().is_empty(), |d| {
                            d.opacity(0.4).cursor_default()
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            let value = this
                                .tag_editor
                                .as_ref()
                                .unwrap()
                                .query
                                .read(cx)
                                .value()
                                .to_string();
                            this.add_file_tag(Some(value), window, cx);
                        })),
                ),
        );
        if e.suggestions && !e.query.read(cx).value().trim().is_empty() {
            search = search.child(
                gpui::deferred(
                    div()
                        .absolute()
                        .top(px(44.))
                        .left_0()
                        .w(e.input_bounds.get().size.width)
                        .occlude()
                        .id("file-tag-options")
                        .max_h(px((f32::from(
                            window.viewport_size().height - e.input_bounds.get().bottom(),
                        ) - 16.)
                            .clamp(0., 220.)))
                        .overflow_y_scroll()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(0x1a1827))
                        .p(px(5.))
                        .shadow(vec![BoxShadow {
                            color: rgba(0x00000080).into(),
                            offset: point(px(0.), px(12.)),
                            blur_radius: px(32.),
                            spread_radius: px(0.),
                        }])
                        .when(options.is_empty(), |d| {
                            d.child(
                                div()
                                    .px_3()
                                    .py(px(10.))
                                    .text_size(px(12.))
                                    .text_color(rgb(MUTED))
                                    .child(crate::i18n::text("Подходящих значений нет.")),
                            )
                        })
                        .children(options.into_iter().enumerate().map(|(index, value)| {
                            motion::button(("file-tag-option", index))
                                .keyboard_focus(false)
                                .cursor_pointer()
                                .w_full()
                                .px_3()
                                .py(px(10.))
                                .rounded(px(5.))
                                .text_size(px(13.))
                                .line_height(px(16.25))
                                .text_ellipsis()
                                .when(e.active == Some(index), |d| d.bg(rgb(0x2c263f)))
                                .hover_colors(Some(rgb(0x2c263f).into()), None, None)
                                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                                    window.prevent_default()
                                })
                                .child(value.clone())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let editor = this.tag_editor.as_mut().unwrap();
                                    editor.query.update(cx, |s, cx| {
                                        s.set_value(value.clone(), window, cx);
                                        s.focus(window, cx);
                                    });
                                    editor.suggestions = false;
                                    editor.active = None;
                                    cx.notify();
                                }))
                        })),
                )
                .with_priority(40),
            );
        }

        let panel = div()
            .id("file-tags-panel")
            .track_focus(&e.focus)
            .w(px(width))
            .max_h(window.viewport_size().height * 0.85)
            .overflow_y_scroll()
            .p_6()
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(13.))
            .bg(gpui::linear_gradient(
                130.,
                gpui::linear_color_stop(rgb(0x1a1827), 0.),
                gpui::linear_color_stop(rgb(SURFACE), 1.),
            ))
            .shadow(vec![BoxShadow {
                color: rgba(0x000000a6).into(),
                offset: point(px(0.), px(20.)),
                blur_radius: px(100.),
                spread_radius: px(0.),
            }])
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && this.tag_editor.as_ref().unwrap().suggestions
                {
                    this.tag_editor.as_mut().unwrap().suggestions = false;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if matches!(event.keystroke.key.as_str(), "up" | "down") {
                    let count = this.file_tag_matches(cx).len();
                    if count > 0 {
                        let e = this.tag_editor.as_mut().unwrap();
                        e.suggestions = true;
                        e.active = Some(match (e.active, event.keystroke.key.as_str()) {
                            (None, "down") => 0,
                            (None, _) => count - 1,
                            (Some(i), "down") => (i + 1) % count,
                            (Some(i), _) => (i + count - 1) % count,
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }
                }
            }))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_4()
                    .pb_6()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .size(px(40.))
                                    .flex_shrink_0()
                                    .rounded(px(11.))
                                    .border_1()
                                    .border_color(rgb(0x6d5aa4))
                                    .bg(rgb(0x2c263f))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(ACCENT))
                                    .child(icon("tag").size(px(20.))),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_size(px(17.))
                                            .font_weight(FontWeight::BOLD)
                                            .child(crate::i18n::text("Редактировать теги")),
                                    )
                                    .child(
                                        div()
                                            .mt(px(6.))
                                            .text_size(px(12.))
                                            .line_height(px(18.))
                                            .text_color(rgb(MUTED))
                                            .whitespace_normal()
                                            .child(self.entries[e.index].details.filename.clone()),
                                    ),
                            ),
                    )
                    .child(
                        button("file-tag-close", "")
                            .with_focus(&e.close_focus)
                            .size(px(28.))
                            .px_0()
                            .border_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("close").size(px(18.)))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_file_tags(window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .p_4()
                    .border_1()
                    .border_color(rgb(LINE))
                    .rounded(px(10.))
                    .bg(rgb(0x1a1927))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .mb(px(14.))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(MUTED))
                                    .child(crate::i18n::text("Выбранные теги")),
                            )
                            .child(
                                div()
                                    .min_w(px(24.))
                                    .px(px(7.))
                                    .py(px(3.))
                                    .rounded(px(6.))
                                    .text_size(px(11.))
                                    .text_color(rgb(MUTED))
                                    .bg(rgb(0x1a1827))
                                    .text_center()
                                    .child(e.draft.len().to_string()),
                            ),
                    )
                    .child(chips),
            )
            .child(
                div()
                    .mt_5()
                    .child(
                        div()
                            .mb(px(10.))
                            .text_size(px(12.))
                            .line_height(px(18.))
                            .text_color(rgb(MUTED))
                            .child(crate::i18n::text(
                                "Найдите существующий тег или введите новый.",
                            )),
                    )
                    .child(search),
            )
            .when_some(e.error.clone(), |d, error| {
                d.child(
                    div()
                        .mt(px(7.))
                        .text_size(px(11.))
                        .text_color(rgb(0xff8899))
                        .child(error),
                )
            })
            .child(
                div()
                    .my(px(15.))
                    .text_size(px(11.))
                    .line_height(px(18.7))
                    .text_color(rgb(MUTED))
                    .child(crate::i18n::text(
                        "Изменения сохраняются только на время демосессии.",
                    )),
            )
            .child(
                div()
                    .mt_5()
                    .pt(px(18.))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        button("file-tag-cancel", crate::i18n::text("Отмена")).on_click(
                            cx.listener(|this, _, window, cx| this.close_file_tags(window, cx)),
                        ),
                    )
                    .child(
                        button("file-tag-save", crate::i18n::text("Сохранить теги"))
                            .with_focus(&e.save_focus)
                            .bg(motion::Surface::gradient(
                                120.,
                                rgb(0x7b43d9),
                                rgb(0x6839ce),
                            ))
                            .text_color(rgb(0xffffff))
                            .hover_colors(Some(rgb(0x8853e1).into()), None, None)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.save_file_tags(window, cx)),
                            ),
                    ),
            );

        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x05040cab))
            .text_color(rgb(TEXT))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_file_tags(window, cx)),
            )
            .when_some(e.backdrop.clone(), |d, image| {
                d.child(img(image).absolute().inset_0().size_full())
            })
            .child(panel)
    }
}
