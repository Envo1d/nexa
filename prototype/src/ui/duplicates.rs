use std::{collections::BTreeSet, time::Duration};

use gpui::{Context, Div, Window, div, prelude::*, px, rgb};

use super::{Gallery, LINE, SURFACE, TEXT, motion};
use crate::catalog::Entry;

mod presentation;

#[cfg(test)]
mod tests;

#[derive(Clone)]
struct Version {
    entry: Entry,
    main: bool,
}

#[derive(Clone)]
struct Pair {
    id: usize,
    versions: [Version; 2],
    exact: bool,
    distance: u8,
}

#[derive(Clone, Copy)]
enum Action {
    Delete(usize),
    Replace,
}

struct ViewerReturn {
    pair: usize,
    entries: Vec<Entry>,
    visible: Vec<usize>,
    selected: Option<usize>,
    tag_counts: Vec<(String, usize)>,
}

pub(super) struct Duplicates {
    pairs: Vec<Pair>,
    selected: Option<usize>,
    exact: bool,
    rejected: BTreeSet<usize>,
    last_rejected: Option<usize>,
    removed: BTreeSet<usize>,
    processed: usize,
    running: bool,
    stopped: bool,
    saved_at: Option<String>,
    confirmation: Option<Action>,
    message: Option<String>,
    results_scroll: gpui::ScrollHandle,
    comparison_scroll: gpui::ScrollHandle,
    viewer_return: Option<ViewerReturn>,
}

impl Duplicates {
    pub(super) fn new() -> Self {
        Self {
            pairs: demo_pairs(),
            selected: None,
            exact: true,
            rejected: BTreeSet::new(),
            last_rejected: None,
            removed: BTreeSet::new(),
            processed: 0,
            running: false,
            stopped: false,
            saved_at: None,
            confirmation: None,
            message: None,
            results_scroll: gpui::ScrollHandle::new(),
            comparison_scroll: gpui::ScrollHandle::new(),
            viewer_return: None,
        }
    }

    fn available(&self, pair: &Pair) -> bool {
        pair.id < self.processed
            && !self.rejected.contains(&pair.id)
            && !self.removed.contains(&pair.id)
    }

    fn count(&self, exact: bool) -> usize {
        self.pairs
            .iter()
            .filter(|pair| pair.exact == exact && self.available(pair))
            .count()
    }

    fn selected_pair(&self) -> Option<&Pair> {
        self.pairs
            .iter()
            .find(|pair| Some(pair.id) == self.selected && self.available(pair))
    }

    fn select_mode(&mut self, exact: bool) {
        self.exact = exact;
        self.selected = None;
        self.confirmation = None;
        self.results_scroll
            .set_offset(gpui::point(px(0.0), px(0.0)));
    }

    fn reject(&mut self) -> bool {
        if self.running {
            return false;
        }

        let Some(pair) = self.selected_pair().filter(|pair| !pair.exact) else {
            return false;
        };
        let id = pair.id;

        self.rejected.insert(id);
        self.last_rejected = Some(id);
        self.selected = None;
        self.confirmation = None;

        true
    }

    fn undo(&mut self) -> bool {
        if self.running {
            return false;
        }

        let Some(id) = self.last_rejected.take() else {
            return false;
        };

        self.rejected.remove(&id);
        self.select_mode(false);
        self.selected = Some(id);

        true
    }

    fn confirm(&mut self) -> bool {
        if self.running || self.selected_pair().is_none() {
            return false;
        }

        let Some(action) = self.confirmation.take() else {
            return false;
        };
        let id = self.selected.expect("selected pair validated");

        self.removed.insert(id);
        self.selected = None;
        self.message = Some(match action {
            Action::Delete(_) => "Демонстрационный файл удалён из результатов. Реальные файлы не изменены.",
            Action::Replace => "Демонстрационная версия Main заменена. Реальные файлы и метаданные не изменены.",
        }.into());

        true
    }
}

fn demo_pairs() -> Vec<Pair> {
    let entries = crate::catalog::demo_entries();
    [
        (0, 0, true, 0),
        (2, 5, false, 6),
        (3, 3, true, 0),
        (6, 8, false, 10),
    ]
    .into_iter()
    .enumerate()
    .map(|(id, (left, right, exact, distance))| {
        let versions = [left, right]
            .into_iter()
            .enumerate()
            .map(|(side, index)| {
                let mut entry = entries[index].clone();
                entry.extension = "WEBP".into();
                entry.details.filename = format!("demo-{}-{}.webp", id + 1, side + 1);
                entry.details.relative_path = format!(
                    "Demo/{}/{}",
                    if side == 0 { "Main" } else { "Stage" },
                    entry.details.filename
                );
                Version {
                    entry,
                    main: side == 0,
                }
            })
            .collect::<Vec<_>>();
        Pair {
            id,
            versions: versions.try_into().ok().expect("two versions"),
            exact,
            distance,
        }
    })
    .collect()
}

impl Gallery {
    pub(super) fn cancel_duplicate_confirmation(&mut self) {
        self.duplicates.confirmation = None;
    }

    pub(super) fn open_duplicates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.dialog_open {
            return;
        }

        self.switch_workspace(false, window, cx);
        self.duplicates_open = true;
        self.player = None;
        self.viewer = false;
        self.marquee = None;
        self.details_open = false;
        self.drawer_transition = motion::Transition::new(0.0, 220);
        self.sort_open = false;
        self.notice = None;
        self.focus.focus(window);
        tracing::info!(
            operation = "open_duplicates",
            storage = "demo_session",
            "Duplicate search workspace opened"
        );
        cx.notify();
    }

    fn scan_duplicates(&mut self, cx: &mut Context<Self>) {
        if self.duplicates.running {
            return;
        }

        let state = &mut self.duplicates;
        state.running = true;
        state.stopped = false;
        state.processed = 0;
        state.selected = None;
        state.confirmation = None;
        state.saved_at = None;
        state.message = None;
        state.last_rejected = None;
        tracing::info!(
            operation = "duplicate_scan",
            storage = "demo_session",
            "Demo duplicate search started"
        );
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(350))
                    .await;
                let keep_running = this.update(cx, |this, cx| {
                    let state = &mut this.duplicates;
                    state.processed += 1;
                    let done = state.stopped || state.processed == state.pairs.len();
                    if done {
                        state.running = false;
                        state.saved_at =
                            Some(chrono::Local::now().format("%d.%m.%Y %H:%M").to_string());
                        tracing::info!(
                            operation = "duplicate_scan",
                            stopped = state.stopped,
                            processed = state.processed,
                            "Demo duplicate search completed"
                        );
                    }
                    cx.notify();
                    !done
                });
                if !matches!(keep_running, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn review_duplicate(&mut self, undo: bool, cx: &mut Context<Self>) {
        let changed = if undo {
            self.duplicates.undo()
        } else {
            self.duplicates.reject()
        };
        if changed {
            tracing::info!(
                operation = "duplicate_review",
                storage = "demo_session",
                undo,
                "Demo pair review updated"
            );
            cx.notify();
        }
    }

    fn open_duplicate_version(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.duplicates.running {
            return;
        }
        let Some(pair) = self.duplicates.selected_pair() else {
            return;
        };
        let pair_id = pair.id;
        let entries = pair
            .versions
            .iter()
            .map(|version| version.entry.clone())
            .collect();
        self.duplicates.viewer_return = Some(ViewerReturn {
            pair: pair_id,
            entries: std::mem::replace(&mut self.entries, entries),
            visible: std::mem::replace(&mut self.visible, vec![0, 1]),
            selected: self.selected.replace(side),
            tag_counts: std::mem::take(&mut self.tag_counts),
        });
        self.tag_counts = crate::catalog::tag_counts(&self.entries);
        self.open_viewer(window, cx);
    }

    pub(super) fn restore_duplicate_viewer(&mut self) {
        let Some(saved) = self.duplicates.viewer_return.take() else {
            return;
        };

        if let Some(pair) = self
            .duplicates
            .pairs
            .iter_mut()
            .find(|pair| pair.id == saved.pair)
        {
            for (version, entry) in pair.versions.iter_mut().zip(&self.entries) {
                version.entry = entry.clone();
            }
        }

        self.entries = saved.entries;
        self.visible = saved.visible;
        self.selected = saved.selected;
        self.tag_counts = saved.tag_counts;
    }

    pub(super) fn duplicates_workspace(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .rounded(px(10.0))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(SURFACE))
            .text_color(rgb(TEXT))
            .text_size(px(12.0))
            .child(self.duplicates_header())
            .child(self.duplicates_controls(cx))
            .child(self.duplicates_body(window, cx))
    }
}
