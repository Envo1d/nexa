use gpui::{Context, Div, Window, div, prelude::*, px};

use super::{Gallery, LINE, MUTED, TEXT, motion, presets::scrolling};
use crate::appearance::rgb;

impl Gallery {
    pub(super) fn wizard_scope(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let w = self.distribution.as_ref().unwrap();
        let remaining = w.queue.len() - w.position;
        let mut choices = div().flex().flex_wrap().gap(px(18.));

        for (i, label) in [
            crate::i18n::text("Текущий файл").to_string(),
            (if crate::i18n::english() {
                format!("All remaining files ({remaining})")
            } else {
                format!("Все оставшиеся файлы ({remaining})")
            }),
        ]
        .into_iter()
        .enumerate()
        {
            let active = w.all == (i == 1);
            let focus = w.scope_focus[i].clone().tab_stop(active);
            choices = choices.child(
                motion::button(("wizard-scope", i))
                    .with_focus(&focus)
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                        if matches!(
                            event.keystroke.key.as_str(),
                            "left" | "up" | "right" | "down"
                        ) {
                            let w = this.distribution.as_mut().unwrap();
                            w.all = !w.all;
                            w.error = None;
                            w.scope_focus[usize::from(w.all)].focus(window);
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .relative()
                            .size(px(13.))
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(if active { super::super::ACCENT } else { MUTED }))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(active, |d| {
                                d.child(
                                    div()
                                        .size(px(7.))
                                        .rounded_full()
                                        .bg(rgb(super::super::ACCENT)),
                                )
                            })
                            .when(focus.is_focused(window), |d| {
                                d.child(
                                    div()
                                        .absolute()
                                        .inset(px(-5.))
                                        .rounded_full()
                                        .border_2()
                                        .border_color(rgb(super::super::ACCENT)),
                                )
                            }),
                    )
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let w = this.distribution.as_mut().unwrap();
                        w.all = i == 1;
                        w.error = None;
                        w.scope_focus[i].focus(window);
                        cx.notify();
                    })),
            );
        }

        // A fieldset legend interrupts the top border and contributes half its
        // line box above it, unlike a separate heading inside a flex column.
        div()
            .relative()
            .mt(px(7.))
            .mb(px(18.))
            .p_3()
            .pt(px(19.))
            .border_1()
            .border_color(rgb(LINE))
            .rounded_lg()
            .child(
                div()
                    .absolute()
                    .top(px(-8.))
                    .left(px(12.))
                    .px(px(5.))
                    .bg(rgb(super::SHELL))
                    .text_size(px(11.))
                    .line_height(px(15.))
                    .text_color(rgb(MUTED))
                    .child(crate::i18n::text("Применить эти сведения к")),
            )
            .child(choices)
    }

    pub(super) fn wizard_batch_files(&self, cx: &mut Context<Self>) -> Div {
        let w = self.distribution.as_ref().unwrap();
        let remaining = w.queue.len() - w.position;
        let mut section = div()
            .mt(px(18.))
            .text_size(px(12.))
            .line_height(px(16.))
            .child(
                motion::button("wizard-batch-toggle")
                    .keyboard_focus(true)
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .text_color(rgb(0xd0b4fa))
                    .child(if w.batch_expanded { "▾" } else { "▸" })
                    .child(if crate::i18n::english() {
                        format!("Files to distribute ({remaining})")
                    } else {
                        format!("Файлы для распределения ({remaining})")
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        let w = this.distribution.as_mut().unwrap();
                        w.batch_expanded = !w.batch_expanded;
                        cx.notify();
                    })),
            );

        if w.batch_expanded {
            let mut list = div()
                .id("wizard-batch-files")
                .relative()
                .mt(px(10.))
                .max_h(px(140.))
                .overflow_y_scroll()
                .track_scroll(&w.batch_scroll.handle)
                .pr(w.batch_scroll.gutter());

            for &index in &w.queue[w.position..] {
                let entry = &self.entries[index];
                list = list.child(
                    div()
                        .flex()
                        .justify_between()
                        .gap_3()
                        .py_2()
                        .border_b_1()
                        .border_color(rgb(LINE))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(rgb(TEXT))
                                .child(entry.details.filename.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(rgb(MUTED))
                                .child(entry.extension.clone()),
                        ),
                );
            }

            section =
                section.child(list.child(scrolling::scrollbar(&w.batch_scroll, cx.entity_id())));
        }

        section
    }
}
