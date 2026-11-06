use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use gpui::{Context, Div, RenderImage, UniformListScrollHandle, Window, div, prelude::*, px};

use super::{Gallery, LINE, MUTED, SURFACE, TEXT, shell::control};
use crate::appearance::rgb;
use crate::catalog::{self, Entry, SortMode};

pub(super) struct Workspace {
    pub(super) entries: Vec<Entry>,
    pub(super) previews: HashMap<usize, Arc<RenderImage>>,
    query: String,
    pub(super) selected: Option<usize>,
    pub(super) selection: BTreeSet<usize>,
    videos_only: bool,
    images_only: bool,
    sort: SortMode,
    scroll: UniformListScrollHandle,
}

impl Workspace {
    pub(super) fn incoming() -> Self {
        Self {
            entries: catalog::incoming_demo_entries(),
            previews: HashMap::new(),
            query: String::new(),
            selected: None,
            selection: BTreeSet::new(),
            videos_only: false,
            images_only: false,
            sort: SortMode::Newest,
            scroll: UniformListScrollHandle::new(),
        }
    }
}

impl Gallery {
    pub(super) fn switch_workspace(
        &mut self,
        stage: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading || self.dialog_open {
            return;
        }
        self.settings_open = false;
        self.duplicates_open = false;
        self.taxonomy_open = false;
        self.health_open = false;
        if self.stage == stage {
            cx.notify();
            return;
        }

        self.player = None;
        self.marquee = None;
        self.viewer = false;
        self.notice = None;
        self.sort_open = false;
        self.details_motion = None;
        let mut query = self.search.read(cx).value().to_string();
        let saved = &mut self.inactive_workspace;
        std::mem::swap(&mut self.entries, &mut saved.entries);
        std::mem::swap(&mut self.previews, &mut saved.previews);
        std::mem::swap(&mut query, &mut saved.query);
        std::mem::swap(&mut self.selected, &mut saved.selected);
        std::mem::swap(&mut self.stage_selection, &mut saved.selection);
        std::mem::swap(&mut self.videos_only, &mut saved.videos_only);
        std::mem::swap(&mut self.images_only, &mut saved.images_only);
        std::mem::swap(&mut self.sort, &mut saved.sort);
        std::mem::swap(&mut self.scroll, &mut saved.scroll);
        self.stage = stage;
        self.tag_counts = catalog::tag_counts(&self.entries);
        self.search.update(cx, |state, cx| {
            state.set_placeholder(
                if stage {
                    crate::i18n::text("Поиск по имени файла или пути")
                } else {
                    crate::i18n::text("Метаданные через точку с запятой")
                },
                window,
                cx,
            );
            state.set_value(query, window, cx);
        });
        self.focus.focus(window);
        self.refilter(cx);
        tracing::info!(
            operation = "switch_workspace",
            workspace = if stage { "stage" } else { "main" },
            records = self.entries.len(),
            "Isolated workspace opened"
        );
    }

    pub(super) fn select_card(&mut self, index: usize, modifiers: gpui::Modifiers) {
        if self.stage {
            if modifiers.shift {
                let anchor = self
                    .selected
                    .and_then(|index| self.visible.iter().position(|&item| item == index));
                let end = self.visible.iter().position(|&item| item == index);
                if let (Some(start), Some(end)) = (anchor, end) {
                    if !modifiers.control && !modifiers.platform {
                        self.stage_selection.clear();
                    }
                    self.stage_selection
                        .extend(&self.visible[start.min(end)..=start.max(end)]);
                } else {
                    self.stage_selection.insert(index);
                }
            } else if modifiers.control || modifiers.platform {
                if !self.stage_selection.remove(&index) {
                    self.stage_selection.insert(index);
                }
            } else {
                self.stage_selection.clear();
                self.stage_selection.insert(index);
            }
            self.selected = if self.stage_selection.contains(&index) {
                Some(index)
            } else {
                self.stage_selection.last().copied()
            };
        } else {
            self.selected = Some(index);
        }
    }

    pub(super) fn stage_sidebar(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let count = self.stage_selection.len();

        let summary = div()
            .flex()
            .flex_col()
            .px_4()
            .pt(px(16.0))
            .pb(px(24.0))
            .gap_3()
            .text_color(rgb(MUTED))
            .child(
                div()
                    .text_size(px(32.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT))
                    .child(self.entries.len().to_string()),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .child(crate::i18n::text("файлов ждут распределения")),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .text_size(px(12.0))
                    .line_height(px(19.2))
                    .child(crate::i18n::text(
                        "Выберите файлы, чтобы назначить метаданные и перенести их в библиотеку.",
                    )),
            );
        let previews = super::filters::toggle(
            "stage-previews",
            crate::i18n::text("Показывать превью"),
            self.show_previews,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.show_previews = !this.show_previews;
            cx.notify();
        }));
        let reset = control(
            "stage-reset",
            crate::i18n::text("Сбросить фильтры"),
            "reset",
            false,
        )
        .justify_center()
        .text_size(px(11.0))
        .on_click(cx.listener(|this, _, window, cx| {
            this.videos_only = false;
            this.images_only = false;
            this.search
                .update(cx, |state, cx| state.set_value("", window, cx));
            this.refilter(cx);
        }));

        div()
            .flex()
            .flex_col()
            .min_h_0()
            .w(px(super::layout::filter_width(f32::from(
                window.viewport_size().width,
            ))))
            .flex_shrink_0()
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(LINE))
            .bg(gpui::linear_gradient(
                120.0,
                gpui::linear_color_stop(rgb(SURFACE), 0.0),
                gpui::linear_color_stop(rgb(0x10101a), 1.0),
            ))
            .child(
                div()
                    .h(px(52.0))
                    .flex_shrink_0()
                    .p_4()
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(crate::i18n::text("Очередь входящих")),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(summary)
                    .child(div().px_4().py_4().child(previews)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p_4()
                    .gap_2()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(
                        self.distribution_button(
                            "stage-distribute",
                            &(if crate::i18n::english() {
                                format!("Distribute selected ({count})")
                            } else {
                                format!("Распределить выбранные ({count})")
                            }),
                            cx,
                        )
                        .w_full()
                        .h_auto()
                        .min_h(px(36.0))
                        .whitespace_normal(),
                    )
                    .child(reset),
            )
    }

    pub(super) fn stage_selection_bar(&self, cx: &mut Context<Self>) -> Div {
        let count = self.stage_selection.len();

        div()
            .flex()
            .items_center()
            .h(px(56.0))
            .flex_shrink_0()
            .mb_3()
            .gap(px(10.0))
            .border_y_1()
            .border_color(rgb(LINE))
            .text_size(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(rgb(MUTED))
                    .child(if count == 0 {
                        crate::i18n::text("Выберите файлы для распределения").to_owned()
                    } else {
                        if crate::i18n::english() {
                            format!("Selected: {count}")
                        } else {
                            format!("Выбрано: {count}")
                        }
                    }),
            )
            .child(
                super::button("stage-clear", crate::i18n::text("Снять выделение"))
                    .opacity(if count == 0 { 0.45 } else { 1.0 })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.stage_selection.clear();
                        this.selected = None;
                        cx.notify();
                    })),
            )
            .child(self.distribution_button(
                "stage-distribute-bar",
                crate::i18n::text("Распределить"),
                cx,
            ))
    }

    pub(super) fn distribution_button(
        &self,
        id: &'static str,
        label: &str,
        cx: &mut Context<Self>,
    ) -> super::motion::MotionDiv {
        control(id, label, "staging", true)
            .text_size(px(12.0))
            .justify_center()
            .opacity(if self.stage_selection.is_empty() {
                0.45
            } else {
                1.0
            })
            .on_click(cx.listener(|this, _, window, cx| this.open_distribution(window, cx)))
    }
}
