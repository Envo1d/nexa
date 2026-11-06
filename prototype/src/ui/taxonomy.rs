use gpui::{
    BoxShadow, Context, Div, Entity, Focusable, Subscription, UniformListScrollHandle, Window, div,
    point, prelude::*, px, uniform_list,
};
use gpui_component::input::{Input, InputEvent, InputState};

use super::{ACCENT, Gallery, LINE, MUTED, SURFACE, TEXT, button, shell::icon};
use crate::appearance::{rgb, rgba};
use crate::taxonomy::{Catalog, Category};

mod select;

pub(super) struct Editor {
    pub catalog: Catalog,
    pub search: Entity<InputState>,
    name: Entity<InputState>,
    copyright: String,
    category: Category,
    selected: Option<usize>,
    links: Option<usize>,
    repeated: bool,
    deleting: bool,
    error: Option<String>,
    list_scroll: UniformListScrollHandle,
    detail_scroll: gpui::ScrollHandle,
    select_open: bool,
    select_active: usize,
    select_focus: gpui::FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl Editor {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Gallery>) -> Self {
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder(crate::i18n::text("Поиск имён…")));
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(crate::i18n::text("Имя")));
        let search_subscription = cx.subscribe_in(&search, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.taxonomy
                    .list_scroll
                    .scroll_to_item(0, gpui::ScrollStrategy::Top);
                this.reset_taxonomy(window, cx);
            }
        });
        let name_subscription = cx.subscribe_in(&name, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.apply_taxonomy(window, cx);
            } else {
                cx.notify();
            }
        });

        Self {
            catalog: Catalog::default(),
            search,
            name,
            copyright: String::new(),
            category: Category::Copyright,
            selected: None,
            links: None,
            repeated: false,
            deleting: false,
            error: None,
            list_scroll: UniformListScrollHandle::new(),
            detail_scroll: gpui::ScrollHandle::new(),
            select_open: false,
            select_active: 0,
            select_focus: cx.focus_handle(),
            _subscriptions: vec![search_subscription, name_subscription],
        }
    }

    fn visible(&self, cx: &Context<Gallery>) -> Vec<usize> {
        self.catalog.filtered(
            self.category,
            &self.search.read(cx).value(),
            self.links,
            self.repeated,
        )
    }
}

fn field_surface(
    id: &'static str,
    input: &Entity<InputState>,
    cx: &Context<Gallery>,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .track_focus(&input.focus_handle(cx))
        .w_full()
        .h(px(36.0))
        .flex()
        .items_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(0x1a1827))
        .focus(|style| {
            style.border_color(rgb(ACCENT)).shadow(vec![BoxShadow {
                color: rgba(0xa786ff1c).into(),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(2.0),
            }])
        })
}

fn field_input(input: &Entity<InputState>) -> Input {
    Input::new(input)
        .appearance(false)
        .h(px(34.0))
        .line_height(px(15.0))
        .px(px(12.0))
        .py(px(9.5))
        .gap(px(8.0))
        .text_size(px(12.0))
        .text_color(rgb(TEXT))
}

fn field(input: &Entity<InputState>, cx: &Context<Gallery>) -> gpui::Stateful<Div> {
    field_surface("dictionary-name-field", input, cx).child(field_input(input))
}
impl Gallery {
    pub(super) fn refresh_taxonomy_locale(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.taxonomy.search.update(cx, |state, cx| {
            state.set_placeholder(crate::i18n::text("Поиск имён…"), window, cx);
        });
        self.taxonomy.name.update(cx, |state, cx| {
            state.set_placeholder(crate::i18n::text("Имя"), window, cx);
        });
        self.taxonomy.error = None;
    }

    pub(super) fn open_taxonomy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.dialog_open {
            return;
        }
        self.switch_workspace(false, window, cx);
        self.player = None;
        self.viewer = false;
        self.marquee = None;
        self.details_open = false;
        self.drawer_transition = super::motion::Transition::new(0.0, 220);
        self.sort_open = false;
        self.notice = None;
        self.taxonomy_open = true;
        self.taxonomy.catalog.refresh(
            self.entries
                .iter()
                .chain(self.inactive_workspace.entries.iter()),
        );
        self.reset_taxonomy(window, cx);
        self.focus.focus(window);
        tracing::info!(
            operation = "open_dictionary",
            records = self.taxonomy.catalog.records.len(),
            storage = "session",
            "Dictionary workspace opened"
        );
    }

    pub(super) fn reset_taxonomy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.taxonomy.visible(cx);
        if !self
            .taxonomy
            .selected
            .is_some_and(|id| visible.contains(&id))
        {
            self.taxonomy.selected = visible.first().copied();
        }
        let record = self
            .taxonomy
            .catalog
            .records
            .iter()
            .find(|r| Some(r.id) == self.taxonomy.selected);
        let name = record.map_or_else(String::new, |r| r.name.clone());
        self.taxonomy.copyright = record.map_or_else(String::new, |r| r.copyright.clone());
        self.taxonomy
            .name
            .update(cx, |state, cx| state.set_value(name, window, cx));
        self.taxonomy.deleting = false;
        self.taxonomy.select_open = false;
        self.taxonomy.error = None;
        self.taxonomy
            .detail_scroll
            .set_offset(gpui::point(px(0.0), px(0.0)));
        cx.notify();
    }

    fn apply_taxonomy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.taxonomy.selected else {
            return;
        };
        let name = self.taxonomy.name.read(cx).value().to_string();
        let copyright = self.taxonomy.copyright.clone();
        let replacement = (!self.taxonomy.deleting).then_some((name.as_str(), copyright.as_str()));
        let action = if replacement.is_some() {
            "edit"
        } else {
            "delete"
        };
        let span = tracing::info_span!(
            "dictionary_change",
            operation = "dictionary_change",
            action,
            storage = "session"
        );
        let _entered = span.enter();
        tracing::info!("Dictionary change started");
        match self.taxonomy.catalog.apply(
            id,
            replacement,
            &mut self.entries,
            &mut self.inactive_workspace.entries,
        ) {
            Ok(queued) => {
                if queued > 0 {
                    // Index-keyed presentation state cannot survive removal/reordering.
                    self.previews.clear();
                    self.inactive_workspace.previews.clear();
                    self.selected = None;
                    self.inactive_workspace.selected = None;
                    self.stage_selection.clear();
                    self.inactive_workspace.selection.clear();
                }
                self.tag_counts = crate::catalog::tag_counts(&self.entries);
                self.refilter(cx);
                self.reset_taxonomy(window, cx);
                self.notice = Some(if queued > 0 {
                    if crate::i18n::english() {
                        format!("Entry deleted. Files added to Incoming: {queued}.")
                    } else {
                        format!("Запись удалена. Файлов во Входящих: +{queued}.")
                    }
                } else if replacement.is_some() {
                    crate::i18n::text("Запись обновлена.").into()
                } else {
                    crate::i18n::text("Запись удалена.").into()
                });
                tracing::info!(queued, "Dictionary change completed");
            }
            Err(message) => {
                self.taxonomy.error = Some(message.into());
                tracing::warn!(reason = message, "Dictionary change rejected");
                cx.notify();
            }
        }
    }

    pub(super) fn taxonomy_workspace(&self, cx: &mut Context<Self>) -> Div {
        let editor = &self.taxonomy;
        let visible = editor.visible(cx);
        let category = editor.category;
        let total = editor
            .catalog
            .records
            .iter()
            .filter(|r| r.category == category)
            .count();
        let mut categories = div().flex().flex_wrap().gap(px(6.0));
        for (index, category) in Category::ALL.into_iter().enumerate() {
            let count = editor
                .catalog
                .records
                .iter()
                .filter(|r| r.category == category)
                .count();
            categories = categories.child(
                button(("dictionary-category", index), "")
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .px(px(13.0))
                    .h(px(40.0))
                    .bg(if category == editor.category {
                        rgb(0x242034)
                    } else {
                        crate::appearance::rgba(0)
                    })
                    .border_color(if category == editor.category {
                        rgb(LINE)
                    } else {
                        crate::appearance::rgba(0)
                    })
                    .text_color(rgb(if category == editor.category {
                        TEXT
                    } else {
                        MUTED
                    }))
                    .child(
                        icon(category.icon())
                            .size(px(17.0))
                            .text_color(rgb(category.color())),
                    )
                    .child(category.label())
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(MUTED))
                            .child(count.to_string()),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.taxonomy.category = category;
                        this.taxonomy.selected = None;
                        this.taxonomy
                            .list_scroll
                            .scroll_to_item(0, gpui::ScrollStrategy::Top);
                        this.reset_taxonomy(window, cx);
                    })),
            );
        }

        let mut filters = div()
            .flex()
            .items_center()
            .gap(px(3.0))
            .p(px(3.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(SURFACE));
        for (index, (value, label)) in [
            (None, crate::i18n::text("Все связи")),
            (Some(0), crate::i18n::text("0 связей")),
            (Some(1), crate::i18n::text("1 связь")),
        ]
        .into_iter()
        .enumerate()
        {
            filters = filters.child(
                super::motion::button(("dictionary-links", index))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .px(px(10.0))
                    .py(px(7.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(crate::appearance::rgba(0))
                    .text_size(px(12.0))
                    .line_height(px(15.0))
                    .bg(if editor.links == value {
                        rgb(0x242034)
                    } else {
                        crate::appearance::rgba(0)
                    })
                    .text_color(rgb(if editor.links == value {
                        0xd0b4fa
                    } else {
                        MUTED
                    }))
                    .when(editor.links != value, |view| {
                        view.hover_colors(
                            Some(rgba(0xeeedf60a).into()),
                            Some(rgb(TEXT).into()),
                            None,
                        )
                    })
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.taxonomy.links = value;
                        this.taxonomy
                            .list_scroll
                            .scroll_to_item(0, gpui::ScrollStrategy::Top);
                        this.reset_taxonomy(window, cx);
                    })),
            );
        }

        let row_ids = visible.clone();
        let list = uniform_list(
            "dictionary-rows",
            visible.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|index| this.taxonomy_row(row_ids[index], cx))
                    .collect()
            }),
        )
        .track_scroll(editor.list_scroll.clone())
        .size_full();
        let rows = div()
            .flex_1()
            .min_h_0()
            .p(px(2.0))
            .when(visible.is_empty(), |view| {
                view.child(
                    div()
                        .p_4()
                        .text_color(rgb(MUTED))
                        .child(crate::i18n::text("Нет подходящих записей.")),
                )
            })
            .when(!visible.is_empty(), |view| view.child(list));

        let detail = self.taxonomy_detail(cx);

        self.taxonomy_layout(categories, filters, rows, detail, total, cx)
    }

    fn taxonomy_row(&self, id: usize, cx: &mut Context<Self>) -> super::motion::MotionDiv {
        let editor = &self.taxonomy;
        let category = editor.category;
        let record = editor
            .catalog
            .records
            .iter()
            .find(|r| r.id == id)
            .expect("filtered record");
        super::motion::button(("dictionary-record", id))
            .relative()
            .flex()
            .items_center()
            .w_full()
            .h(px(66.0))
            .px(px(18.0))
            .py(px(13.0))
            .gap(px(13.0))
            .rounded(px(5.0))
            .border_t_1()
            .border_color(rgb(LINE))
            .cursor_pointer()
            .bg(if editor.selected == Some(id) {
                rgb(0x30263e)
            } else {
                crate::appearance::rgba(0)
            })
            .when(editor.selected == Some(id), |view| {
                view.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(3.0))
                        .rounded(px(3.0))
                        .bg(rgb(ACCENT)),
                )
            })
            .hover_colors(Some(rgb(0x242034).into()), None, None)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(34.0))
                    .flex_shrink_0()
                    .rounded(px(8.0))
                    .bg(rgb(0x242034))
                    .text_color(rgb(MUTED))
                    .child(
                        record
                            .name
                            .chars()
                            .next()
                            .unwrap_or(' ')
                            .to_uppercase()
                            .to_string(),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(record.name.clone()),
                    )
                    .child(div().text_size(px(11.0)).text_color(rgb(MUTED)).child(
                        if record.copyright.is_empty() {
                            category.singular().to_owned()
                        } else {
                            record.copyright.clone()
                        },
                    )),
            )
            .child(div().text_color(rgb(MUTED)).child(record.count.to_string()))
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.taxonomy.selected != Some(id) {
                    this.taxonomy.selected = Some(id);
                    this.reset_taxonomy(window, cx);
                }
            }))
    }

    fn taxonomy_detail(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let editor = &self.taxonomy;
        let category = editor.category;
        let selected = editor
            .catalog
            .records
            .iter()
            .find(|r| Some(r.id) == editor.selected);

        let mut detail = div()
            .id("dictionary-detail")
            .w(px(330.0))
            .flex_shrink_0()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&editor.detail_scroll)
            .border_l_1()
            .border_color(rgb(LINE))
            .p(px(24.0))
            .bg(rgb(0x10101a))
            .flex()
            .flex_col()
            .gap(px(18.0));
        if let Some(record) = selected {
            let dirty = editor.name.read(cx).value().as_ref() != record.name
                || (category == Category::Character && editor.copyright != record.copyright);
            let heading = div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .mb(px(6.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(11.0))
                        .text_color(rgb(category.color()))
                        .child(icon(category.icon()).size(px(16.0)))
                        .child(category.singular()),
                )
                .child(
                    div()
                        .text_size(px(20.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(record.name.clone()),
                )
                .when(!record.copyright.is_empty(), |view| {
                    view.child(div().text_color(rgb(MUTED)).child(record.copyright.clone()))
                });
            detail = detail.child(heading);
            if editor.deleting {
                detail = detail.child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(if crate::i18n::english() { format!("Delete “{}”?", record.name) } else { format!("Удалить «{}»?", record.name) }))
                    .child(div().text_color(rgb(MUTED)).child(if matches!(category, Category::Copyright | Category::Artist) {
                        crate::i18n::text("Связанные файлы будут перемещены во Входящие для повторного распределения. При удалении произведения удаляются и его персонажи. Файлы на диске не изменяются.")
                    } else { crate::i18n::text("Запись будет удалена из метаданных всех связанных файлов. Файлы на диске не изменяются.") }));
            } else {
                detail = detail.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(MUTED))
                                .child(crate::i18n::text("Имя")),
                        )
                        .child(field(&editor.name, cx)),
                );
                if category == Category::Character {
                    detail = detail.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(MUTED))
                                    .child(crate::i18n::text("Произведение")),
                            )
                            .child(self.copyright_select(cx)),
                    );
                }
                detail = detail.child(div().text_size(px(12.0)).text_color(rgb(MUTED)).child(if category == Category::Character {
                    crate::i18n::text("Смена произведения обновляет все связанные файлы. Предыдущее произведение удаляется, если его больше не использует ни один персонаж файла.")
                } else { crate::i18n::text("Изменения применяются ко всем связанным файлам.") }))
                    .when(dirty, |view| view.child(div().text_size(px(11.0)).text_color(rgb(ACCENT)).child(crate::i18n::text("Несохранённые изменения"))));
            }
            let can_save =
                editor.deleting || (dirty && !editor.name.read(cx).value().trim().is_empty());
            detail = detail
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .justify_end()
                        .gap_2()
                        .child(
                            button("dictionary-cancel", crate::i18n::text("Отмена"))
                                .text_size(px(11.0))
                                .when(!dirty && !editor.deleting, |v| v.opacity(0.45))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.reset_taxonomy(window, cx)
                                })),
                        )
                        .child(
                            button(
                                "dictionary-save",
                                if editor.deleting {
                                    crate::i18n::text("Удалить запись")
                                } else {
                                    crate::i18n::text("Сохранить изменения")
                                },
                            )
                            .text_size(px(11.0))
                            .bg(rgb(if editor.deleting { 0x8e354b } else { 0x6839ce }))
                            .when(!can_save, |v| v.opacity(0.45))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    if can_save {
                                        this.apply_taxonomy(window, cx);
                                    }
                                },
                            )),
                        ),
                )
                .child(
                    div()
                        .border_t_1()
                        .border_color(rgb(LINE))
                        .pt_3()
                        .text_color(rgb(MUTED))
                        .child(if crate::i18n::english() {
                            format!("Linked files: {}", record.count)
                        } else {
                            format!("Связанных файлов: {}", record.count)
                        }),
                )
                .when(!editor.deleting, |view| {
                    view.child(
                        button("dictionary-delete", "")
                            .flex()
                            .items_center()
                            .gap_2()
                            .w(px(130.0))
                            .text_size(px(11.0))
                            .child(icon("trash").size(px(15.0)))
                            .child(crate::i18n::text("Удалить запись"))
                            .text_color(rgb(0xf292a7))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.reset_taxonomy(window, cx);
                                this.taxonomy.deleting = true;
                                cx.notify();
                            })),
                    )
                });
            if category == Category::Copyright {
                let related: Vec<_> = editor
                    .catalog
                    .records
                    .iter()
                    .filter(|r| r.category == Category::Character && r.copyright == record.name)
                    .collect();
                detail = detail.child(div().border_t_1().border_color(rgb(LINE)).pt_3().child(
                    if crate::i18n::english() {
                        format!("Characters · {}", related.len())
                    } else {
                        format!("Персонажи · {}", related.len())
                    },
                ));
                for character in related {
                    let id = character.id;
                    detail = detail.child(
                        button(
                            ("dictionary-related", id),
                            &format!("{} · {}", character.name, character.count),
                        )
                        .w(px(145.0))
                        .text_size(px(11.0))
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            icon("person")
                                .size(px(14.0))
                                .text_color(rgb(Category::Character.color())),
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.taxonomy.category = Category::Character;
                                this.taxonomy.links = None;
                                this.taxonomy.repeated = false;
                                this.taxonomy
                                    .search
                                    .update(cx, |state, cx| state.set_value("", window, cx));
                                this.taxonomy.selected = Some(id);
                                this.reset_taxonomy(window, cx);
                            },
                        )),
                    );
                }
            }
        } else {
            detail = detail.child(div().text_color(rgb(MUTED)).child(crate::i18n::text(
                "Выберите запись, чтобы изменить её данные.",
            )));
        }

        detail
    }

    fn taxonomy_layout(
        &self,
        categories: Div,
        filters: Div,
        rows: Div,
        detail: gpui::Stateful<Div>,
        total: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let editor = &self.taxonomy;
        let category = editor.category;
        let visible = editor.visible(cx).len();

        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .line_height(px(16.25))
            .gap(px(18.0))
            .px(px(32.0))
            .py(px(26.0))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(15.0))
                            .text_color(rgb(MUTED))
                            .mb_1()
                            .child(crate::i18n::text("Метаданные коллекции")),
                    )
                    .child(
                        div()
                            .text_size(px(24.0))
                            .line_height(px(30.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(crate::i18n::text("Справочники")),
                    )
                    .child(div().text_color(rgb(MUTED)).child(crate::i18n::text(
                        "Выберите запись слева, чтобы изменить её данные.",
                    ))),
            )
            .child(categories.flex_shrink_0())
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(14.0))
                    .flex_shrink_0()
                    .child(
                        field_surface("dictionary-search-field", &editor.search, cx)
                            .w(px(280.0))
                            .mb(px(8.0))
                            .child(
                                field_input(&editor.search)
                                    .prefix(icon("search").size(px(17.0)).text_color(rgb(MUTED)))
                                    .when(!editor.search.read(cx).value().is_empty(), |input| {
                                        input.suffix(
                                            super::motion::button("dictionary-clear-search")
                                                .cursor_pointer()
                                                .child(icon("close").size(px(16.0)))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.taxonomy.search.update(cx, |state, cx| {
                                                        state.set_value("", window, cx);
                                                        state.focus(window, cx);
                                                    });
                                                })),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(10.0))
                            .child(filters)
                            .when(category == Category::Character, |view| {
                                view.child(
                                    super::motion::button("dictionary-repeated")
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .gap(px(8.0))
                                        .px(px(10.0))
                                        .py(px(7.0))
                                        .border_1()
                                        .border_color(rgba(0))
                                        .rounded(px(6.0))
                                        .cursor_pointer()
                                        .text_size(px(12.0))
                                        .line_height(px(15.0))
                                        .text_color(rgb(if editor.repeated {
                                            0xd0b4fa
                                        } else {
                                            MUTED
                                        }))
                                        .bg(if editor.repeated {
                                            rgb(0x242034)
                                        } else {
                                            crate::appearance::rgba(0)
                                        })
                                        .when(!editor.repeated, |view| {
                                            view.hover_colors(
                                                Some(rgba(0xeeedf60a).into()),
                                                Some(rgb(TEXT).into()),
                                                None,
                                            )
                                        })
                                        .child(
                                            div()
                                                .size(px(15.0))
                                                .flex_shrink_0()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .border_1()
                                                .rounded(px(4.0))
                                                .border_color(rgb(if editor.repeated {
                                                    ACCENT
                                                } else {
                                                    MUTED
                                                }))
                                                .when(editor.repeated, |view| {
                                                    view.bg(rgb(0x7b43d9))
                                                        .text_color(rgb(0xffffff))
                                                        .child(
                                                            gpui_component::Icon::new(
                                                                gpui_component::IconName::Check,
                                                            )
                                                            .size(px(12.0)),
                                                        )
                                                }),
                                        )
                                        .child(crate::i18n::text(
                                            "Одинаковые имена в разных произведениях",
                                        ))
                                        .child(
                                            div()
                                                .text_color(rgb(MUTED))
                                                .text_size(px(11.0))
                                                .child(editor.catalog.repeated().len().to_string()),
                                        )
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.taxonomy.repeated = !this.taxonomy.repeated;
                                            this.reset_taxonomy(window, cx);
                                        })),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(11.0))
                    .text_color(rgb(MUTED))
                    .child(format!("{} · {} / {total}", category.label(), visible))
                    .when(category == Category::Character, |view| {
                        view.child(crate::i18n::text("Имена уточняются произведением."))
                    }),
            )
            .when_some(editor.error.clone(), |view, error| {
                view.child(div().text_color(rgb(0xf09aa9)).child(error))
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .border_1()
                    .border_color(rgb(LINE))
                    .rounded(px(12.0))
                    .overflow_hidden()
                    .bg(rgb(SURFACE))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .rounded_tl(px(11.0))
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_shrink_0()
                                    .justify_between()
                                    .px(px(20.0))
                                    .py(px(13.0))
                                    .text_size(px(11.0))
                                    .text_color(rgb(MUTED))
                                    .line_height(px(13.75))
                                    .bg(rgb(0x10101a))
                                    .border_b_1()
                                    .border_color(rgb(LINE))
                                    .child(crate::i18n::text("Имя"))
                                    .child(crate::i18n::text("Связанные файлы")),
                            )
                            .child(rows),
                    )
                    .child(detail),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(MUTED))
                    .child(crate::i18n::text(
                        "Демобиблиотека · Изменения действуют до конца сеанса.",
                    )),
            )
    }
}
