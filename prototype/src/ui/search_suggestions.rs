use std::collections::BTreeMap;

use gpui::{
    BoxShadow, Context, KeyDownEvent, MouseButton, Window, div, point, prelude::*, px, rgb, rgba,
};

use super::{Gallery, MUTED, TEXT, motion};
use crate::{
    naming,
    taxonomy::{Catalog, Category, Record},
};

#[derive(Default)]
pub(super) struct Suggestions {
    pub(super) open: bool,
    pub(super) active: Option<usize>,
    pub(super) catalog: Catalog,
    pub(super) applied: String,
    scroll: gpui::ScrollHandle,
}

impl Suggestions {
    pub(super) fn collection_query(&self, draft: &str, stage: bool) -> String {
        if stage {
            draft.to_owned()
        } else {
            self.applied.clone()
        }
    }
}

pub(super) fn matches(query: &str, records: &[Record]) -> Vec<Record> {
    let mut parts: Vec<_> = query.split(';').map(str::trim).collect();
    let query = parts.pop().unwrap_or_default().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }

    let existing: Vec<_> = parts.iter().map(|s| s.to_lowercase()).collect();
    let mut unique = BTreeMap::new();
    for record in records {
        let name = record.name.to_lowercase();
        if name.contains(&query) && !existing.contains(&name) {
            unique.insert(
                (record.category, name, record.copyright.clone()),
                record.clone(),
            );
        }
    }

    let mut values: Vec<_> = unique.into_values().collect();
    values.sort_by(|a, b| {
        let rank = |r: &Record| !r.name.to_lowercase().starts_with(&query);
        rank(a)
            .cmp(&rank(b))
            .then_with(|| naming::compare_names(&a.name, &b.name))
            .then_with(|| naming::compare_names(&a.copyright, &b.copyright))
    });
    values.truncate(30);
    values
}

pub(super) fn choose(query: &str, name: &str) -> String {
    let mut parts: Vec<_> = query.split(';').collect();
    parts.pop();
    parts.push(name);
    parts
        .into_iter()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

impl Gallery {
    pub(super) fn commit_search_suggestion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.stage {
            return;
        }
        let options = matches(
            &self.search.read(cx).value(),
            &self.search_suggestions.catalog.records,
        );
        if self.search_suggestions.open
            && let Some(record) = self.search_suggestions.active.and_then(|i| options.get(i))
        {
            self.choose_search_suggestion(&record.name, window, cx);
        } else {
            self.search_suggestions.applied = self.search.read(cx).value().to_string();
            self.search_suggestions.open = false;
            self.search_suggestions.active = None;
            self.refilter(cx);
        }
    }

    fn choose_search_suggestion(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = choose(&self.search.read(cx).value(), name);
        self.search_suggestions.applied = query.clone();
        self.search
            .update(cx, |s, cx| s.set_value(query, window, cx));
        // InputState emits Change asynchronously; close after that event has settled.
        let entity = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = entity.update(cx, |this, cx| {
                this.search_suggestions.open = false;
                this.search_suggestions.active = None;
                this.refilter(cx);
                tracing::debug!(
                    operation = "library_search_suggestion",
                    results = this.visible.len(),
                    "Search suggestion applied"
                );
            });
        });
    }

    pub(super) fn search_suggestion_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.stage {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" && self.search_suggestions.open {
            self.search_suggestions.open = false;
            self.search_suggestions.active = None;
        } else if matches!(key, "up" | "down") {
            let count = matches(
                &self.search.read(cx).value(),
                &self.search_suggestions.catalog.records,
            )
            .len();
            self.search_suggestions.open = true;
            self.search_suggestions.active =
                next_index(self.search_suggestions.active, count, key == "down");
            if let Some(index) = self.search_suggestions.active {
                self.search_suggestions.scroll.scroll_to_item(index);
            }
        } else if key == "enter" {
            self.commit_search_suggestion(window, cx);
        } else {
            return;
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn search_suggestion_popup(&self, cx: &mut Context<Self>) -> Option<gpui::Deferred> {
        let options = matches(
            &self.search.read(cx).value(),
            &self.search_suggestions.catalog.records,
        );
        if options.is_empty() {
            return None;
        }

        Some(
            gpui::deferred(
                div()
                    .id("library-search-suggestions")
                    .absolute()
                    .top(px(44.))
                    .left_0()
                    .w_full()
                    .occlude()
                    .max_h(px(260.))
                    .overflow_y_scroll()
                    .track_scroll(&self.search_suggestions.scroll)
                    .p(px(5.))
                    .border_1()
                    .border_color(rgb(0x6d5aa4))
                    .rounded(px(8.))
                    .bg(rgb(0x1a1927))
                    .shadow(vec![BoxShadow {
                        color: rgba(0x0000004d).into(),
                        offset: point(px(0.), px(8.)),
                        blur_radius: px(24.),
                        spread_radius: px(0.),
                    }])
                    .children(options.into_iter().enumerate().map(|(index, record)| {
                        let label = if record.category == Category::Character
                            && !record.copyright.is_empty()
                        {
                            format!("{} ({})", record.name, record.copyright)
                        } else {
                            record.name.clone()
                        };
                        motion::button(("library-search-option", index))
                            .keyboard_focus(false)
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.))
                            .p(px(8.))
                            .rounded(px(5.))
                            .text_size(px(11.))
                            .line_height(px(13.75))
                            .text_color(rgb(TEXT))
                            .when(self.search_suggestions.active == Some(index), |d| {
                                d.bg(rgb(0x372f56))
                            })
                            .hover_colors(Some(rgb(0x372f56).into()), None, None)
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.prevent_default()
                            })
                            .child(div().flex_1().min_w_0().child(label))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(9.))
                                    .text_color(rgb(MUTED))
                                    .child(record.category.singular()),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.choose_search_suggestion(&record.name, window, cx)
                            }))
                    })),
            )
            .with_priority(20),
        )
    }
}

pub(super) fn next_index(active: Option<usize>, count: usize, down: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match active {
        None => {
            if down {
                0
            } else {
                count - 1
            }
        }
        Some(i) => {
            if down {
                (i + 1) % count
            } else {
                (i + count - 1) % count
            }
        }
    })
}

#[cfg(test)]
mod tests;
