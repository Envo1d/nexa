use gpui::{AppContext, Context, Entity, FocusHandle, Window};
use gpui_component::input::{InputEvent, InputState};
use unicode_normalization::UnicodeNormalization;

use super::Gallery;

pub(super) struct Editor {
    index: usize,
    draft: Vec<String>,
    query: Entity<InputState>,
    error: Option<String>,
    active: Option<usize>,
    suggestions: bool,
    focus: FocusHandle,
    close_focus: FocusHandle,
    save_focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    input_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    backdrop: Option<std::sync::Arc<gpui::RenderImage>>,
    _subscription: gpui::Subscription,
}

fn add_tag(draft: &mut Vec<String>, value: &str) -> Result<(), &'static str> {
    let value: String = value.trim().nfc().collect();
    if value.is_empty() {
        return Ok(());
    }

    let key = crate::naming::name_key(&value);
    if draft.iter().any(|tag| crate::naming::name_key(tag) == key) {
        return Ok(());
    }
    if value.chars().count() > 24 || value.chars().any(char::is_control) || draft.len() >= 100 {
        return Err("Можно добавить до 100 тегов длиной до 24 символов, без управляющих символов.");
    }

    draft.push(value);
    Ok(())
}

impl Gallery {
    pub(super) fn open_file_tags(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tag_editor.is_some() || self.distribution.is_some() || self.loading {
            return;
        }

        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Добавить тег…"));
        let subscription = cx.subscribe_in(&query, window, |this, _, event, window, cx| {
            match event {
                InputEvent::PressEnter { .. } => this.add_file_tag(None, window, cx),
                InputEvent::Change | InputEvent::Focus => {
                    if let Some(editor) = &mut this.tag_editor {
                        editor.active = None;
                        editor.suggestions = true;
                    }
                }
                InputEvent::Blur => {
                    if let Some(editor) = &mut this.tag_editor {
                        editor.suggestions = false;
                    }
                }
            }
            cx.notify();
        });
        self.tag_editor = Some(Editor {
            index,
            draft: self.entries[index].tags.clone(),
            query,
            error: None,
            active: None,
            suggestions: false,
            focus: cx.focus_handle(),
            close_focus: cx.focus_handle().tab_stop(true),
            save_focus: cx.focus_handle().tab_stop(true),
            return_focus: window.focused(cx),
            input_bounds: Default::default(),
            backdrop: super::modal_backdrop::capture(window.scale_factor()),
            _subscription: subscription,
        });
        self.tag_editor
            .as_ref()
            .unwrap()
            .query
            .update(cx, |s, cx| s.focus(window, cx));
        tracing::info!(
            operation = "file_tags",
            storage = "demo_session",
            "Tag editor opened"
        );
        cx.notify();
    }

    fn file_tag_matches(&self, cx: &Context<Self>) -> Vec<String> {
        let editor = self.tag_editor.as_ref().unwrap();
        let query = crate::naming::name_key(editor.query.read(cx).value().trim());
        let mut options = self
            .tag_counts
            .iter()
            .map(|(tag, _)| tag.clone())
            .collect::<Vec<_>>();
        options.sort_by(|a, b| crate::naming::compare_names(a, b));
        options.dedup_by(|a, b| crate::naming::name_key(a) == crate::naming::name_key(b));
        options.retain(|tag| crate::naming::name_key(tag).contains(&query));
        options.sort_by_key(|tag| usize::from(!crate::naming::name_key(tag).starts_with(&query)));
        options.truncate(30);
        options
    }

    fn add_file_tag(&mut self, value: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.tag_editor.as_ref() else {
            return;
        };
        let value = value.unwrap_or_else(|| {
            if editor.suggestions {
                editor
                    .active
                    .and_then(|index| self.file_tag_matches(cx).get(index).cloned())
            } else {
                None
            }
            .unwrap_or_else(|| editor.query.read(cx).value().to_string())
        });
        let editor = self.tag_editor.as_mut().unwrap();
        match add_tag(&mut editor.draft, &value) {
            Ok(()) => {
                editor.error = None;
                editor.suggestions = false;
                editor.query.update(cx, |s, cx| s.set_value("", window, cx));
            }
            Err(error) => editor.error = Some(error.into()),
        }
        cx.notify();
    }

    fn save_file_tags(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.tag_editor.as_mut() else {
            return;
        };
        let pending = editor.query.read(cx).value().to_string();
        if let Err(error) = add_tag(&mut editor.draft, &pending) {
            editor.error = Some(error.into());
            tracing::warn!(
                operation = "file_tags",
                reason = "invalid_pending_tag",
                "Tag save rejected"
            );
            cx.notify();
            return;
        }
        let editor = self.tag_editor.take().unwrap();
        self.entries[editor.index].tags = editor.draft;
        self.tag_counts = crate::catalog::tag_counts(&self.entries);
        self.refilter(cx);
        editor
            .return_focus
            .as_ref()
            .unwrap_or(&self.focus)
            .focus(window);
        tracing::info!(
            operation = "file_tags",
            storage = "demo_session",
            "File tags saved"
        );
        cx.notify();
    }

    pub(super) fn close_file_tags(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.tag_editor.take() {
            editor
                .return_focus
                .as_ref()
                .unwrap_or(&self.focus)
                .focus(window);
            tracing::debug!(operation = "file_tags", "Tag draft discarded");
            cx.notify();
        }
    }

    pub(super) fn file_tags_tab(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self.tag_editor.as_ref().unwrap();
        let outside = !editor.focus.contains_focused(window, cx) || editor.focus.is_focused(window);
        if outside
            || (backwards && editor.close_focus.is_focused(window))
            || (!backwards && editor.save_focus.is_focused(window))
        {
            if backwards {
                editor.save_focus.focus(window);
            } else {
                editor.close_focus.focus(window);
            }
        } else {
            if backwards {
                window.focus_prev();
            } else {
                window.focus_next();
            }
            if !editor.focus.contains_focused(window, cx) {
                if backwards {
                    editor.save_focus.focus(window);
                } else {
                    editor.close_focus.focus(window);
                }
            }
        }
    }
}

mod presentation;

#[cfg(test)]
mod tests;
