use gpui::{Context, Div, FontWeight, div, prelude::*, px, rgb};

use super::presentation::{action, badge, label, row, small};
use super::{Gallery, LINE, SURFACE, icon, text};

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
                .child(label(
                    if section == 0 {
                        "libraryPath"
                    } else {
                        "stagingPath"
                    },
                    "notSelected",
                ))
                .child(
                    action(("settings-browse", section), "browse", Some("folder")).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.settings_preview_action("browse", cx)
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
                small(concat!(
                    "В этом сеансе сканирование не выполнялось · ",
                    "Файлов: 0 · Обработано: 0 · Проблем: 0"
                ))
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
                                .child("Только в настольном приложении / 512 МБ"),
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

    pub(super) fn settings_catalog_card(&self) -> Div {
        let card = card();

        card.child(self.settings_disabled_action(
            "catalogBackup",
            "catalogBackupDescription",
            "catalogCreateBackup",
            false,
        ))
        .child(self.settings_disabled_action(
            "catalogExport",
            "catalogExportDescription",
            "catalogExport",
            false,
        ))
        .child(self.settings_disabled_action(
            "catalogRestore",
            "catalogRestoreDescription",
            "catalogChooseBackup",
            false,
        ))
        .child(
            div()
                .px(px(20.0))
                .py(px(16.0))
                .child(text("catalogNativeOnly")),
        )
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
        let card = card();

        card.child(
            row(false)
                .child(
                    div()
                        .flex_1()
                        .child(
                            div()
                                .line_height(px(17.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(text("version")),
                        )
                        .child(small("Nexa 2.16.2").mt(px(6.0))),
                )
                .child(badge("Не проверено", false)),
        )
        .child(self.settings_toggle(4, "autoUpdate", "autoUpdateDescription", false, cx))
        .child(self.settings_action_row(
            "checkNow",
            "checkDescription",
            "checkNow",
            Some("reset"),
            true,
            cx,
        ))
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

    pub(super) fn settings_disabled_action(
        &self,
        title: &str,
        description: &str,
        button: &'static str,
        last: bool,
    ) -> Div {
        row(last)
            .child(label(title, description))
            .child(action(button, button, None).opacity(0.35).cursor_default())
    }
}
