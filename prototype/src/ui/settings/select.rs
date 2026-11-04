use gpui::{
    App, BoxShadow, Context, Corner, Focusable, IntoElement, RenderOnce, WeakEntity, Window, div,
    point, prelude::*, px, rgb, rgba,
};
use gpui_component::{Selectable, popover::Popover};

use super::{ACCENT, Gallery, LANGUAGES, LINE, MUTED, TEXT, THEMES, icon, motion, text};

impl Gallery {
    pub(super) fn settings_selector(
        &self,
        theme: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let slot = usize::from(theme);
        let selected = if theme {
            self.settings.theme
        } else {
            self.settings.language
        };
        let options: Vec<String> = if theme {
            THEMES.iter().map(|key| text(key).to_owned()).collect()
        } else {
            LANGUAGES.iter().map(|value| (*value).to_owned()).collect()
        };
        let gallery = cx.weak_entity();
        let open = gallery.clone();

        Popover::new(if theme {
            "settings-theme"
        } else {
            "settings-language"
        })
        .appearance(false)
        .anchor(if theme {
            Corner::BottomLeft
        } else {
            Corner::TopLeft
        })
        .open(self.settings.select_open[slot])
        .on_open_change(move |opened, window, cx| {
            let _ = open.update(cx, |this, cx| {
                this.settings.select_open[slot] = *opened;
                this.settings.select_active[slot] = if theme {
                    this.settings.theme
                } else {
                    this.settings.language
                };
                if !opened {
                    this.settings.select_focus[slot].focus(window);
                }
                cx.notify();
            });
        })
        .trigger(Trigger {
            gallery: gallery.clone(),
            theme,
            label: options[selected].clone(),
            open: self.settings.select_open[slot],
            focus: self.settings.select_focus[slot].clone(),
        })
        .when(theme, |view| view.pb(px(2.0)))
        .when(!theme, |view| view.pt(px(2.0)))
        .content(move |state, window, cx| {
            state.focus_handle(cx).focus(window);
            let active = gallery
                .upgrade()
                .map(|view| view.read(cx).settings.select_active[slot])
                .unwrap_or(selected);
            let keys = gallery.clone();
            let key_options = options.clone();

            div()
                .id("settings-select-options")
                .track_focus(&state.focus_handle(cx))
                .flex()
                .flex_col()
                .w(px(210.0))
                .p(px(5.0))
                .rounded(px(9.0))
                .border_1()
                .border_color(rgb(0x6d5aa4))
                .bg(rgb(0x1a1827))
                .text_size(px(11.0))
                .line_height(px(15.0))
                .shadow(vec![BoxShadow {
                    color: rgba(0x00000080).into(),
                    offset: point(px(0.0), px(10.0)),
                    blur_radius: px(28.0),
                    spread_radius: px(0.0),
                }])
                .on_key_down(
                    cx.listener(move |state, event: &gpui::KeyDownEvent, window, cx| {
                        let key = event.keystroke.key.as_str();
                        if key == "tab" {
                            state.dismiss(window, cx);
                            return;
                        }

                        let first = key_options.iter().position(|value| {
                            key.chars().count() == 1
                                && value.to_lowercase().starts_with(&key.to_lowercase())
                        });
                        if first.is_none()
                            && !matches!(key, "up" | "down" | "home" | "end" | "enter" | "space")
                        {
                            return;
                        }

                        let choose = matches!(key, "enter" | "space");
                        let _ = keys.update(cx, |this, cx| {
                            let count = key_options.len();
                            let active = &mut this.settings.select_active[slot];
                            *active = match key {
                                "up" => (*active + count - 1) % count,
                                "down" => (*active + 1) % count,
                                "home" => 0,
                                "end" => count - 1,
                                _ => first.unwrap_or(*active),
                            };
                            if choose {
                                if theme {
                                    this.settings.theme = *active;
                                } else {
                                    this.settings.language = *active;
                                }
                                this.settings.saved = false;
                            }
                            cx.notify();
                        });
                        if choose {
                            state.dismiss(window, cx);
                        }
                        cx.stop_propagation();
                    }),
                )
                .children(options.iter().enumerate().map(|(index, value)| {
                    let click = gallery.clone();
                    let hover = gallery.clone();

                    div()
                        .id(("settings-option", index))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(6.0))
                        .px(px(7.0))
                        .py(px(9.0))
                        .rounded(px(5.0))
                        .cursor_pointer()
                        .text_color(rgb(if index == selected { ACCENT } else { TEXT }))
                        .when(index == active, |view| view.bg(rgb(0x2c2644)))
                        .child(value.clone())
                        .when(index == selected, |view| {
                            view.child(icon("check").size(px(14.0)))
                        })
                        .on_hover(move |hovered, _, cx| {
                            if *hovered {
                                let _ = hover.update(cx, |this, cx| {
                                    this.settings.select_active[slot] = index;
                                    cx.notify();
                                });
                            }
                        })
                        .on_click(cx.listener(move |state, _, window, cx| {
                            let _ = click.update(cx, |this, cx| {
                                if theme {
                                    this.settings.theme = index;
                                } else {
                                    this.settings.language = index;
                                }
                                this.settings.saved = false;
                                cx.notify();
                            });
                            state.dismiss(window, cx);
                        }))
                }))
                .into_any_element()
        })
    }
}

#[derive(IntoElement)]
struct Trigger {
    gallery: WeakEntity<Gallery>,
    theme: bool,
    label: String,
    open: bool,
    focus: gpui::FocusHandle,
}

impl Selectable for Trigger {
    fn selected(mut self, selected: bool) -> Self {
        self.open = selected;
        self
    }
    fn is_selected(&self) -> bool {
        self.open
    }
}

impl RenderOnce for Trigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        motion::button(if self.theme {
            "settings-theme-trigger"
        } else {
            "settings-language-trigger"
        })
        .track_focus(&self.focus)
        .tab_index(0)
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .w(px(210.0))
        .h(px(36.0))
        .px(px(10.0))
        .border_1()
        .border_color(rgb(if self.open { ACCENT } else { LINE }))
        .rounded(px(7.0))
        .bg(rgb(0x1a1927))
        .text_color(rgb(TEXT))
        .text_size(px(11.0))
        .line_height(px(13.75))
        .cursor_pointer()
        .focus(|style| style.border_color(rgb(ACCENT)))
        .hover_colors(
            Some(rgb(0x242034).into()),
            None,
            Some(rgb(if self.open { ACCENT } else { 0x6d5aa4 }).into()),
        )
        .child(self.label)
        .child(
            icon("down")
                .size(px(15.0))
                .text_color(rgb(MUTED))
                .rotate(gpui::Radians(if self.open {
                    std::f32::consts::PI
                } else {
                    0.0
                })),
        )
        .on_key_down(move |event, _, cx| {
            if matches!(
                event.keystroke.key.as_str(),
                "enter" | "space" | "up" | "down"
            ) {
                let _ = self.gallery.update(cx, |this, cx| {
                    let slot = usize::from(self.theme);
                    this.settings.select_active[slot] = if self.theme {
                        this.settings.theme
                    } else {
                        this.settings.language
                    };
                    this.settings.select_open[slot] = true;
                    cx.notify();
                });
                cx.stop_propagation();
            }
        })
    }
}
