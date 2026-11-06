use gpui::{Context, Div, Window, div, prelude::*, px};

use super::{Gallery, LINE, MUTED, SURFACE, TEXT, shell::icon};
use crate::appearance::rgb;

const LABELS: [&str; 4] = [
    "Отсутствующие файлы",
    "Незавершённые операции",
    "Ошибки превью",
    "Неполные метаданные",
];
const ICONS: [&str; 4] = ["warning", "git-branch", "image", "info"];
const PATHS: [&str; 4] = [
    r"C:\Media\missing-image.png",
    "",
    r"C:\Media\preview-image.webp",
    r"C:\Media\untitled-video.mp4",
];
const REASONS: [&str; 4] = [
    "Запись каталога сохранена, но файла по прежнему пути нет.",
    "Перемещение файла не завершено.",
    "Превью недоступно.",
    "Технические метаданные неполны или не прочитаны.",
];
const HINTS: [&str; 4] = [
    "Пути, которые больше не доступны. Теги и избранное сохранены.",
    "Незавершённые перемещения, очистка после замены, ошибки синхронизации и недоступные источники по последней проверке.",
    "Последние ошибки превью для максимум 1 024 файлов в текущем сеансе. Прошлые сеансы не учитываются.",
    "Недоступные технические свойства или незаполненные автор, копирайт либо название видео в библиотеке.",
];
const DEMO: &str = "Демонстрационные данные. Исправления действуют только на этом экране и не затрагивают реальные файлы.";

struct PreviewReturn {
    entries: Vec<crate::catalog::Entry>,
    visible: Vec<usize>,
    selected: Option<usize>,
    tag_counts: Vec<(String, usize)>,
}

pub(super) struct Health {
    preview_return: Option<PreviewReturn>,
    pub(super) metadata_entry: crate::catalog::Entry,
    category: usize,
    remaining: [bool; 4],
    proposal: bool,
    confirmed: bool,
    message: Option<String>,
}

impl Health {
    pub(super) fn new() -> Self {
        let mut metadata_entry = crate::catalog::demo_entries().remove(1);
        metadata_entry.details.filename = "untitled-video.mp4".into();
        metadata_entry.details.relative_path = PATHS[3].into();
        metadata_entry.details.artist = Some("Demo artist".into());
        metadata_entry.details.copyrights = vec!["Demo collection".into()];
        metadata_entry.details.character_copyrights.clear();
        metadata_entry.characters.clear();
        metadata_entry.tags = vec!["favorite".into(), "landscape".into()];
        metadata_entry.title.clear();

        Self {
            metadata_entry,
            preview_return: None,
            category: 0,
            remaining: [true; 4],
            proposal: false,
            confirmed: false,
            message: None,
        }
    }

    pub(super) fn metadata_saved(&mut self) {
        self.remaining[3] = false;
        self.message = Some(crate::i18n::text("Метаданные обновлены на время демосеанса.").into());
    }

    fn choose(&mut self, category: usize) {
        self.category = category;
        self.proposal = false;
        self.confirmed = false;
        self.message = None;
    }

    fn can_repair(&self) -> bool {
        let relink_confirmed = self.category != 0 || (self.proposal && self.confirmed);

        self.remaining[self.category] && relink_confirmed
    }

    fn repair(&mut self) {
        if !self.can_repair() {
            return;
        }

        self.remaining[self.category] = false;
        self.proposal = false;
        self.confirmed = false;
        self.message = Some(
            crate::i18n::text(
                "Демонстрационная проблема устранена. Реальные файлы и каталог не изменены.",
            )
            .into(),
        );
    }
}

fn muted(text: impl Into<String>) -> Div {
    div()
        .text_color(rgb(MUTED))
        .line_height(px(19.5))
        .child(text.into())
}

fn panel() -> Div {
    div()
        .min_w_0()
        .min_h_0()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(SURFACE))
}

fn empty() -> Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .p_4()
        .gap_3()
        .child(icon("circle-check").size(px(40.0)))
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(gpui::FontWeight::BOLD)
                .child(crate::i18n::text("В этой категории проблем нет")),
        )
        .child(muted(crate::i18n::text(
            "Обновите список после исправлений или проверьте файлы на внешние изменения.",
        )))
}

impl Gallery {
    pub(super) fn refresh_health_locale(&mut self) {
        self.health.message = None;
    }

    pub(super) fn open_health_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.health.preview_return = Some(PreviewReturn {
            entries: std::mem::replace(&mut self.entries, vec![self.health.metadata_entry.clone()]),
            visible: std::mem::replace(&mut self.visible, vec![0]),
            selected: self.selected.replace(0),
            tag_counts: std::mem::take(&mut self.tag_counts),
        });
        self.tag_counts = crate::catalog::tag_counts(&self.entries);
        self.open_viewer(window, cx);
    }

    pub(super) fn restore_health_preview(&mut self) {
        if let Some(saved) = self.health.preview_return.take() {
            self.health.metadata_entry = self.entries[0].clone();
            self.entries = saved.entries;
            self.visible = saved.visible;
            self.selected = saved.selected;
            self.tag_counts = saved.tag_counts;
        }
    }

    pub(super) fn open_health(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.dialog_open {
            return;
        }
        self.switch_workspace(false, window, cx);
        self.health_open = true;
        self.player = None;
        self.viewer = false;
        self.marquee = None;
        self.details_open = false;
        self.drawer_transition = super::motion::Transition::new(0.0, 220);
        self.sort_open = false;
        self.notice = None;
        self.focus.focus(window);
        tracing::info!(
            operation = "open_health",
            storage = "demo_session",
            "Library health workspace opened"
        );
        cx.notify();
    }

    fn repair_health(&mut self, cx: &mut Context<Self>) {
        let category = self.health.category;
        if !self.health.can_repair() {
            return;
        }
        tracing::info!(
            operation = "health_repair",
            category,
            storage = "demo_session",
            "Demo repair started"
        );
        self.health.repair();
        tracing::info!(
            operation = "health_repair",
            category,
            storage = "demo_session",
            "Demo repair completed"
        );
        cx.notify();
    }

    pub(super) fn health_workspace(&self, cx: &mut Context<Self>) -> Div {
        let category = self.health.category;
        let present = self.health.remaining[category];
        let total = self.health.remaining.iter().filter(|&&value| value).count();
        let filename = PATHS[category]
            .rsplit('\\')
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or(crate::i18n::text("Восстановление файловой операции"));
        let mut detail = panel()
            .id("health-detail")
            .flex()
            .flex_col()
            .w(gpui::relative(0.390244))
            .flex_shrink_0()
            .min_w(px(290.0))
            .p_4()
            .overflow_y_scroll();

        if present {
            detail = detail
                .child(muted(crate::i18n::text("Подробности проблемы")).text_size(px(12.0)))
                .child(
                    div()
                        .mt_3()
                        .mb_2()
                        .text_size(px(16.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(filename.to_owned()),
                )
                .child(muted(PATHS[category]).my(px(5.0)).text_size(px(12.0)))
                .child(
                    div()
                        .py_3()
                        .text_color(rgb(0xefbc79))
                        .flex()
                        .gap_2()
                        .child(icon("warning").size(px(18.0)))
                        .child(crate::i18n::text(REASONS[category])),
                );
            if category != 1 {
                detail = detail.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .py_4()
                        .border_y_1()
                        .border_color(rgb(LINE))
                        .children(
                            [
                                (crate::i18n::text("Автор"), "Demo artist"),
                                (crate::i18n::text("Копирайты"), "Demo collection"),
                                (crate::i18n::text("Персонажи"), "—"),
                                (crate::i18n::text("Теги"), "favorite; landscape"),
                                (crate::i18n::text("Название видео"), "—"),
                                (crate::i18n::text("Избранное"), crate::i18n::text("Да")),
                            ]
                            .into_iter()
                            .map(|(label, value)| {
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(muted(label).line_height(px(16.5)).text_size(px(11.0)))
                                    .child(value)
                            }),
                        ),
                );
            }
            detail = detail.child(self.health_repairs(cx));

            if self.health.proposal {
                detail = detail.child(
                    panel()
                        .mt_4()
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(crate::i18n::text("Восстановление связи с файлом"))
                        .child(muted(r"C:\Media\Found\restored-image.png"))
                        .child(muted(crate::i18n::text(
                            "Демонстрационная привязка не перемещает и не удаляет файлы.",
                        )))
                        .child(
                            button(
                                "health-confirm",
                                if self.health.confirmed {
                                    crate::i18n::text("☑ Подтверждаю выбор файла")
                                } else {
                                    crate::i18n::text("☐ Подтверждаю выбор файла")
                                },
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.health.confirmed = !this.health.confirmed;
                                cx.notify();
                            })),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    button("health-cancel", crate::i18n::text("Отмена")).on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.health.proposal = false;
                                            this.health.confirmed = false;
                                            cx.notify();
                                        }),
                                    ),
                                )
                                .child(
                                    button(
                                        "health-relink",
                                        crate::i18n::text("Восстановить связь"),
                                    )
                                    .opacity(if self.health.confirmed { 1.0 } else { 0.4 })
                                    .on_click(cx.listener(|this, _, _, cx| this.repair_health(cx))),
                                ),
                        ),
                );
            }
        } else {
            detail = detail.child(div().flex_1().flex().items_center().justify_center().child(
                muted(crate::i18n::text(
                    "Выберите проблему, чтобы посмотреть разметку и доступные исправления.",
                )),
            ));
        }

        self.health_layout(detail, filename, present, total, cx)
    }
}

#[cfg(test)]
mod tests;

mod presentation;

use presentation::button;
