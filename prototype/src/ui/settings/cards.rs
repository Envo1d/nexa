use gpui::{Context, Div, FontWeight, div, prelude::*, px};

use super::presentation::{action, badge, label, row, small};
use super::{Gallery, LINE, SURFACE, icon, text};
use crate::appearance::rgb;

fn card() -> Div {
    div()
        .px(px(16.0))
        .border_1()
        .border_color(rgb(LINE))
        .rounded(px(8.0))
        .bg(rgb(SURFACE))
}

impl Gallery {
    pub(super) fn settings_path_card(&self, section: usize, cx: &mut Context<Self>) -> Div {
        let card = card();

        card.child(
            row(true)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(text(
                            if section == 0 {
                                "libraryPath"
                            } else {
                                "stagingPath"
                            },
                        )))
                        .child(
                            small(
                                self.settings.paths[section]
                                    .as_deref()
                                    .unwrap_or(text("notSelected")),
                            )
                            .mt(px(6.)),
                        ),
                )
                .child(
                    action(("settings-browse", section), "browse", Some("folder")).on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.settings_browse(section, window, cx)
                        }),
                    ),
                ),
        )
    }

    pub(super) fn settings_structure_card(&self, width: f32, cx: &mut Context<Self>) -> Div {
        let card = card();

        card.child(self.settings_toggle(0, "copyrightDirectory", "copyrightDescription", false, cx))
            .child(self.settings_toggle(1, "artistDirectory", "artistDescription", false, cx))
            .child(self.settings_field(0, "characterSeparator", "separatorDescription", width, cx))
            .child(self.settings_field(1, "videoFolder", "videoFolderDescription", width, cx))
            .child(self.settings_field(2, "gifFolder", "gifDescription", width, cx))
            .child(self.settings_toggle(2, "sync", "syncDescription", true, cx))
    }

    pub(super) fn settings_indexing_card(&self, cx: &mut Context<Self>) -> Div {
        let actions = row(false)
            .flex_wrap()
            .justify_start()
            .gap(px(8.0))
            .child(
                action("settings-scan-library", "scanLibrary", Some("scan")).on_click(
                    cx.listener(|this, _, _, cx| this.settings_preview_action("scanLibrary", cx)),
                ),
            )
            .child(
                action("settings-scan-stage", "scanStage", Some("search")).on_click(
                    cx.listener(|this, _, _, cx| this.settings_preview_action("scanStage", cx)),
                ),
            );
        let summary = div()
            .p(px(12.0))
            .mb(px(16.0))
            .border_1()
            .border_color(rgb(0x374f72))
            .rounded(px(8.0))
            .bg(rgb(0x152b46))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .line_height(px(19.0))
                    .child(icon("info").size(px(16.0)).text_color(rgb(0x8bbbff)))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(text("lastScan")),
                    )
                    .child(badge(text("idle"), true)),
            )
            .child(
                small(crate::i18n::text("В этом сеансе сканирование не выполнялось · Файлов: 0 · Обработано: 0 · Проблем: 0"))
                .line_height(px(17.0))
                .mt(px(8.0)),
            );

        card()
            .child(actions)
            .child(self.settings_toggle(3, "autoScan", "autoScanDescription", false, cx))
            .child(summary)
    }

    pub(super) fn settings_cache_card(&self, cx: &mut Context<Self>) -> Div {
        let card = card();

        card.child(
            div()
                .py(px(16.0))
                .border_b_1()
                .border_color(rgb(LINE))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(16.0))
                        .child(label("thumbnailCache", "cacheLimit"))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(crate::i18n::text(
                                    "Только в настольном приложении / 512 МБ",
                                )),
                        ),
                )
                .child(
                    div()
                        .mt(px(16.0))
                        .h(px(6.0))
                        .rounded_full()
                        .bg(rgb(0x30263e)),
                ),
        )
        .child(self.settings_action_row(
            "clearCache",
            "clearDescription",
            "clearCache",
            Some("reset"),
            true,
            cx,
        ))
    }

    pub(super) fn settings_catalog_card(&self, cx: &mut Context<Self>) -> Div {
        let mut content = card();
        for (index, (title, description, button)) in [
            (
                "catalogBackup",
                "catalogBackupDescription",
                "catalogCreateBackup",
            ),
            ("catalogExport", "catalogExportDescription", "catalogExport"),
            (
                "catalogRestore",
                "catalogRestoreDescription",
                "catalogChooseBackup",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            content = content.child(
                row(false).child(label(title, description)).child(
                    action(("catalog-action", index), button, None)
                        .when(self.dialog_open, |d| d.opacity(0.4).cursor_default())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if index == 2 {
                                this.settings_choose_backup(window, cx);
                            } else {
                                this.settings_export(index == 1, window, cx);
                            }
                        })),
                ),
            );
        }
        content.child(self.settings_restore_preview(cx)).child(small(
            crate::i18n::text("Копии сеанса GPUI: Библиотека и Входящие. Производственные JSON-копии Tauri, словарь, настройки и исходные файлы не включены. Экспорт метаданных не подходит для восстановления.")
        ).py(px(16.)).px(px(20.)))
    }
    pub(super) fn settings_logs_card(&self, cx: &mut Context<Self>) -> Div {
        let card = card();

        card.child(self.settings_action_row(
            "applicationLogs",
            "logsMissing",
            "openLogs",
            Some("folder"),
            true,
            cx,
        ))
    }

    pub(super) fn settings_updates_card(&self, cx: &mut Context<Self>) -> Div {
        use super::dialogs::Update;
        let status = match self.settings.workflow.update {
            Update::Unchecked => crate::i18n::text("Не проверено"),
            Update::Checking => crate::i18n::text("Проверка обновлений…"),
            Update::Available => crate::i18n::text("Доступна Nexa 2.16.3 · демонстрация"),
            Update::Complete => crate::i18n::text("Предпросмотр установки открыт"),
        };
        card()
            .child(
                row(false)
                    .child(
                        div()
                            .flex_1()
                            .child(label("version", ""))
                            .child(small("Nexa 2.16.2").mt(px(6.))),
                    )
                    .child(badge(status, false)),
            )
            .child(self.settings_toggle(4, "autoUpdate", "autoUpdateDescription", false, cx))
            .child(
                row(true)
                    .child(label("checkNow", "checkDescription"))
                    .child(
                        action("settings-check-update", "checkNow", Some("reset"))
                            .when(self.settings.workflow.update == Update::Checking, |d| {
                                d.opacity(0.4).cursor_default()
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.settings_check_update(cx))),
                    ),
            )
            .when(self.settings.workflow.update == Update::Available, |d| {
                d.child(
                    div()
                        .pb(px(16.))
                        .child(
                            small(crate::i18n::text(
                                "Демонстрационное обновление. Установщик и сеть не используются.",
                            ))
                            .mb(px(12.)),
                        )
                        .child(
                            action("settings-install-update", "", Some("reset"))
                                .child(crate::i18n::text("Установить Nexa 2.16.3"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.settings_confirm_update(window, cx)
                                })),
                        ),
                )
            })
    }
    pub(super) fn settings_appearance_card(&self, cx: &mut Context<Self>) -> Div {
        let card = card();

        card.child(
            row(false)
                .child(label("language", "languageDescription"))
                .child(self.settings_selector(false, cx)),
        )
        .child(
            row(true)
                .child(label("theme", "themeDescription"))
                .child(self.settings_selector(true, cx)),
        )
    }
}
