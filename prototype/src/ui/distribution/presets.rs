use gpui::{AppContext, Context, Entity, Focusable, Window};
use gpui_component::input::{InputEvent, InputState};

use super::Gallery;

#[derive(Clone, Default, Debug, PartialEq)]
struct Metadata {
    artist: String,
    values: [Vec<String>; 3],
}

impl Metadata {
    fn insert_value(&mut self, field: usize, value: String) {
        super::metadata::insert_value(&mut self.values[field], value);
    }

    fn summary(&self) -> String {
        let characters = self.values[2].iter().map(|value| {
            super::metadata::character_parts(value, &self.values[0])
                .map_or(value.as_str(), |(name, _)| name)
        });
        let summary = self.values[0]
            .iter()
            .map(String::as_str)
            .chain(characters)
            .chain(self.values[1].iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" · ");

        if summary.is_empty() {
            "—".into()
        } else {
            summary
        }
    }

    fn prune_characters(&mut self) {
        super::metadata::prune_characters(&mut self.values);
    }
}

struct Preset {
    id: usize,
    name: String,
    metadata: Metadata,
}

pub(super) struct State {
    entries: Vec<Preset>,
    next_id: usize,
    pub(super) search: Entity<InputState>,
    pub(super) editor: Option<Editor>,
    notice: Option<String>,
    error: Option<String>,
    _search_subscription: gpui::Subscription,
    catalog: [Vec<String>; 4],
    pub(super) trigger_focus: gpui::FocusHandle,
    control_focus: gpui::FocusHandle,
    picker_focus: gpui::FocusHandle,
    manage_focus: gpui::FocusHandle,
    _focus_subscription: gpui::Subscription,
    backdrop: Option<std::sync::Arc<gpui::RenderImage>>,
    picker_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    return_focus: Option<gpui::FocusHandle>,
    picker_scroll: scrolling::ScrollState,
}

pub(super) struct Editor {
    selected: Option<usize>,
    name: Entity<InputState>,
    artist: Entity<InputState>,
    queries: [Entity<InputState>; 3],
    metadata: Metadata,
    focused: Option<usize>,
    active: Option<usize>,
    assignment: Option<String>,
    confirming_delete: bool,
    _subscriptions: Vec<gpui::Subscription>,
    scroll: scrolling::ScrollState,
    library_scroll: scrolling::ScrollState,
    chip_scroll: [scrolling::ScrollState; 3],
    focus: gpui::FocusHandle,
    close_focus: gpui::FocusHandle,
    cancel_focus: gpui::FocusHandle,
    save_focus: gpui::FocusHandle,
    popup_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    assignment_focus: Vec<gpui::FocusHandle>,
    assignment_enter_pending: bool,
    entry_bounds: [std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>; 4],
}

impl Editor {
    fn refresh_characters(&mut self, _window: &mut Window, cx: &mut Context<Gallery>) {
        while self.assignment_focus.len() <= self.metadata.values[0].len() {
            self.assignment_focus.push(cx.focus_handle().tab_stop(true));
        }
    }
}

impl State {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Gallery>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx));
        let subscription = cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify());
        let control_focus = cx.focus_handle();
        let picker_focus = cx.focus_handle();
        let focus_subscription = cx.on_focus_out(&picker_focus, window, |this, _, window, cx| {
            if let Some(wizard) = &mut this.distribution
                && wizard.presets
                && !wizard.preset_state.trigger_focus.is_focused(window)
            {
                wizard.presets = false;
                cx.notify();
            }
        });

        Self {
            entries: vec![],
            next_id: 0,
            search,
            editor: None,
            notice: None,
            error: None,
            _search_subscription: subscription,
            catalog: Default::default(),
            trigger_focus: cx.focus_handle().tab_stop(true),
            control_focus,
            picker_focus,
            manage_focus: cx.focus_handle().tab_stop(true),
            _focus_subscription: focus_subscription,
            backdrop: None,
            picker_bounds: Default::default(),
            return_focus: None,
            picker_scroll: Default::default(),
        }
    }

    fn validate(
        &self,
        selected: Option<usize>,
        name: &str,
        artist: &str,
    ) -> Result<(), &'static str> {
        validate_name(&self.entries, selected, name, artist)
    }
}

fn validate_name(
    entries: &[Preset],
    selected: Option<usize>,
    name: &str,
    artist: &str,
) -> Result<(), &'static str> {
    if name.trim().is_empty() || artist.trim().is_empty() {
        return Err(crate::i18n::text("Введите название пресета и автора."));
    }
    if name.chars().count() > 256 || name.chars().any(char::is_control) {
        return Err(crate::i18n::text(
            "Используйте непустое имя длиной до 256 символов без управляющих символов.",
        ));
    }
    if entries
        .iter()
        .any(|p| Some(p.id) != selected && p.name.to_lowercase() == name.trim().to_lowercase())
    {
        return Err(crate::i18n::text(
            "Пресет с таким названием уже существует.",
        ));
    }
    Ok(())
}

impl Gallery {
    pub(in crate::ui) fn distribution_tab(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let advance = |window: &mut Window| {
            if backwards {
                window.focus_prev();
            } else {
                window.focus_next();
            }
        };
        let Some(w) = self.distribution.as_mut() else {
            advance(window);
            return;
        };
        if let Some(e) = &w.preset_state.editor {
            if e.assignment.is_some() {
                let first_choice = &e.assignment_focus[0];
                let cancel_choice = &e.assignment_focus[e.metadata.values[0].len()];
                if (backwards && first_choice.is_focused(window))
                    || (!backwards && cancel_choice.is_focused(window))
                {
                    e.queries[if backwards { 2 } else { 1 }]
                        .update(cx, |state, cx| state.focus(window, cx));
                    cx.notify();
                    return;
                }
            }

            let can_save = !e.name.read(cx).value().trim().is_empty()
                && !e.artist.read(cx).value().trim().is_empty();
            let first = &e.close_focus;
            let last = if can_save && !e.confirming_delete {
                &e.save_focus
            } else {
                &e.cancel_focus
            };
            let assignment_focused = || {
                e.assignment.is_some()
                    && e.assignment_focus
                        .iter()
                        .any(|handle| handle.is_focused(window))
            };
            let outside = !e.focus.contains_focused(window, cx) && !assignment_focused();
            if outside
                || e.focus.is_focused(window)
                || (backwards && first.is_focused(window))
                || (!backwards && last.is_focused(window))
            {
                if backwards {
                    last.focus(window);
                } else {
                    first.focus(window);
                }
            } else {
                advance(window);
                let assignment_focused = e.assignment.is_some()
                    && e.assignment_focus
                        .iter()
                        .any(|handle| handle.is_focused(window));
                if !e.focus.contains_focused(window, cx) && !assignment_focused {
                    if backwards {
                        last.focus(window);
                    } else {
                        first.focus(window);
                    }
                }
            }
        } else if w.presets {
            if !backwards && w.preset_state.manage_focus.is_focused(window) {
                w.presets = false;
                w.close_focus.focus(window);
            } else if backwards && w.preset_state.search.focus_handle(cx).is_focused(window) {
                w.preset_state.trigger_focus.focus(window);
            } else if !backwards && w.preset_state.trigger_focus.is_focused(window) {
                w.preset_state
                    .search
                    .update(cx, |state, cx| state.focus(window, cx));
            } else {
                advance(window);
            }
        } else {
            let first = if w.done || w.editing {
                &w.close_focus
            } else {
                &w.preset_state.trigger_focus
            };
            let outside = !w.focus.contains_focused(window, cx) || w.focus.is_focused(window);
            if outside
                || (backwards && first.is_focused(window))
                || (!backwards && w.next_focus.is_focused(window))
            {
                if backwards {
                    w.next_focus.focus(window);
                } else {
                    first.focus(window);
                }
            } else {
                advance(window);
                if !w.focus.contains_focused(window, cx) {
                    if backwards {
                        w.next_focus.focus(window);
                    } else {
                        first.focus(window);
                    }
                }
            }
        }
        cx.notify();
    }

    pub(in crate::ui) fn distribution_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let w = self.distribution.as_mut().unwrap();
        if let Some(editor) = &mut w.preset_state.editor {
            if editor.assignment.take().is_some() {
                editor.queries[2].update(cx, |s, cx| {
                    s.set_value("", window, cx);
                    s.focus(window, cx);
                });
                editor.active = None;
                cx.notify();
                return;
            }
            let suggestions_open = editor.focused.is_some_and(|field| {
                let input = if field == 3 {
                    &editor.artist
                } else {
                    &editor.queries[field]
                };
                !input.read(cx).value().trim().is_empty()
                    && (field != 3 || !w.preset_state.catalog[3].is_empty())
            });
            if suggestions_open {
                editor.focused = None;
                editor.active = None;
            } else if editor.confirming_delete {
                editor.confirming_delete = false;
                editor.focus.focus(window);
            } else {
                self.close_preset_manager(window, cx);
            }
        } else if w.presets {
            w.presets = false;
            w.preset_state.trigger_focus.focus(window);
        } else {
            self.close_distribution(cx);
        }
        cx.notify();
    }

    fn close_preset_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = &mut self.distribution.as_mut().unwrap().preset_state;
        state.editor = None;
        state
            .return_focus
            .as_ref()
            .unwrap_or(&state.trigger_focus)
            .focus(window);
        tracing::debug!(operation = "metadata_presets", "Demo preset manager closed");
        cx.notify();
    }

    pub(super) fn open_preset_manager(
        &mut self,
        create: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let w = self.distribution.as_mut().unwrap();
        w.preset_state.return_focus = Some(if create && !w.presets && w.step == 5 {
            w.save_preset_focus.clone()
        } else {
            w.preset_state.trigger_focus.clone()
        });
        if !w.presets {
            w.preset_state.backdrop = crate::ui::modal_backdrop::capture(window.scale_factor());
        }
        w.presets = false;
        w.focused = false;
        w.artist_focused = false;
        w.preset_state.notice = None;
        w.preset_state.error = None;
        w.preset_state
            .search
            .update(cx, |s, cx| s.set_value("", window, cx));
        let current = Metadata {
            artist: w.artist.read(cx).value().to_string(),
            values: w.values.clone(),
        };
        let first = (!create).then(|| w.preset_state.entries.first()).flatten();
        let selected = first.map(|p| p.id);
        let name = first.map(|p| p.name.clone()).unwrap_or_default();
        let metadata = first.map(|p| p.metadata.clone()).unwrap_or(current);
        let artist =
            cx.new(|cx| InputState::new(window, cx).default_value(metadata.artist.clone()));
        let queries = std::array::from_fn(|_| cx.new(|cx| InputState::new(window, cx)));
        let mut subscriptions = vec![];
        for (field, state) in queries.iter().chain(std::iter::once(&artist)).enumerate() {
            subscriptions.push(cx.subscribe_in(
                state,
                window,
                move |this, input, event, window, cx| {
                    if this
                        .distribution
                        .as_ref()
                        .and_then(|w| w.preset_state.editor.as_ref())
                        .is_none()
                    {
                        return;
                    }
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        let editor = this
                            .distribution
                            .as_ref()
                            .unwrap()
                            .preset_state
                            .editor
                            .as_ref()
                            .unwrap();
                        let value = editor
                            .active
                            .and_then(|i| this.preset_options(field, cx).get(i).cloned())
                            .unwrap_or_else(|| input.read(cx).value().to_string());
                        this.preset_add(field, value, window, cx);
                    } else {
                        let e = this
                            .distribution
                            .as_mut()
                            .unwrap()
                            .preset_state
                            .editor
                            .as_mut()
                            .unwrap();
                        match event {
                            InputEvent::Focus => e.focused = Some(field),
                            InputEvent::Blur if e.focused == Some(field) => e.focused = None,
                            InputEvent::Change => {
                                e.active = None;
                                e.assignment = None;
                            }
                            _ => {}
                        }
                    }
                    cx.notify();
                },
            ));
        }
        let name = cx.new(|cx| InputState::new(window, cx).default_value(name));
        subscriptions.push(cx.subscribe_in(
            &name,
            window,
            |_, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value().to_string();
                    let limited = limited_name(&value);
                    if value != limited {
                        input.update(cx, |s, cx| s.set_value(limited, window, cx));
                    }
                }
                cx.notify();
            },
        ));
        let focus = cx.focus_handle();
        self.distribution.as_mut().unwrap().preset_state.editor = Some(Editor {
            selected,
            name: name.clone(),
            artist,
            queries,
            metadata,
            focused: None,
            active: None,
            assignment: None,
            confirming_delete: false,
            _subscriptions: subscriptions,
            scroll: Default::default(),
            library_scroll: Default::default(),
            chip_scroll: Default::default(),
            focus: focus.clone(),
            close_focus: cx.focus_handle().tab_stop(true),
            cancel_focus: cx.focus_handle().tab_stop(true),
            save_focus: cx.focus_handle().tab_stop(true),
            popup_bounds: Default::default(),
            assignment_focus: vec![],
            assignment_enter_pending: false,
            entry_bounds: Default::default(),
        });
        self.distribution
            .as_mut()
            .unwrap()
            .preset_state
            .editor
            .as_mut()
            .unwrap()
            .refresh_characters(window, cx);
        focus.focus(window);
        tracing::info!(
            operation = "metadata_presets",
            create,
            "Demo preset manager opened"
        );
        cx.notify();
    }

    fn fill_preset_editor(
        &mut self,
        id: Option<usize>,
        duplicate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let w = self.distribution.as_mut().unwrap();
        let p = id.and_then(|id| w.preset_state.entries.iter().find(|p| p.id == id));
        let name = p.map(|p| p.name.clone()).unwrap_or_default();
        let metadata = p.map(|p| p.metadata.clone()).unwrap_or_else(|| Metadata {
            artist: w.artist.read(cx).value().to_string(),
            values: w.values.clone(),
        });
        let e = w.preset_state.editor.as_mut().unwrap();
        if !duplicate {
            e.metadata = metadata;
        } else {
            e.metadata.artist = e.artist.read(cx).value().to_string();
        }
        e.selected = if duplicate { None } else { id };
        e.name.update(cx, |s, cx| {
            s.set_value(if duplicate { String::new() } else { name }, window, cx)
        });
        e.artist.update(cx, |s, cx| {
            s.set_value(e.metadata.artist.clone(), window, cx)
        });
        e.focused = None;
        e.active = None;
        e.assignment = None;
        e.confirming_delete = false;
        e.refresh_characters(window, cx);
        if duplicate {
            e.focus.focus(window);
        }
        cx.notify();
    }

    fn save_preset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = &mut self.distribution.as_mut().unwrap().preset_state;
        state.notice = None;
        state.error = None;
        let e = state.editor.as_ref().unwrap();
        let name = e.name.read(cx).value().trim().to_string();
        let artist = e.artist.read(cx).value().to_string();
        if let Err(error) = state.validate(e.selected, &name, &artist) {
            state.error = Some(if crate::i18n::english() {
                format!("Could not save preset: Error: {error}")
            } else {
                format!("Не удалось сохранить пресет: Error: {error}")
            });
            tracing::warn!(
                operation = "metadata_presets",
                reason = error,
                "Demo preset validation rejected"
            );
            cx.notify();
            return;
        }
        let e = state.editor.as_ref().unwrap();
        let selected = e.selected;
        let mut metadata = e.metadata.clone();
        metadata.artist = artist;
        metadata.prune_characters();
        let id = selected.unwrap_or_else(|| {
            state.next_id += 1;
            state.next_id
        });
        state.entries.retain(|p| p.id != id);
        state.entries.push(Preset { id, name, metadata });
        state.entries.sort_by_key(|a| a.name.to_lowercase());
        state.notice = Some(crate::i18n::text("Пресет метаданных сохранён.").into());
        tracing::info!(
            operation = "metadata_presets",
            updated = selected.is_some(),
            "Demo preset saved"
        );
        self.close_preset_manager(window, cx);
    }

    fn apply_preset(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let w = self.distribution.as_mut().unwrap();
        let Some(preset) = w.preset_state.entries.iter().find(|p| p.id == id) else {
            return;
        };
        w.values = preset.metadata.values.clone();
        w.artist.update(cx, |s, cx| {
            s.set_value(preset.metadata.artist.clone(), window, cx)
        });
        w.query.update(cx, |s, cx| s.set_value("", window, cx));
        w.assignment = None;
        w.active = None;
        w.focused = false;
        w.artist_focused = false;
        w.error = None;
        w.presets = false;
        w.preset_state.notice = Some(
            crate::i18n::text("Пресет применён к форме. Проверьте данные перед распределением.")
                .into(),
        );
        w.preset_state.error = None;
        w.preset_state.trigger_focus.focus(window);
        tracing::info!(
            operation = "metadata_presets",
            "Demo preset applied to distribution draft"
        );
        cx.notify();
    }

    fn preset_options(&self, field: usize, cx: &Context<Self>) -> Vec<String> {
        let e = self
            .distribution
            .as_ref()
            .unwrap()
            .preset_state
            .editor
            .as_ref()
            .unwrap();
        let query = if field == 3 {
            &e.artist
        } else {
            &e.queries[field]
        }
        .read(cx)
        .value()
        .to_lowercase();
        if query.trim().is_empty() {
            return vec![];
        }
        // The reference manager receives the explicit catalog, not the wizard's
        // library-derived fallback. The isolated demo has no explicit catalog.
        let catalog = &self.distribution.as_ref().unwrap().preset_state.catalog[field];
        let mut options: Vec<String> = catalog
            .iter()
            .filter(|s| {
                let character_allowed = field != 2
                    || e.metadata.values[0]
                        .iter()
                        .any(|c| s.ends_with(&format!(" ({c})")));
                let unselected = field == 3
                    || !e.metadata.values[field]
                        .iter()
                        .any(|selected| selected.eq_ignore_ascii_case(s));
                character_allowed && unselected && s.to_lowercase().contains(query.trim())
            })
            .cloned()
            .collect();
        options.sort_by_key(|s| !s.to_lowercase().starts_with(query.trim()));
        options.truncate(30);
        options
    }

    fn preset_add(
        &mut self,
        field: usize,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let e = self
            .distribution
            .as_mut()
            .unwrap()
            .preset_state
            .editor
            .as_mut()
            .unwrap();
        let mut value = value.trim().to_string();
        if value.is_empty() {
            return;
        }
        if field == 3 {
            e.artist.update(cx, |s, cx| s.set_value(value, window, cx));
            e.focused = None;
        } else {
            if field == 2 {
                if e.metadata.values[0].is_empty() {
                    return;
                }
                let qualified = e.metadata.values[0]
                    .iter()
                    .any(|c| value.ends_with(&format!(" ({c})")));
                if !qualified {
                    if e.metadata.values[0].len() > 1 {
                        e.assignment = Some(value);
                        e.assignment_enter_pending = true;
                        e.focused = None;
                        e.assignment_focus[0].focus(window);
                        cx.notify();
                        return;
                    }
                    value = format!("{value} ({})", e.metadata.values[0][0]);
                }
            }
            e.metadata.insert_value(field, value);
            e.queries[field].update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.focus(window, cx);
            });
            e.assignment = None;
        }
        e.active = None;
        e.refresh_characters(window, cx);
        cx.notify();
    }
}

fn limited_name(value: &str) -> String {
    let mut units = 0;
    value
        .chars()
        .take_while(|character| {
            units += character.len_utf16();
            units <= 256
        })
        .collect()
}

fn matching_presets<'a>(entries: &'a [Preset], query: &str) -> Vec<&'a Preset> {
    let query = query.trim().to_lowercase();
    let mut matches: Vec<_> = entries
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&query))
        .collect();
    matches.sort_by_key(|p| !p.name.to_lowercase().starts_with(&query));

    matches
}

mod presentation;
pub(super) mod scrolling;

#[cfg(test)]
mod tests;
