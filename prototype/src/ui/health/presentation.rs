use gpui::{Context, Div, ElementId, FontWeight, div, prelude::*, px, rgb};

use super::{
    DEMO, Gallery, HINTS, ICONS, LABELS, LINE, MUTED, PATHS, REASONS, TEXT, icon, muted, panel,
};
use crate::ui::motion;

pub(super) fn button(id: impl Into<ElementId>, title: &str) -> motion::MotionDiv {
    crate::ui::button(id, title)
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .min_h(px(32.0))
        .px_3()
        .py_2()
        .rounded(px(6.0))
        .text_size(px(11.0))
        .line_height(px(14.0))
        .font_weight(FontWeight::MEDIUM)
        .bg(rgb(0x1a1827))
        .text_color(rgb(TEXT))
        .flex_shrink_0()
}

fn primary(id: &'static str, title: &str) -> motion::MotionDiv {
    button(id, title)
        .bg(motion::Surface::gradient(
            120.0,
            rgb(0x7b43d9),
            rgb(0x6839ce),
        ))
        .border_color(gpui::rgba(0))
        .text_color(rgb(0xffffff))
}

impl Gallery {
    pub(super) fn health_repairs(&self, cx: &mut Context<Self>) -> Div {
        let actions = div().flex().flex_wrap().gap_2().mt_4();
        match self.health.category {
            0 => actions
                .child(
                    muted(concat!(
                        "Укажите переименованный или перемещённый файл. ",
                        "Его ID в каталоге, теги и избранное сохранятся."
                    ))
                    .w_full()
                    .my(px(5.0)),
                )
                .child(
                    primary("health-locate", "")
                        .child(icon("staging").size(px(16.0)))
                        .child("Указать файл…")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.health.proposal = true;
                            this.health.confirmed = false;
                            tracing::info!(
                                operation = "health_relink_proposal",
                                storage = "demo_session",
                                "Demo relink proposal prepared"
                            );
                            cx.notify();
                        })),
                ),
            1 => actions
                .child(
                    primary("health-recovery", "Повторить восстановление")
                        .on_click(cx.listener(|this, _, _, cx| this.repair_health(cx))),
                )
                .child(
                    button("health-indexing", "Настройки индексирования").on_click(cx.listener(
                        |this, _, _, cx| this.unavailable("Настройки индексирования", cx),
                    )),
                ),
            2 => actions.child(
                primary("health-preview", "Повторить создание превью")
                    .on_click(cx.listener(|this, _, _, cx| this.repair_health(cx))),
            ),
            _ => actions
                .child(
                    primary("health-metadata", "Перечитать технические метаданные")
                        .on_click(cx.listener(|this, _, _, cx| this.repair_health(cx))),
                )
                .child(button("health-edit", "Редактировать метаданные").on_click(
                    cx.listener(|this, _, _, cx| this.unavailable("Редактор метаданных файла", cx)),
                )),
        }
    }

    pub(super) fn health_layout(
        &self,
        detail: gpui::Stateful<Div>,
        filename: &str,
        present: bool,
        total: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let category = self.health.category;
        let description = concat!(
            "Проверяйте проблемы каталога и восстанавливайте связь с файлами, ",
            "сохраняя их разметку."
        );
        let feedback = self.health.message.clone().unwrap_or_else(|| format!("Найдено проблем: {total}. Нажмите «Проверить файлы», чтобы проверить пути на диске."));
        let heading = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .p_4()
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::BOLD)
                            .child(LABELS[category]),
                    )
                    .child(muted(HINTS[category]).text_size(px(12.0)).my(px(5.0))),
            )
            .child(muted(if present { "1" } else { "0" }));
        let row = crate::ui::button("health-issue", "")
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .p_3()
            .rounded(px(8.0))
            .bg(rgb(0x2c263f))
            .border_color(rgb(0x6d5a90))
            .child(
                div()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(rgb(0x1a1827))
                    .text_color(rgb(0xa786ff))
                    .child(
                        icon(if category == 3 {
                            "video"
                        } else if category == 1 {
                            "git-branch"
                        } else {
                            "image"
                        })
                        .size(px(22.0)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child(filename.to_owned()),
                    )
                    .child(
                        muted(if category == 1 {
                            REASONS[category]
                        } else {
                            PATHS[category]
                        })
                        .text_size(px(11.0))
                        .line_height(px(14.0))
                        .whitespace_nowrap(),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .line_height(px(14.0))
                            .text_color(rgb(0xefbc79))
                            .child(REASONS[category]),
                    ),
            )
            .child(icon("arrow").size(px(16.0)))
            .on_click(cx.listener(|_, _, _, cx| cx.notify()));
        let footer = div()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(rgb(LINE))
            .child(
                muted(if present {
                    "1–1 из 1"
                } else {
                    "0–0 из 0"
                })
                .flex_1()
                .text_size(px(11.0)),
            )
            .child(button("health-prev", "Назад").opacity(0.4).cursor_default())
            .child(button("health-next", "Далее").opacity(0.4).cursor_default());

        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(LINE))
            .bg(gpui::linear_gradient(
                110.0,
                gpui::linear_color_stop(rgb(0x141420), 0.0),
                gpui::linear_color_stop(rgb(0x10101a), 1.0),
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(300.0))
                            .child(
                                muted("Обслуживание библиотеки")
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .mb_2()
                                    .text_size(px(20.0))
                                    .line_height(px(24.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Состояние библиотеки"),
                            )
                            .child(muted(description).my(px(5.0))),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(
                                button("health-refresh", "")
                                    .child(icon("reset").size(px(16.0)))
                                    .child("Обновить")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.health.message = None;
                                        tracing::info!(
                                            operation = "health_refresh",
                                            storage = "demo_session",
                                            "Session diagnostics refreshed"
                                        );
                                        cx.notify();
                                    })),
                            )
                            .child(
                                primary("health-check", "")
                                    .child(icon("scan").size(px(16.0)))
                                    .child("Проверить файлы")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.health.message = Some(DEMO.into());
                                        tracing::info!(
                                            operation = "health_check",
                                            outcome = "demo_only",
                                            "Production file verification is not connected"
                                        );
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .my(px(5.0))
                    .px_3()
                    .py_2()
                    .rounded(px(8.0))
                    .bg(rgb(0x152b46))
                    .text_color(rgb(0x8bbbff))
                    .child(DEMO),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children((0..4).map(|index| {
                        crate::ui::button(("health-category", index), "")
                            .h(px(36.0))
                            .px_3()
                            .py_2()
                            .rounded(px(6.0))
                            .bg(rgb(0x1a1827))
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(category == index, |view| {
                                view.bg(rgb(0x2c263f)).border_color(rgb(0x6d5a90))
                            })
                            .child(icon(ICONS[index]).size(px(16.0)))
                            .child(LABELS[index])
                            .child(
                                div()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(rgb(0x242034))
                                    .text_size(px(11.0))
                                    .text_color(rgb(MUTED))
                                    .child(if self.health.remaining[index] {
                                        "1"
                                    } else {
                                        "0"
                                    }),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.health.choose(index);
                                cx.notify();
                            }))
                    })),
            )
            .child(muted(feedback).min_h(px(24.0)))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_4()
                    .child(
                        panel()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .overflow_hidden()
                            .child(heading)
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .p_2()
                                    .when(present, |view| view.child(row))
                                    .when(!present, |view| view.flex().child(super::empty())),
                            )
                            .child(footer),
                    )
                    .child(detail),
            )
    }
}
