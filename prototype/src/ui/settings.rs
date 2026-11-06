use std::{cell::Cell, rc::Rc};

use gpui::{AppContext, Context, Entity, ScrollHandle, Subscription, Window, point, px};
use gpui_component::input::{InputEvent, InputState};

use super::{ACCENT, Gallery, LINE, MUTED, SURFACE, TEXT, motion, shell::icon};

mod backup;
mod cards;
mod dialogs;
mod presentation;
mod select;
mod text;
use text::text;

const SECTIONS: [(&str, &str, &str); 9] = [
    ("library", "folder", "libraryDescription"),
    ("staging", "folder-input", "stagingDescription"),
    ("structure", "git-branch", "structureDescription"),
    ("indexing", "scan", "indexingDescription"),
    ("cache", "database", "cacheDescription"),
    ("catalogSection", "copy", "catalogDescription"),
    ("logs", "file-text", "logsDescription"),
    ("updates", "reset", "updatesDescription"),
    ("appearance", "nav-settings", "appearanceDescription"),
];
const DEFAULT_FIELDS: [&str; 3] = [" х ", "Videos", "Animations"];
const LANGUAGES: [&str; 3] = ["Системный", "English", "Русский"];
const THEMES: [&str; 7] = [
    "themeName",
    "theme-graphite",
    "theme-arctic-blue",
    "theme-forest",
    "theme-warm-amber",
    "theme-rosewood",
    "theme-paper",
];

pub(super) struct Settings {
    toggles: [bool; 5],
    paths: [Option<String>; 2],
    saved_paths: [Option<String>; 2],
    pub(super) workflow: dialogs::Workflow,
    fields: [Entity<InputState>; 3],
    language: usize,
    theme: usize,
    saved_appearance: [usize; 2],
    select_open: [bool; 2],
    select_active: [usize; 2],
    select_focus: [gpui::FocusHandle; 2],
    saved: bool,
    message: Option<String>,
    scroll: ScrollHandle,
    positions: [Rc<Cell<f32>>; 9],
    _subscriptions: Vec<Subscription>,
}

impl Settings {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Gallery>) -> Self {
        let fields = DEFAULT_FIELDS
            .map(|value| cx.new(|cx| InputState::new(window, cx).default_value(value)));
        let subscriptions = fields
            .iter()
            .map(|field| {
                cx.subscribe(field, |this, _, event, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.settings.saved = false;
                        cx.notify();
                    }
                })
            })
            .collect();

        Self {
            toggles: [true, true, false, false, false],
            paths: [None, None],
            saved_paths: [None, None],
            workflow: dialogs::Workflow::new(cx),
            fields,
            language: 0,
            theme: 0,
            saved_appearance: [0, 0],
            select_open: [false; 2],
            select_active: [0; 2],
            select_focus: std::array::from_fn(|_| cx.focus_handle()),
            saved: false,
            message: None,
            scroll: ScrollHandle::new(),
            positions: std::array::from_fn(|_| Rc::new(Cell::new(f32::INFINITY))),
            _subscriptions: subscriptions,
        }
    }

    fn active_section(&self) -> usize {
        let offset = -f32::from(self.scroll.offset().y);
        let maximum = f32::from(self.scroll.max_offset().height);
        let positions = self.positions.each_ref().map(|position| position.get());

        section_at_offset(offset, maximum, &positions)
    }
}

fn section_at_offset(offset: f32, maximum: f32, positions: &[f32; 9]) -> usize {
    if maximum > 0.0 && offset >= maximum - 2.0 {
        return 8;
    }

    positions
        .iter()
        .rposition(|position| *position <= offset + 80.0)
        .unwrap_or(0)
}

impl Gallery {
    fn apply_interface_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.saved_appearance = [self.settings.language, self.settings.theme];
        let locale = crate::i18n::apply(self.settings.language);
        crate::appearance::apply(self.settings.theme, cx);
        self.shell_background = super::background::shell_image();
        self.notice = None;
        self.search.update(cx, |state, cx| {
            state.set_placeholder(
                crate::i18n::text("Метаданные через точку с запятой"),
                window,
                cx,
            );
        });
        self.refresh_taxonomy_locale(window, cx);
        self.refresh_viewer_locale(window, cx);
        self.refresh_health_locale();
        self.refresh_duplicates_locale();
        tracing::info!(
            operation = "interface_preferences",
            locale,
            theme = self.settings.theme,
            storage = "demo_session",
            "Saved language and theme applied to all windows"
        );
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.dialog_open {
            return;
        }

        self.switch_workspace(false, window, cx);
        self.settings_open = true;
        [self.settings.language, self.settings.theme] = self.settings.saved_appearance;
        self.player = None;
        self.viewer = false;
        self.marquee = None;
        self.details_open = false;
        self.drawer_transition = motion::Transition::new(0.0, 220);
        self.sort_open = false;
        self.notice = None;
        self.settings.scroll.set_offset(point(px(0.0), px(0.0)));
        self.settings.message = None;
        self.settings.select_open = [false; 2];
        self.focus.focus(window);
        tracing::info!(
            operation = "open_settings",
            storage = "demo_session",
            "Settings preview opened"
        );
        cx.notify();
    }

    fn settings_preview_action(&mut self, operation: &'static str, cx: &mut Context<Self>) {
        tracing::info!(
            operation,
            storage = "demo_session",
            outcome = "preview_only",
            "Settings action preview completed without external changes"
        );
        self.notice = Some(format!("{} · {}", text(operation), text("previewNotice")));
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
