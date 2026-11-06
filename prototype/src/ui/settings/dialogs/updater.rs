use std::{path::PathBuf, time::Duration};

use gpui::{
    App, AppContext, Bounds, Context, FontWeight, Render, Window, WindowBounds, WindowOptions, div,
    img, prelude::*, px, size,
};

use super::super::super::{ACCENT, LINE, MUTED, TEXT, motion};
use crate::appearance::rgb;

struct Updater {
    tick: usize,
}

pub(super) fn open(cx: &mut App) -> Result<(), String> {
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(460.), px(440.)),
                cx,
            ))),
            window_min_size: Some(size(px(400.), px(440.))),
            titlebar: None,
            ..Default::default()
        },
        |_, cx| {
            cx.new(|cx: &mut Context<Updater>| {
                cx.spawn(async |this, cx| {
                    for tick in 1..=100 {
                        cx.background_executor()
                            .timer(Duration::from_millis(80))
                            .await;
                        if this
                            .update(cx, |this, cx| {
                                this.tick = tick;
                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    tracing::info!(
                        operation = "update_preview",
                        mode = "simulation",
                        "Updater presentation completed without installation"
                    );
                })
                .detach();
                Updater { tick: 0 }
            })
        },
    )
    .map_err(|_| {
        crate::i18n::text("Не удалось открыть окно предпросмотра обновления.").to_owned()
    })?;
    tracing::info!(
        operation = "update_preview",
        mode = "simulation",
        "Updater preview window opened"
    );

    Ok(())
}

impl Render for Updater {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let phase = match self.tick {
            0..=14 => 0,
            15..=39 => 1,
            40..=79 => 2,
            _ => 3,
        };
        let title = [
            crate::i18n::text("Подготовка"),
            crate::i18n::text("Проверка обновления"),
            crate::i18n::text("Обновление Nexa"),
            crate::i18n::text("Всё готово"),
        ][phase];
        let message = [
            crate::i18n::text("Подготовка обновления…"),
            crate::i18n::text("Проверка пакета обновления…"),
            crate::i18n::text("Установка обновления…"),
            crate::i18n::text("Предпросмотр завершён. Приложение не изменялось."),
        ][phase];
        let step = match phase {
            0 | 1 => 0,
            2 => 1,
            _ => 2,
        };
        let mut steps = div().flex().justify_between().text_size(px(11.));
        for (index, label) in [
            crate::i18n::text("Проверка"),
            crate::i18n::text("Установка"),
            crate::i18n::text("Открыть Nexa"),
        ]
        .iter()
        .enumerate()
        {
            steps = steps.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_color(rgb(if index <= step { ACCENT } else { MUTED }))
                    .child(
                        div()
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(if index <= step { ACCENT } else { MUTED }))
                            .child(if index < step {
                                "✓".to_owned()
                            } else {
                                (index + 1).to_string()
                            }),
                    )
                    .child(*label),
            );
        }
        div()
            .id("updater-window")
            .size_full()
            .overflow_y_scroll()
            .px(px(32.))
            .pb(px(24.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(0x15131f))
            .text_color(rgb(TEXT))
            .font_family("Segoe UI")
            .child(
                div()
                    .h(px(44.))
                    .mx(px(-16.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.start_window_move()
                    })
                    .child(crate::i18n::text("Nexa · Предпросмотр обновления"))
                    .child(
                        motion::button("close-updater")
                            .keyboard_focus(true)
                            .size(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .child("×")
                            .on_click(cx.listener(|_, _, window, _| window.remove_window())),
                    ),
            )
            .child(
                div()
                    .size(px(132.))
                    .mx_auto()
                    .mt(px(16.))
                    .mb(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        img(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../public/icon.png"))
                            .size(px(68.)),
                    ),
            )
            .child(
                div()
                    .text_center()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .mb(px(8.))
                    .child(crate::i18n::text("Версия 2.16.3 · демонстрация")),
            )
            .child(
                div()
                    .text_center()
                    .text_size(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .mb(px(8.))
                    .child(title),
            )
            .child(
                div()
                    .text_center()
                    .min_h(px(38.))
                    .text_size(px(12.))
                    .line_height(px(19.2))
                    .text_color(rgb(MUTED))
                    .child(message),
            )
            .child(
                div()
                    .h(px(4.))
                    .mt(px(24.))
                    .mb(px(16.))
                    .rounded(px(4.))
                    .overflow_hidden()
                    .bg(rgb(0x242034))
                    .child(
                        div()
                            .h_full()
                            .w(px(150.))
                            .ml(px(((self.tick % 25) as f32 / 25.) * 300. - 100.))
                            .bg(rgb(ACCENT)),
                    ),
            )
            .child(steps)
            .child(
                div()
                    .text_center()
                    .mt(px(20.))
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(crate::i18n::text(
                        "Симуляция: загрузка и перезапуск не выполняются.",
                    )),
            )
    }
}
