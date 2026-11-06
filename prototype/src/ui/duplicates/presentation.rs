use gpui::{Context, Div, FontWeight, ObjectFit, Window, div, img, prelude::*, px, relative};

use super::{Action, Version};
use crate::appearance::rgb;
use crate::ui::{
    ACCENT, BACKGROUND, Gallery, LINE, MUTED, SURFACE, TEXT, button, motion, shell::icon,
};

const EXPLANATION: &str = "Поиск среди проиндексированных файлов Main и Stage. Точные совпадения проверяются по SHA-256, похожие изображения — по превью. Файлы и метаданные сохраняются независимо до выбранного вами действия.";
const DEMO: &str =
    "Демонстрационные данные · поиск, отметки и действия действуют только в этом сеансе.";

fn muted(text: impl Into<String>) -> Div {
    div()
        .text_color(rgb(MUTED))
        .line_height(px(19.2))
        .child(text.into())
}

fn action(
    id: impl Into<gpui::ElementId>,
    label: &str,
    name: &'static str,
    enabled: bool,
    primary: bool,
) -> motion::MotionDiv {
    button(id, "")
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .text_size(px(12.0))
        .line_height(px(16.0))
        .rounded(px(7.0))
        .keyboard_focus(enabled)
        .when(primary, |view| {
            view.bg(rgb(0x7541cf))
                .border_color(rgb(0x925bea))
                .text_color(rgb(0xffffff))
        })
        .when(!enabled, |view| view.opacity(0.45).cursor_default())
        .child(icon(name).size(px(16.0)))
        .child(label.to_owned())
}

impl Gallery {
    pub(super) fn duplicates_header(&self) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(14.0))
            .px(px(20.0))
            .py(px(16.0))
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(LINE))
            .bg(gpui::linear_gradient(
                110.0,
                gpui::linear_color_stop(rgb(0x251b3b), 0.0),
                gpui::linear_color_stop(rgb(SURFACE), 0.65),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(48.0))
                    .rounded(px(14.0))
                    .bg(rgb(0x2a203d))
                    .border_1()
                    .border_color(rgb(0x49345f))
                    .text_color(rgb(ACCENT))
                    .child(icon("files").size(px(24.0))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        muted(crate::i18n::text("Проверка библиотеки"))
                            .font_weight(FontWeight::MEDIUM),
                    )
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(FontWeight::BOLD)
                            .child(crate::i18n::text("Поиск дублей по содержимому")),
                    ),
            )
    }

    pub(super) fn duplicates_controls(&self, cx: &mut Context<Self>) -> Div {
        let state = &self.duplicates;
        let progress = state.processed * 100 / state.pairs.len();
        let status = if state.running {
            crate::i18n::text("Поиск…")
        } else if state.stopped {
            crate::i18n::text("Остановлено · неполные результаты")
        } else if state.saved_at.is_some() {
            crate::i18n::text("Поиск завершён")
        } else {
            crate::i18n::text("Готово к поиску")
        };
        let actions = div()
            .flex()
            .flex_wrap()
            .gap(px(10.0))
            .child(
                action(
                    "duplicate-scan",
                    if state.saved_at.is_some() {
                        crate::i18n::text("Обновить результаты")
                    } else {
                        crate::i18n::text("Проверить Main и Stage")
                    },
                    "search",
                    !state.running,
                    true,
                )
                .on_click(cx.listener(|this, _, _, cx| this.scan_duplicates(cx))),
            )
            .when(state.running, |view| {
                view.child(
                    action(
                        "duplicate-stop",
                        crate::i18n::text("Остановить после текущего пакета"),
                        "pause",
                        !state.stopped,
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.duplicates.stopped {
                            this.duplicates.stopped = true;
                            tracing::info!(
                                operation = "duplicate_scan",
                                "Demo search stop requested"
                            );
                            cx.notify();
                        }
                    })),
                )
            });

        let status_icon = if state.saved_at.is_some() && !state.stopped {
            "circle-check"
        } else {
            "info"
        };
        let scan_card = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(14.0))
            .py(px(12.0))
            .rounded(px(10.0))
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(if state.running { 0x49345f } else { LINE }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(icon(status_icon).size(px(16.0)))
                            .child(status),
                    )
                    .when(state.running || state.saved_at.is_some(), |view| {
                        view.child(
                            div()
                                .text_color(rgb(ACCENT))
                                .font_weight(FontWeight::BOLD)
                                .child(format!("{progress}%")),
                        )
                    }),
            )
            .child(
                div()
                    .h(px(6.0))
                    .mt(px(4.0))
                    .rounded(px(20.0))
                    .bg(rgb(0x20202d))
                    .overflow_hidden()
                    .child(
                        div()
                            .h_full()
                            .w(relative(progress as f32 / 100.0))
                            .rounded(px(20.0))
                            .bg(rgb(ACCENT)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(11.0))
                    .text_color(rgb(MUTED))
                    .child(if crate::i18n::english() {
                        format!(
                            "Processed {} of {} files",
                            state.processed * 2,
                            state.pairs.len() * 2
                        )
                    } else {
                        format!(
                            "Обработано {} из {} файлов",
                            state.processed * 2,
                            state.pairs.len() * 2
                        )
                    })
                    .child(if crate::i18n::english() {
                        format!("Reviewed: {}", state.processed * 2)
                    } else {
                        format!("Проверено: {}", state.processed * 2)
                    }),
            );

        let controls = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .px(px(20.0))
            .py(px(16.0))
            .flex_shrink_0()
            .bg(rgb(0x11111c))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(muted(crate::i18n::text(EXPLANATION)).max_w(px(950.0)))
            .child(muted(crate::i18n::text(DEMO)).text_size(px(11.0)).text_color(rgb(ACCENT)))
            .child(actions)
            .when_some(state.saved_at.clone().filter(|_| !state.running), |view, date| {
                view.child(muted(if crate::i18n::english() { format!("Saved search: {date}. Refresh to check new or changed indexed files.") } else { format!("Сохранённый поиск: {date}. Обновите для проверки новых или изменённых проиндексированных файлов.") }))
            })
            .child(scan_card)
            .when(!state.rejected.is_empty() || state.last_rejected.is_some(), |view| {
                view.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(icon("check").size(px(15.0)))
                        .child(muted(if crate::i18n::english() { format!("Content pairs excluded: {}", state.rejected.len()) } else { format!("Исключено пар по содержимому: {}", state.rejected.len()) }))
                        .child(div().flex_1())
                        .when(state.last_rejected.is_some(), |view| {
                            view.child(
                                action("duplicate-undo", crate::i18n::text("Отменить последнее исключение"), "reset", !state.running, false)
                                    .on_click(cx.listener(|this, _, _, cx| this.review_duplicate(true, cx))),
                            )
                        }),
                )
            })
            .when_some(state.message.clone(), |view, message| {
                view.child(muted(message).text_color(rgb(ACCENT)))
            });

        controls.child(
            div()
                .flex()
                .gap(px(4.0))
                .p(px(4.0))
                .w_auto()
                .items_start()
                .rounded(px(10.0))
                .border_1()
                .border_color(rgb(LINE))
                .bg(rgb(BACKGROUND))
                .children([true, false].into_iter().map(|exact| {
                    action(
                        if exact {
                            "duplicate-exact"
                        } else {
                            "duplicate-similar"
                        },
                        if exact {
                            crate::i18n::text("Точные совпадения")
                        } else {
                            crate::i18n::text("Похожие изображения")
                        },
                        if exact { "copy" } else { "image" },
                        true,
                        false,
                    )
                    .border_0()
                    .text_color(rgb(if state.exact == exact { TEXT } else { MUTED }))
                    .when(state.exact == exact, |view| view.bg(rgb(0x30223f)))
                    .child(
                        div()
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(5.0))
                            .bg(rgb(0x292332))
                            .text_size(px(11.0))
                            .child(state.count(exact).to_string()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.duplicates.select_mode(exact);
                        cx.notify();
                    }))
                })),
        )
    }

    pub(super) fn duplicates_body(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let state = &self.duplicates;
        let compact = f32::from(window.viewport_size().width) < 950.0;
        let count = state.count(state.exact);
        let results = div()
            .id("duplicate-results")
            .w(px(if compact { 190.0 } else { 280.0 }))
            .flex_shrink_0()
            .h_full()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&state.results_scroll)
            .p(px(12.0))
            .border_r_1()
            .border_color(rgb(LINE))
            .bg(rgb(0x11111c))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px_1()
                    .pt(px(5.0))
                    .pb(px(14.0))
                    .text_size(px(11.0))
                    .text_color(rgb(MUTED))
                    .child(crate::i18n::text("Пары совпавших файлов"))
                    .child(count.to_string()),
            )
            .when(count == 0, |view| {
                view.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_3()
                        .py(px(30.0))
                        .px(px(14.0))
                        .text_color(rgb(MUTED))
                        .child(
                            icon(if state.running {
                                "search"
                            } else {
                                "circle-check"
                            })
                            .size(px(30.0)),
                        )
                        .child(if state.running {
                            crate::i18n::text("Поиск…")
                        } else {
                            crate::i18n::text("В этой категории совпадений нет")
                        }),
                )
            })
            .children(
                state
                    .pairs
                    .iter()
                    .filter(|pair| pair.exact == state.exact && state.available(pair))
                    .map(|pair| {
                        let id = pair.id;
                        let active = state.selected == Some(id);
                        button(("duplicate-pair", id), "")
                            .w_full()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(6.0))
                            .p(px(12.0))
                            .mb(px(8.0))
                            .rounded(px(10.0))
                            .text_size(px(12.0))
                            .keyboard_focus(true)
                            .when(active, |view| {
                                view.bg(rgb(0x2a203d)).border_color(rgb(0x60477e))
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .text_size(px(10.0))
                                    .text_color(rgb(ACCENT))
                                    .child(
                                        icon(if pair.exact { "copy" } else { "image" })
                                            .size(px(14.0)),
                                    )
                                    .child(crate::i18n::text("Библиотека"))
                                    .child(icon("arrow").size(px(12.0)))
                                    .child(crate::i18n::text("Входящие")),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .child(pair.versions[0].entry.details.filename.clone()),
                            )
                            .child(muted(pair.versions[1].entry.details.filename.clone()))
                            .child(
                                div()
                                    .w_full()
                                    .border_t_1()
                                    .border_color(rgb(LINE))
                                    .pt(px(8.0))
                                    .text_size(px(10.0))
                                    .text_color(rgb(MUTED))
                                    .child(if pair.exact {
                                        crate::i18n::text("Точные совпадения").into()
                                    } else {
                                        if crate::i18n::english() {
                                            format!(
                                                "Similar images · Hash distance: {}/63",
                                                pair.distance
                                            )
                                        } else {
                                            format!(
                                                "Похожие изображения · Расстояние хешей: {}/63",
                                                pair.distance
                                            )
                                        }
                                    }),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.duplicates.selected = Some(id);
                                this.duplicates.confirmation = None;
                                this.duplicates
                                    .comparison_scroll
                                    .set_offset(gpui::point(px(0.0), px(0.0)));
                                cx.notify();
                            }))
                    }),
            );
        let comparison = div()
            .id("duplicate-comparison")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&state.comparison_scroll)
            .px(px(20.0))
            .py(px(16.0))
            .bg(rgb(BACKGROUND));
        let comparison = if let Some(pair) = state.selected_pair() {
            comparison
                .child(div().flex().gap(px(10.0)).p(px(14.0)).mb(px(16.0)).rounded(px(10.0)).border_1().border_color(rgb(LINE)).bg(rgb(SURFACE))
                    .child(icon(if pair.exact { "circle-check" } else { "info" }).size(px(18.0)).text_color(rgb(ACCENT)))
                    .child(muted(if pair.exact {
                        crate::i18n::text("Одинаковое содержимое по SHA-256. Каждая дополнительная копия сравнивается с первой в группе. Перед удалением проверьте метаданные.")
                    } else {
                        crate::i18n::text("Приблизительное визуальное совпадение, включая возможные версии арта. Это не оценка достоверности. Кадрирование, повороты и значительные правки могут не распознаться; возможны ложные совпадения.")
                    }).flex_1()))
                .child(div().flex().gap(px(20.0)).mb(px(20.0)).when(compact, |view| view.flex_col())
                    .children(pair.versions.iter().enumerate().map(|(side, version)| self.duplicate_version(version, side, compact, window, cx))))
                .child(div().flex().flex_wrap().justify_between().gap_3()
                    .when(!pair.exact, |view| view.child(action("duplicate-reject", crate::i18n::text("Не похоже"), "close", !state.running, false)
                        .on_click(cx.listener(|this, _, _, cx| this.review_duplicate(false, cx)))))
                    .child(action("duplicate-replace", crate::i18n::text("Заменить Main этой версией из Stage…"), "copy", !state.running, true)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if !this.duplicates.running {
                                this.duplicates.confirmation = Some(Action::Replace);
                                cx.notify();
                            }
                        }))))
                .when_some(state.confirmation, |view, action_kind| view.child(self.duplicate_confirmation(action_kind, cx)))
        } else {
            comparison.child(
                div()
                    .min_h_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .p(px(24.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(84.0))
                            .mb(px(20.0))
                            .rounded(px(24.0))
                            .border_1()
                            .border_color(rgb(0x49345f))
                            .bg(rgb(0x2a203d))
                            .text_color(rgb(ACCENT))
                            .child(icon("image").size(px(38.0))),
                    )
                    .child(
                        div()
                            .mb(px(10.0))
                            .text_size(px(18.0))
                            .font_weight(FontWeight::BOLD)
                            .child(crate::i18n::text("Сравните перед выбором")),
                    )
                    .child(
                        muted(crate::i18n::text(
                            "Выберите пару для сравнения превью, разрешения, размера и метаданных.",
                        ))
                        .max_w(px(380.0))
                        .line_height(px(20.4))
                        .mb(px(24.0)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .text_size(px(11.0))
                            .text_color(rgb(ACCENT))
                            .child(icon("circle-check").size(px(15.0)))
                            .child(crate::i18n::text("Вы решаете, какую версию оставить")),
                    ),
            )
        };

        div()
            .flex()
            .flex_1()
            .min_h_0()
            .child(results)
            .child(comparison)
    }

    fn duplicate_version(
        &self,
        version: &Version,
        side: usize,
        compact: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let entry = &version.entry;
        let enabled = !self.duplicates.running;
        let (width, height) = entry.details.resolution.unwrap_or_default();
        let size = entry
            .details
            .size
            .map(|bytes| {
                if crate::i18n::english() {
                    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
                } else {
                    format!("{:.1} МБ", bytes as f64 / 1_048_576.0)
                }
            })
            .unwrap_or_else(|| "—".into());
        let fields = [
            (
                crate::i18n::text("Художник"),
                entry.details.artist.clone().unwrap_or_else(|| "—".into()),
            ),
            (
                crate::i18n::text("Копирайт"),
                entry.details.copyrights.join(", "),
            ),
            (crate::i18n::text("Персонажи"), entry.characters.clone()),
            (crate::i18n::text("Теги"), entry.tags.join(", ")),
            (
                crate::i18n::text("Название видео"),
                if entry.video {
                    entry.title.clone()
                } else {
                    "—".into()
                },
            ),
            (
                crate::i18n::text("Путь"),
                entry.details.relative_path.clone(),
            ),
        ];
        div()
            .flex()
            .flex_col()
            .when(!compact, |view| view.flex_1())
            .min_w_0()
            .p(px(16.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(SURFACE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(if version.main {
                                crate::i18n::text("Библиотека")
                            } else {
                                crate::i18n::text("Входящие")
                            }),
                    )
                    .child(
                        div()
                            .px(px(6.0))
                            .py(px(3.0))
                            .rounded(px(5.0))
                            .border_1()
                            .border_color(rgb(LINE))
                            .text_size(px(10.0))
                            .text_color(rgb(MUTED))
                            .child(entry.extension.clone()),
                    ),
            )
            .child(
                div()
                    .h(px(
                        (f32::from(window.viewport_size().height) * 0.23).clamp(170.0, 280.0)
                    ))
                    .my(px(12.0))
                    .rounded(px(8.0))
                    .overflow_hidden()
                    .bg(rgb(BACKGROUND))
                    .when(self.show_previews, |view| {
                        view.child(
                            img(entry.path.clone())
                                .size_full()
                                .object_fit(ObjectFit::Contain),
                        )
                    })
                    .when(!self.show_previews, |view| {
                        view.flex()
                            .items_center()
                            .justify_center()
                            .child(icon("image").size(px(32.0)))
                    }),
            )
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .child(entry.details.filename.clone()),
            )
            .child(
                div()
                    .py(px(12.0))
                    .mb(px(12.0))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .text_size(px(11.0))
                    .text_color(rgb(ACCENT))
                    .child(format!("{width} × {height} · {size} · {}", entry.extension)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(fields.into_iter().map(|(label, value)| {
                        div()
                            .flex()
                            .gap(px(12.0))
                            .min_w_0()
                            .child(muted(label).w(px(80.0)).flex_shrink_0())
                            .child(div().flex_1().min_w_0().child(if value.is_empty() {
                                "—".into()
                            } else {
                                value
                            }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(10.0))
                    .mt(px(16.0))
                    .child(
                        action(
                            ("duplicate-open", side),
                            crate::i18n::text("Открыть оригинал"),
                            "arrow",
                            enabled,
                            false,
                        )
                        .text_size(px(11.0))
                        .flex_1()
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.open_duplicate_version(side, window, cx)
                            },
                        )),
                    )
                    .child(
                        action(
                            ("duplicate-delete", side),
                            crate::i18n::text("Удалить этот файл…"),
                            "trash",
                            enabled,
                            false,
                        )
                        .text_size(px(11.0))
                        .flex_1()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.duplicates.running {
                                this.duplicates.confirmation = Some(Action::Delete(side));
                                cx.notify();
                            }
                        })),
                    ),
            )
    }

    fn duplicate_confirmation(&self, action_kind: Action, cx: &mut Context<Self>) -> Div {
        let label = match action_kind {
            Action::Delete(side) => {
                if crate::i18n::english() {
                    format!(
                        "Remove demo version {} from the results?",
                        if side == 0 { "Main" } else { "Stage" }
                    )
                } else {
                    format!(
                        "Удалить демонстрационную версию {} из результатов?",
                        if side == 0 { "Main" } else { "Stage" }
                    )
                }
            }
            Action::Replace => {
                crate::i18n::text("Заменить демонстрационную версию Main версией Stage?").into()
            }
        };
        div().flex().flex_col().gap_3().p_4().mt_4().rounded_lg().border_1().border_color(rgb(0x60477e)).bg(rgb(SURFACE))
            .child(label).child(muted(crate::i18n::text("Действие применяется только к демонстрационным результатам. Реальные файлы и каталог не изменяются.")))
            .child(div().flex().gap_2()
                .child(action("duplicate-confirm", crate::i18n::text("Подтвердить"), "check", !self.duplicates.running, true)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.duplicates.confirm() {
                            tracing::info!(operation = "duplicate_action", storage = "demo_session", "Confirmed demo duplicate action completed");
                            cx.notify();
                        }
                    })))
                .child(action("duplicate-cancel", crate::i18n::text("Отмена"), "close", true, false)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.duplicates.confirmation = None;
                        cx.notify();
                    }))))
    }
}
