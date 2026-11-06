//! File-action adapters. Destructive workflows only mutate isolated session records.

use std::{collections::BTreeSet, path::PathBuf};

use gpui::{AppContext, ClipboardItem, Context, Entity, FocusHandle, Window};
use gpui_component::input::InputState;

use super::Gallery;
use crate::catalog::Entry;

mod native;
mod presentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Open,
    Reveal,
    Share,
    CopyName,
    Delete,
    Replace,
    Order,
}

pub(super) struct Draft {
    action: Action,
    indices: Vec<usize>,
    paths: Vec<PathBuf>,
    stage: bool,
    order: Vec<usize>,
    target: Option<usize>,
    acknowledged: bool,
    query: Entity<InputState>,
    applied_query: String,
    suggestions_open: bool,
    suggestion_active: Option<usize>,
    catalog: crate::taxonomy::Catalog,
    backdrop: Option<std::sync::Arc<gpui::RenderImage>>,
    preview_return: Option<(Vec<Entry>, Vec<usize>, Option<usize>)>,
    _subscription: gpui::Subscription,
    pub(super) focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    dragging: Option<usize>,
    candidate_limit: usize,
}

fn selection(index: usize, stage: bool, selected: &BTreeSet<usize>, count: usize) -> Vec<usize> {
    if index >= count {
        return Vec::new();
    }
    if stage && selected.contains(&index) {
        selected.iter().copied().filter(|&i| i < count).collect()
    } else {
        vec![index]
    }
}

fn replace_content(main: &mut Entry, source: &Entry) {
    main.path = source.path.clone();
    main.video = source.video;
    main.demo = source.demo;
    main.extension = source.extension.clone();
    main.duration = source.duration.clone();
    main.modified = source.modified;
    main.date_label = source.date_label.clone();
    main.details.size = source.details.size;
    main.details.resolution = source.details.resolution;
    main.details.created_label = source.details.created_label.clone();
}

impl Gallery {
    pub(super) fn run_file_action(
        &mut self,
        index: usize,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_action_busy || self.file_operation.is_some() || self.loading {
            return;
        }
        let indices = selection(index, self.stage, &self.stage_selection, self.entries.len());
        if indices.is_empty() || indices.len() > 500 {
            self.notice = Some(crate::i18n::text("Выберите от 1 до 500 файлов.").into());
            cx.notify();
            return;
        }
        if action == Action::CopyName {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.entries[index].details.filename.clone(),
            ));
            self.notice = Some(crate::i18n::text("Имя файла скопировано.").into());
            cx.notify();
            return;
        }
        if matches!(action, Action::Delete | Action::Replace | Action::Order) {
            if action == Action::Replace && (!self.stage || indices.len() != 1) {
                return;
            }
            let indices = if action == Action::Order {
                order_group(&self.entries, index)
            } else {
                indices
            };
            let query = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(crate::i18n::text("Метаданные через точку с запятой"))
            });
            let subscription = cx.subscribe_in(
                &query,
                window,
                |this: &mut Self, query, event: &gpui_component::input::InputEvent, window, cx| {
                    if matches!(
                        event,
                        gpui_component::input::InputEvent::Change
                            | gpui_component::input::InputEvent::Focus
                    ) && let Some(draft) = this.file_operation.as_mut()
                    {
                        draft.suggestions_open =
                            query.read(cx).value().as_ref() != draft.applied_query;
                        draft.suggestion_active = None;
                    }
                    let options = this
                        .file_operation
                        .as_ref()
                        .map(|d| {
                            super::search_suggestions::matches(
                                &query.read(cx).value(),
                                &d.catalog.records,
                            )
                        })
                        .unwrap_or_default();
                    if matches!(event, gpui_component::input::InputEvent::PressEnter { .. })
                        && let Some(draft) = this.file_operation.as_mut()
                    {
                        draft.applied_query = if draft.suggestions_open {
                            draft
                                .suggestion_active
                                .and_then(|i| options.get(i))
                                .map(|r| {
                                    super::search_suggestions::choose(
                                        &query.read(cx).value(),
                                        &r.name,
                                    )
                                })
                        } else {
                            None
                        }
                        .unwrap_or_else(|| query.read(cx).value().to_string());
                        draft.suggestions_open = false;
                        draft.target = None;
                        draft.acknowledged = false;
                        draft.candidate_limit = 100;
                        let value = draft.applied_query.clone();
                        query.update(cx, |input, cx| input.set_value(value, window, cx));
                    }

                    cx.notify();
                },
            );
            let focus = cx.focus_handle();
            let mut catalog = crate::taxonomy::Catalog::default();
            catalog.refresh(self.inactive_workspace.entries.iter());
            catalog
                .records
                .extend(self.taxonomy.catalog.records.iter().cloned());
            self.file_operation = Some(Draft {
                action,
                paths: indices
                    .iter()
                    .map(|&i| self.entries[i].path.clone())
                    .collect(),
                order: indices.clone(),
                indices,
                stage: self.stage,
                target: None,
                acknowledged: false,
                query,
                applied_query: String::new(),
                suggestions_open: false,
                suggestion_active: None,
                catalog,
                backdrop: super::modal_backdrop::capture(window.scale_factor()),
                preview_return: None,
                _subscription: subscription,
                focus: focus.clone(),
                return_focus: window.focused(cx),
                dragging: None,
                candidate_limit: 100,
            });
            focus.focus(window);
            tracing::info!(
                ?action,
                storage = "isolated_session",
                "File action draft opened"
            );
            cx.notify();
            return;
        }
        if action == Action::Share && indices.len() != 1 {
            self.notice = Some(crate::i18n::text("Для отправки выберите один файл.").into());
            cx.notify();
            return;
        }
        let paths: Vec<_> = indices
            .iter()
            .map(|&i| self.entries[i].path.clone())
            .collect();
        let owner = match native::owner(window) {
            Ok(owner) => owner,
            Err(error) => {
                self.notice = Some(error);
                cx.notify();
                return;
            }
        };
        let window_handle = window.window_handle();
        self.file_action_busy = true;
        tracing::info!(
            ?action,
            count = paths.len(),
            "Read-only system file action started"
        );
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if action == Action::Share {
                        if !paths[0].is_absolute() || !paths[0].is_file() {
                            return Err(crate::i18n::text("Файл недоступен на диске.").to_owned());
                        }
                        native::prepare(&paths[0]).map(Some)
                    } else {
                        let mut succeeded = 0;
                        let mut failures = Vec::new();
                        for path in paths {
                            if !path.is_absolute() || !path.is_file() {
                                failures.push(
                                    crate::i18n::text("Файл недоступен на диске.").to_owned(),
                                );
                                continue;
                            }
                            match native::open(&path, action == Action::Reveal) {
                                Ok(()) => succeeded += 1,
                                Err(error) => failures.push(error),
                            }
                        }
                        if failures.is_empty() {
                            Ok(None)
                        } else {
                            Err(if crate::i18n::english() {
                                format!(
                                    "{succeeded} completed; {} failed. {}",
                                    failures.len(),
                                    failures[0]
                                )
                            } else {
                                format!(
                                    "{succeeded} выполнено; {} не удалось. {}",
                                    failures.len(),
                                    failures[0]
                                )
                            })
                        }
                    }
                })
                .await;
            let result = match result {
                Ok(Some(file)) => window_handle
                    .update(cx, |_, window, _| {
                        if native::owner(window).ok() != Some(owner) {
                            return Err(
                                crate::i18n::text("Окно отправки больше недоступно.").into()
                            );
                        }
                        native::show(owner, file)
                            .map_err(|_| "Windows sharing is unavailable".to_owned())
                    })
                    .unwrap_or_else(|_| Err(crate::i18n::text("Окно закрыто.").into())),
                Ok(None) => Ok(()),
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                this.file_action_busy = false;
                match result {
                    Ok(()) => tracing::info!(?action, "System file action completed"),
                    Err(error) => {
                        tracing::warn!(?action, reason = %error, "System file action failed");
                        this.notice = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn preview_replacement(&mut self, main: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.file_operation.as_ref() else {
            return;
        };
        let entry = if main {
            let Some(index) = draft.target else { return };
            self.inactive_workspace.entries[index].clone()
        } else {
            self.entries[draft.indices[0]].clone()
        };
        let saved = (
            std::mem::replace(&mut self.entries, vec![entry]),
            std::mem::replace(&mut self.visible, vec![0]),
            self.selected.replace(0),
        );
        self.file_operation.as_mut().unwrap().preview_return = Some(saved);
        self.open_viewer(window, cx);
    }

    pub(super) fn restore_replacement_preview(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.file_operation.as_mut() else {
            return;
        };
        let Some((entries, visible, selected)) = draft.preview_return.take() else {
            return;
        };
        self.entries = entries;
        self.visible = visible;
        self.selected = selected;
        self.tag_counts = crate::catalog::tag_counts(&self.entries);
        draft.focus.focus(window);
        cx.notify();
    }

    pub(super) fn close_file_operation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(draft) = self.file_operation.take() {
            draft
                .return_focus
                .as_ref()
                .unwrap_or(&self.focus)
                .focus(window);
            tracing::debug!(action = ?draft.action, "File action draft closed");
            cx.notify();
        }
    }

    fn save_file_operation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.file_operation.as_ref() else {
            return;
        };
        let unchanged = draft.stage == self.stage
            && draft
                .indices
                .iter()
                .zip(&draft.paths)
                .all(|(&i, path)| self.entries.get(i).is_some_and(|entry| &entry.path == path));
        if !unchanged {
            self.notice = Some(
                crate::i18n::text("Список файлов изменился. Откройте действие заново.").into(),
            );
            self.close_file_operation(window, cx);
            return;
        }
        match draft.action {
            Action::Delete => {
                let removed: BTreeSet<_> = draft.indices.iter().copied().collect();
                let mut i = 0;
                self.entries.retain(|_| {
                    let keep = !removed.contains(&i);
                    i += 1;
                    keep
                });
                self.previews = std::mem::take(&mut self.previews)
                    .into_iter()
                    .filter_map(|(i, image)| {
                        if removed.contains(&i) {
                            None
                        } else {
                            Some((i - removed.range(..i).count(), image))
                        }
                    })
                    .collect();
                self.selected = None;
                self.stage_selection.clear();
                self.details_open = false;
                self.notice = Some(if crate::i18n::english() {
                    format!(
                        "{} file(s) removed from this session. Originals remain on disk.",
                        removed.len()
                    )
                } else {
                    format!(
                        "{} файл(ов) удалено из сессии. Оригиналы на диске сохранены.",
                        removed.len()
                    )
                });
            }
            Action::Replace => {
                let Some(target) = draft.target.filter(|_| draft.acknowledged) else {
                    return;
                };
                let Some(main) = self.inactive_workspace.entries.get_mut(target) else {
                    return;
                };
                let index = draft.indices[0];
                replace_content(main, &self.entries[index]);
                self.inactive_workspace.previews.remove(&target);
                if let Some(preview) = self.previews.get(&index) {
                    self.inactive_workspace
                        .previews
                        .insert(target, preview.clone());
                }
                self.entries.remove(index);
                self.previews = std::mem::take(&mut self.previews)
                    .into_iter()
                    .filter_map(|(i, image)| {
                        if i == index {
                            None
                        } else {
                            Some((if i > index { i - 1 } else { i }, image))
                        }
                    })
                    .collect();
                self.selected = None;
                self.stage_selection.clear();
                self.details_open = false;
                self.notice = Some(
                    crate::i18n::text(
                        "Версия Main заменена в сессии. Оригиналы на диске сохранены.",
                    )
                    .into(),
                );
            }
            Action::Order => {
                if draft.indices == draft.order {
                    return;
                }
                for (position, &index) in draft.order.iter().enumerate() {
                    self.entries[index].details.order_position = Some(position);
                }
                self.notice = Some(
                    crate::i18n::text(
                        "Порядок сохранён в сессии. Файлы на диске не переименованы.",
                    )
                    .into(),
                );
            }
            _ => return,
        }
        tracing::info!(action = ?draft.action, storage = "isolated_session", "File action saved");
        self.tag_counts = crate::catalog::tag_counts(&self.entries);
        self.close_file_operation(window, cx);
        self.refilter(cx);
    }
}

fn order_group(entries: &[Entry], index: usize) -> Vec<usize> {
    let target = &entries[index];
    let key = group_key(target);
    let mut group: Vec<_> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| group_key(entry) == key)
        .map(|(i, _)| i)
        .collect();
    group.sort_by_key(|&i| (entries[i].details.order_position.unwrap_or(usize::MAX), i));

    group
}

fn group_key(entry: &Entry) -> (String, String, String, bool) {
    let base = if entry.video || entry.characters.is_empty() {
        entry.title.clone()
    } else {
        entry.characters.clone()
    };
    (
        crate::naming::name_key(entry.details.artist.as_deref().unwrap_or_default()),
        crate::naming::name_key(
            entry
                .details
                .copyrights
                .first()
                .map(String::as_str)
                .unwrap_or_default(),
        ),
        crate::naming::name_key(&base),
        entry.video,
    )
}

fn move_order(order: &mut Vec<usize>, source: usize, destination: usize) {
    if source < order.len() && destination < order.len() && source != destination {
        let item = order.remove(source);
        order.insert(destination, item);
    }
}

impl Gallery {
    pub(super) fn file_operation_tab(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = &self.file_operation else {
            return;
        };
        if backwards {
            window.focus_prev();
        } else {
            window.focus_next();
        }
        if !draft.focus.contains_focused(window, cx) {
            draft.focus.focus(window);
        }
    }
}

pub(super) fn menu_action(
    label: &str,
    action: Action,
    index: usize,
    stage: bool,
    source: PathBuf,
    gallery: gpui::WeakEntity<Gallery>,
) -> gpui_component::menu::PopupMenuItem {
    gpui_component::menu::PopupMenuItem::new(label.to_owned()).on_click(move |_, window, cx| {
        let _ = gallery.update(cx, |this, cx| {
            if this.stage == stage && this.entries.get(index).is_some_and(|e| e.path == source) {
                this.run_file_action(index, action, window, cx);
            }
        });
    })
}

#[cfg(test)]
mod tests;
