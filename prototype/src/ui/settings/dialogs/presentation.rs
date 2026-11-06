use std::path::PathBuf;

use gpui::{Context, Div, FontWeight, Window, div, img, prelude::*, px};

use super::super::super::{ACCENT, Gallery, LINE, TEXT, motion, shell::icon};
use super::super::presentation::{action, small};
use crate::appearance::{rgb, rgba};

impl Gallery {
    pub(in crate::ui) fn settings_update_dialog(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let workflow = &self.settings.workflow;
        let heading = div()
            .flex()
            .items_center()
            .justify_between()
            .py(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(40.))
                            .border_1()
                            .border_color(rgb(0x6d5aa4))
                            .rounded(px(10.))
                            .bg(rgba(0xa78bfa14))
                            .child(icon("reset").size(px(22.)).text_color(rgb(ACCENT))),
                    )
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(crate::i18n::text("Установить обновление?")),
                    ),
            )
            .child(action("update-confirm-close", "", Some("close")).on_click(
                cx.listener(|this, _, window, cx| this.settings_close_modal(window, cx)),
            ));
        let release = div()
            .flex()
            .items_center()
            .gap(px(14.))
            .mt(px(12.))
            .mb(px(18.))
            .p(px(16.))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(9.))
            .bg(rgba(0xa78bfa08))
            .child(
                img(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../public/icon.png"))
                    .size(px(38.)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(small(crate::i18n::text("Новая версия · демонстрация")))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Nexa 2.16.3"),
                    ),
            );
        let warning = div()
            .flex()
            .items_start()
            .gap(px(9.))
            .mt(px(16.))
            .p(px(12.))
            .rounded(px(7.))
            .bg(rgba(0xa78bfa0f))
            .text_size(px(11.))
            .line_height(px(17.6))
            .child(icon("info").size(px(18.)).flex_shrink_0())
            .child(crate::i18n::text(
                "Сохраните незавершённые изменения перед продолжением.",
            ));
        let footer = div()
            .flex()
            .justify_end()
            .gap(px(8.))
            .pt(px(23.))
            .child(
                action("update-confirm-cancel", "cancel", None)
                    .with_focus(&workflow.cancel)
                    .px(px(18.))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.settings_close_modal(window, cx)),
                    ),
            )
            .child(
                action("update-confirm-install", "", Some("check"))
                    .with_focus(&workflow.install)
                    .px(px(18.))
                    .bg(rgb(0x7b43d9))
                    .text_color(rgb(0xffffff))
                    .child(crate::i18n::text("Установить обновление"))
                    .on_click(
                        cx.listener(|this, _, window, cx| {
                            this.settings_install_preview(window, cx)
                        }),
                    ),
            );
        let panel = div()
            .id("settings-update-confirm")
            .occlude()
            .track_focus(&workflow.focus)
            .w(px((f32::from(window.viewport_size().width) - 40.).min(460.)))
            .max_h(window.viewport_size().height * 0.85)
            .overflow_y_scroll()
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(13.))
            .pt(px(6.))
            .px(px(20.))
            .pb(px(20.))
            .bg(gpui::linear_gradient(
                130.,
                gpui::linear_color_stop(rgb(0x1a1827), 0.),
                gpui::linear_color_stop(rgb(0x15131f), 1.),
            ))
            .child(heading)
            .child(release)
            .child(
                small(crate::i18n::text("Nexa загрузит и проверит обновление, затем закроется для установки. После завершения приложение откроется автоматически."))
                .line_height(px(20.4)),
            )
            .child(warning)
            .child(
                small(crate::i18n::text("Предпросмотр GPUI: загрузка, установка и перезапуск имитируются. Файлы приложения не изменяются."))
                .mt(px(12.)),
            )
            .child(footer)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    if this.settings.workflow.install.is_focused(window) {
                        this.settings_install_preview(window, cx);
                    } else {
                        this.settings_close_modal(window, cx);
                    }
                }
            }));

        div()
            .absolute()
            .inset_0()
            .size_full()
            .id("settings-update-backdrop")
            .occlude()
            .bg(rgba(0x080610b8))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| this.settings_close_modal(window, cx)),
            )
            .child(panel.on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation()))
    }

    pub(in crate::ui) fn settings_restore_preview(&self, cx: &mut Context<Self>) -> Div {
        let Some(preview) = &self.settings.workflow.preview else {
            return div();
        };
        let count = preview.snapshot.library.len() + preview.snapshot.incoming.len();
        let stamp = chrono::DateTime::from_timestamp_millis(preview.snapshot.created_at)
            .map(|stamp| {
                stamp
                    .with_timezone(&chrono::Local)
                    .format("%d.%m.%Y %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|| "—".into());
        let mut panel = div()
            .p(px(20.))
            .border_t_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if crate::i18n::english() {
                        format!("Backup contains {count} files")
                    } else {
                        format!("В копии: {count} файлов")
                    }),
            )
            .child(small(stamp).my(px(12.)))
            .child(small(preview.path.display().to_string()).my(px(12.)));

        for root in preview.snapshot.roots.iter().flatten() {
            panel = panel.child(small(format!("• {root}")).my(px(6.)));
        }

        let confirmation = motion::button("catalog-confirm")
            .keyboard_focus(true)
            .flex()
            .items_start()
            .gap(px(10.))
            .cursor_pointer()
            .text_color(rgb(TEXT))
            .child(
                div()
                    .size(px(14.))
                    .mt(px(4.))
                    .border_1()
                    .border_color(rgb(ACCENT))
                    .rounded(px(3.))
                    .when(self.settings.workflow.confirmed, |d| {
                        d.bg(rgb(ACCENT)).child(icon("check").size(px(12.)))
                    }),
            )
            .child(crate::i18n::text(
                "Я понимаю, что текущие записи сеанса будут заменены.",
            ))
            .on_click(cx.listener(|this, _, _, cx| {
                if !this.dialog_open {
                    this.settings.workflow.confirmed = !this.settings.workflow.confirmed;
                    cx.notify();
                }
            }));
        let footer = div()
            .flex()
            .justify_end()
            .flex_wrap()
            .gap(px(12.))
            .mt(px(20.))
            .child(
                action("catalog-cancel-restore", "cancel", None).on_click(cx.listener(
                    |this, _, _, cx| {
                        if !this.dialog_open {
                            this.settings.workflow.preview = None;
                            this.settings.workflow.confirmed = false;
                            cx.notify();
                        }
                    },
                )),
            )
            .child(
                action("catalog-restore-confirm", "catalogRestore", None)
                    .when(!self.settings.workflow.confirmed || self.dialog_open, |d| {
                        d.opacity(0.4).cursor_default()
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.settings_restore(cx))),
            );

        panel.child(small(crate::i18n::text("Восстановление заменяет записи Библиотеки и Входящих только в текущем сеансе GPUI. Настройки, словарь и исходные файлы не восстанавливаются. Перед заменой создаётся защитная копия.")).my(px(12.)))
            .child(confirmation)
            .child(footer)
    }
}
