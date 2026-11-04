use gpui::{
    Context, Div, Focusable, FontWeight, MouseButton, ObjectFit, StyledImage, Window, div, img,
    point, prelude::*, px, rgb, rgba,
};

use super::{Gallery, Panel, UNSUPPORTED};
use crate::ui::{ACCENT, LINE, MUTED, TEXT, motion, shell::icon};

pub(super) fn icon_button(
    id: impl Into<gpui::ElementId>,
    name: &'static str,
    active: bool,
) -> motion::MotionDiv {
    sized_icon_button(id, name, active, 20.0)
}

pub(super) fn video_icon_button(
    id: impl Into<gpui::ElementId>,
    name: &'static str,
    active: bool,
) -> motion::MotionDiv {
    let size = match name {
        "play" | "pause" => 20.0,
        "player-fullscreen" | "player-fullscreen-exit" => 19.0,
        _ => 18.0,
    };
    sized_icon_button(id, name, active, size)
}

pub(super) fn sized_icon_button(
    id: impl Into<gpui::ElementId>,
    name: &'static str,
    active: bool,
    size: f32,
) -> motion::MotionDiv {
    let label = match name {
        "heart" => {
            if active {
                "Убрать из избранного"
            } else {
                "В избранное"
            }
        }
        "viewer-heart-filled" => "Убрать из избранного",
        "tag" => "Теги",
        "info" => "Информация о файле",
        "keyboard" => "Подсказки",
        "external-link" => "Открыть внешне",
        "close" => "Закрыть",
        "play" => "Воспроизвести",
        "pause" => "Пауза",
        "repeat" => "Повтор видео (R)",
        "volume-2" => "Выключить звук",
        "volume-x" => "Включить звук",
        "player-fullscreen" => "Полный экран",
        "player-fullscreen-exit" => "Выйти из полного экрана",
        "minus" => "Уменьшить",
        "plus" => "Увеличить",
        _ => "",
    };

    motion::button(id)
        .keyboard_focus(true)
        .size(px(28.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .cursor_pointer()
        .text_color(rgb(if active { ACCENT } else { MUTED }))
        .bg(if active { rgba(0xa786ff1a) } else { rgba(0) })
        .hover_colors(Some(rgba(0xa786ff1a).into()), Some(rgb(TEXT).into()), None)
        .child(icon(name).size(px(size)))
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(label).build(window, cx))
}

pub(super) fn label_button(id: &'static str, label: &str) -> motion::MotionDiv {
    motion::button(id)
        .keyboard_focus(true)
        .min_h(px(32.0))
        .px(px(12.0))
        .py(px(8.0))
        .text_size(px(11.0))
        .line_height(px(13.75))
        .font_weight(FontWeight::MEDIUM)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .cursor_pointer()
        .text_color(rgb(TEXT))
        .hover_colors(Some(rgb(0x242034).into()), None, None)
        .child(label.to_owned())
}

impl Gallery {
    fn viewer_image_canvas(
        &self,
        index: usize,
        geometry: (f32, f32, f32, f32),
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let (left, top, width, height) = geometry;
        let gallery = cx.weak_entity();
        let generation = self.viewer_state.generation;
        let image = img(self.entries[index].path.clone())
            .id(("viewer-image", index))
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(width))
            .h(px(height))
            .object_fit(ObjectFit::Fill)
            .with_loading(|| status("Загрузка файла…").into_any_element())
            .with_fallback(move || {
                let gallery = gallery.clone();
                gpui::canvas(
                    move |_, _, cx| {
                        let _ = gallery.update(cx, |this, cx| {
                            if this.viewer_state.generation == generation
                                && this.viewer_state.error.is_none()
                            {
                                this.viewer_state.error = Some(UNSUPPORTED.into());
                                this.viewer_state.loading = false;
                                tracing::error!(
                                    operation = "media_viewer",
                                    "GPUI image decoding failed"
                                );
                                cx.notify();
                            }
                        });
                    },
                    |_, (), _, _| (),
                )
                .size_full()
                .into_any_element()
            });

        div()
            .id("viewer-image-canvas")
            .absolute()
            .inset_0()
            .cursor_grab()
            .when(self.viewer_state.drag.is_some(), |view| {
                view.cursor(gpui::CursorStyle::ClosedHand)
            })
            .on_scroll_wheel(
                cx.listener(|this, event: &gpui::ScrollWheelEvent, window, cx| {
                    if this.viewer_state.natural.is_none() {
                        return;
                    }
                    let factor = if event.delta.pixel_delta(px(16.0)).y < px(0.0) {
                        1.0 / 1.15
                    } else {
                        1.15
                    };
                    this.viewer_state
                        .set_zoom(this.viewer_state.zoom * factor, window);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _, _| {
                    if this.viewer_state.natural.is_some() {
                        this.viewer_state.drag = Some((event.position, this.viewer_state.pan));
                    }
                }),
            )
            .on_click(cx.listener(|this, event: &gpui::ClickEvent, _, cx| {
                if event.click_count() == 2 {
                    this.viewer_state.reset_image();
                    cx.notify();
                }
            }))
            .child(image)
    }

    pub(in crate::ui) fn viewer_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let index = self.selected.unwrap_or(0);
        let entry = &self.entries[index];
        let video = entry.video;
        let position = self
            .visible
            .iter()
            .position(|item| *item == index)
            .unwrap_or(0);
        let width = f32::from(window.viewport_size().width);
        let height = f32::from(window.viewport_size().height)
            - if self.viewer_state.fullscreen {
                0.0
            } else {
                60.0
            };
        self.viewer_state.clamp_pan(width, height);
        let (w, h) = self.viewer_state.fitted(width, height);
        let w = w * self.viewer_state.zoom;
        let h = h * self.viewer_state.zoom;
        let left = (width - w) / 2.0 + self.viewer_state.pan.x;
        let top = (height - h) / 2.0 + self.viewer_state.pan.y;
        let fullscreen = self.viewer_state.fullscreen;

        let mut body = div()
            .id("viewer-body")
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .bg(rgb(0x090a0e))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.viewer_state.panel = None;
                    this.viewer_state.focus.focus(window);
                    cx.notify();
                }),
            );

        if let Some(player) = &self.player {
            body = body.child(
                div()
                    .id("viewer-video-surface")
                    .absolute()
                    .inset_0()
                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, window, cx| {
                        if event.click_count() == 2 {
                            this.toggle_video_fullscreen(window, cx);
                        } else {
                            this.toggle_video(cx);
                        }
                    }))
                    .child(alhazen_gpui::video_view(player.clone()).size_full()),
            );
        } else if !video && self.viewer_state.error.is_none() {
            body = body.child(self.viewer_image_canvas(index, (left, top, w, h), cx));
        }

        if self.viewer_state.loading && video {
            body = body.child(status("Загрузка файла…"));
        }
        if self.player.as_ref().is_some_and(|player| {
            matches!(
                player.read(cx).state(),
                alhazen_gpui::PlayerState::Buffering
            )
        }) {
            let buffering = div()
                .px(px(16.0))
                .py(px(10.0))
                .rounded(px(8.0))
                .bg(rgba(0x0a0a12c2))
                .text_color(rgb(0xffffff))
                .child("Буферизация…");
            body = body.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(buffering),
            );
        }
        if let Some(error) = self.viewer_state.error.clone() {
            body = body.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .max_w(px(width * 0.7))
                            .p(px(20.0))
                            .rounded(px(12.0))
                            .bg(rgba(0x0a0a12c2))
                            .border_1()
                            .border_color(rgba(0xffffff20))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(12.0))
                            .text_center()
                            .child(error)
                            .child(
                                label_button("viewer-retry", "Повторить")
                                    .bg(rgb(0x1a1827))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if let Some(index) = this.selected {
                                            gpui::ImageSource::from(
                                                this.entries[index].path.clone(),
                                            )
                                            .remove_asset(cx);
                                        }
                                        this.open_viewer(window, cx)
                                    })),
                            ),
                    ),
            );
        }

        if !fullscreen {
            body = body
                .child(self.viewer_navigation(false, position > 0, cx))
                .child(self.viewer_navigation(true, position + 1 < self.visible.len(), cx));
            if !video {
                body = body.child(self.viewer_zoom(width, cx));
            }
        }
        if video && self.player.is_some() {
            body = body.child(self.video_toolbar(width, window, cx));
        }
        if self.viewer_state.diagnostics && self.player.is_some() {
            body = body.child(self.video_diagnostics(cx));
        }
        if let Some(panel) = self.viewer_state.panel {
            body = body.child(self.viewer_panel(panel, window, cx));
            let show_suggestions = panel == Panel::Tags
                && self.viewer_state.tag_focused
                && !self
                    .viewer_state
                    .tag_input
                    .read(cx)
                    .value()
                    .trim()
                    .is_empty()
                && self
                    .tag_counts
                    .iter()
                    .any(|(tag, _)| !self.entries[index].tags.contains(tag));
            if show_suggestions {
                body = body.child(self.viewer_tag_suggestions(cx));
            }
        }

        div()
            .id("media-viewer")
            .key_context("NexaViewer")
            .track_focus(&self.viewer_state.focus)
            .capture_key_down(
                cx.listener(|this, event, window, cx| this.viewer_keyboard(event, window, cx)),
            )
            .on_mouse_move(
                cx.listener(move |this, event: &gpui::MouseMoveEvent, window, cx| {
                    if video {
                        this.show_video_controls(cx);
                    }
                    if let Some((origin, pan)) = this.viewer_state.drag {
                        if event.pressed_button == Some(MouseButton::Left) {
                            this.viewer_state.pan = point(
                                pan.x + f32::from(event.position.x - origin.x),
                                pan.y + f32::from(event.position.y - origin.y),
                            );
                            this.viewer_state.clamp_pan(width, height);
                            cx.notify();
                        } else {
                            this.viewer_state.drag = None;
                        }
                    }
                    if event.pressed_button.is_none() {
                        this.viewer_state.seeking = false;
                        this.viewer_state.volume_drag = false;
                    }
                    if this.viewer_state.seeking {
                        this.seek_video_pointer(event.position, cx);
                    }
                    if this.viewer_state.volume_drag {
                        this.volume_video_pointer(event.position, cx);
                    }
                    if !window.is_window_active() {
                        this.viewer_state.drag = None;
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.viewer_state.drag = None;
                    this.viewer_state.seeking = false;
                    this.viewer_state.volume_drag = false;
                }),
            )
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(0x141420))
            .text_color(rgb(TEXT))
            .font_family("Segoe UI")
            .text_size(px(13.0))
            .line_height(px(17.0))
            .when(!fullscreen, |view| {
                view.child(self.viewer_header(position, width, cx))
            })
            .child(body)
    }

    fn viewer_header(&self, position: usize, width: f32, cx: &mut Context<Self>) -> Div {
        let entry = &self.entries[self.selected.unwrap_or(0)];
        let title = if entry.video {
            self.viewer_video_title(cx)
        } else {
            "Изображение".into()
        };
        let compact = width <= 1100.0;
        let actions = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(if compact { 4.0 } else { 8.0 }))
            .child(
                icon_button(
                    "viewer-favorite",
                    if entry.favorite {
                        "viewer-heart-filled"
                    } else {
                        "heart"
                    },
                    entry.favorite,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(index) = this.selected {
                        this.entries[index].favorite = !this.entries[index].favorite;
                        tracing::info!(
                            operation = "viewer_annotations",
                            scope = "session",
                            "Viewer favorite changed"
                        );
                        cx.notify();
                    }
                })),
            )
            .children(
                [
                    (Panel::Tags, "tag", "viewer-tags"),
                    (Panel::Info, "info", "viewer-info"),
                    (Panel::Help, "keyboard", "viewer-help"),
                ]
                .into_iter()
                .map(|(panel, name, id)| {
                    icon_button(id, name, self.viewer_state.panel == Some(panel)).on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.viewer_state.panel = if this.viewer_state.panel == Some(panel) {
                                None
                            } else {
                                Some(panel)
                            };
                            this.viewer_state.focus.focus(window);
                            cx.notify();
                        }),
                    )
                }),
            )
            .child(
                icon_button("viewer-external", "external-link", false)
                    .when(entry.demo, |view| {
                        view.opacity(0.4).cursor_default().keyboard_focus(false)
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(entry) = this
                            .selected
                            .and_then(|index| this.entries.get(index))
                            .filter(|entry| !entry.demo)
                        {
                            tracing::info!(
                                operation = "viewer_external",
                                "Opening selected local media with the system application"
                            );
                            cx.open_with_system(&entry.path);
                        }
                    })),
            )
            .child(
                icon_button("viewer-close", "close", false)
                    .on_click(cx.listener(|this, _, window, cx| this.close_viewer(window, cx))),
            );

        div()
            .h(px(60.0))
            .flex_shrink_0()
            .px(px(20.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(rgb(LINE))
            .flex()
            .items_center()
            .gap(px(if compact { 12.0 } else { 20.0 }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(title),
            )
            .child(div().text_color(rgb(MUTED)).child(format!(
                "{} / {}",
                position + 1,
                self.visible.len()
            )))
            .child(div().when(!compact, |view| view.flex_1()).child(actions))
    }

    fn viewer_navigation(
        &self,
        next: bool,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> motion::MotionDiv {
        motion::button(if next {
            "viewer-next"
        } else {
            "viewer-previous"
        })
        .keyboard_focus(enabled)
        .absolute()
        .top(gpui::relative(0.5))
        .mt(px(-22.0))
        .when(next, |view| view.right(px(16.0)))
        .when(!next, |view| view.left(px(16.0)))
        .size(px(44.0))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(rgba(0xffffff20))
        .bg(rgba(0x0a0a12c2))
        .text_color(rgb(0xf4f4f8))
        .opacity(if enabled { 1.0 } else { 0.4 })
        .when(enabled, |view| {
            view.cursor_pointer()
                .hover(|style| style.bg(rgba(0x30303ee6)))
        })
        .child(icon(if next { "arrow-right" } else { "arrow-left" }).size(px(20.0)))
        .on_click(cx.listener(move |this, _, window, cx| {
            if enabled {
                this.move_viewer(if next { 1 } else { -1 }, window, cx);
            }
        }))
    }

    fn viewer_zoom(&self, width: f32, cx: &mut Context<Self>) -> Div {
        let ready = self.viewer_state.image_ready();
        div()
            .absolute()
            .bottom(px(24.0))
            .left(px((width - 260.0) / 2.0))
            .w(px(260.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .border_1()
            .border_color(rgb(0x514361))
            .rounded(px(32.0))
            .bg(rgb(0x141420))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                icon_button("zoom-out", "minus", false)
                    .when(!ready || self.viewer_state.zoom <= 0.25, |view| {
                        view.opacity(0.4).cursor_default().keyboard_focus(false)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.viewer_state
                            .set_zoom(this.viewer_state.zoom / 1.25, window);
                        cx.notify();
                    })),
            )
            .child(div().min_w(px(46.0)).text_center().child(format!(
                "{}%",
                (self.viewer_state.zoom * 100.0).round() as u32
            )))
            .child(
                icon_button("zoom-in", "plus", false)
                    .when(!ready || self.viewer_state.zoom >= 16.0, |view| {
                        view.opacity(0.4).cursor_default().keyboard_focus(false)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.viewer_state
                            .set_zoom(this.viewer_state.zoom * 1.25, window);
                        cx.notify();
                    })),
            )
            .child(
                label_button("zoom-fit", "Вписать")
                    .when(!ready, |view| {
                        view.opacity(0.4).cursor_default().keyboard_focus(false)
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.viewer_state.image_ready() {
                            this.viewer_state.reset_image();
                        }
                        cx.notify();
                    })),
            )
            .child(
                label_button("zoom-original", "1:1")
                    .when(!ready, |view| {
                        view.opacity(0.4).cursor_default().keyboard_focus(false)
                    })
                    .on_click(
                        cx.listener(|this, _, window, cx| this.viewer_original_size(window, cx)),
                    ),
            )
    }

    fn viewer_original_size(&mut self, window: &Window, cx: &mut Context<Self>) {
        if let Some((natural, _)) = self.viewer_state.natural {
            let (fitted, _) = self.viewer_state.fitted(
                window.viewport_size().width.into(),
                f32::from(window.viewport_size().height) - 60.0,
            );
            if fitted > 0.0 {
                self.viewer_state.set_zoom(natural as f32 / fitted, window);
            }
        }
        cx.notify();
    }

    fn viewer_keyboard(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let input_focused = self
            .viewer_state
            .tag_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        if input_focused {
            let matches = self.viewer_tag_matches(cx);
            if key == "escape" && self.viewer_state.tag_focused && !matches.is_empty() {
                self.viewer_state.tag_focused = false;
                cx.stop_propagation();
                cx.notify();
                return;
            }
            if matches!(key, "up" | "down") && !matches.is_empty() {
                let last = matches.len() - 1;
                self.viewer_state.tag_active = Some(match (key, self.viewer_state.tag_active) {
                    ("up", Some(index)) => index.saturating_sub(1),
                    ("up", None) => last,
                    (_, Some(index)) => (index + 1).min(last),
                    _ => 0,
                });
                self.viewer_state.tag_focused = true;
                cx.stop_propagation();
                cx.notify();
                return;
            }
            if key != "escape" {
                return;
            }
        }
        if key == "escape" {
            if self.viewer_state.panel.take().is_some() {
                self.viewer_state.focus.focus(window);
            } else if self.viewer_state.fullscreen {
                self.toggle_video_fullscreen(window, cx);
            } else {
                self.close_viewer(window, cx);
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if matches!(key, "space" | "enter") && !self.viewer_state.focus.is_focused(window) {
            return;
        }
        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.platform
        {
            return;
        }
        let video = self.selected.is_some_and(|index| self.entries[index].video);
        if video && self.video_keyboard(key, window, cx) {
            cx.stop_propagation();
            return;
        }
        match key {
            "left" => self.move_viewer(-1, window, cx),
            "right" => self.move_viewer(1, window, cx),
            "+" | "=" if !video => self
                .viewer_state
                .set_zoom(self.viewer_state.zoom * 1.25, window),
            "-" if !video => self
                .viewer_state
                .set_zoom(self.viewer_state.zoom / 1.25, window),
            "0" if !video && self.viewer_state.image_ready() => self.viewer_state.reset_image(),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

fn status(message: &str) -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .p(px(20.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(rgba(0xffffff20))
                .bg(rgba(0x0a0a12c2))
                .child(message.to_owned()),
        )
}
