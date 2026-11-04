use std::time::{Duration, Instant};

use gpui::{Context, Div, MouseButton, Point, Window, div, prelude::*, px, rgb};

use super::{
    Gallery,
    presentation::{label_button, video_icon_button as icon_button},
};
use crate::ui::{ACCENT, MUTED};

impl Gallery {
    pub(super) fn toggle_video(&mut self, cx: &mut Context<Self>) {
        if let Some(player) = &self.player {
            player.update(cx, |player, cx| player.toggle(cx));
        }
        self.show_video_controls(cx);
    }

    pub(super) fn toggle_video_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.viewer_state.fullscreen = !self.viewer_state.fullscreen;
        if self.viewer_state.fullscreen != window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        self.viewer_state.panel = None;
        self.show_video_controls(cx);
        tracing::info!(
            operation = "viewer_fullscreen",
            fullscreen = self.viewer_state.fullscreen,
            "Video fullscreen changed"
        );
    }

    pub(super) fn show_video_controls(&mut self, cx: &mut Context<Self>) {
        self.viewer_state.controls_until = Instant::now() + Duration::from_secs(2);
        cx.notify();
    }

    pub(super) fn video_keyboard(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "space" | "k" => self.toggle_video(cx),
            "f" => self.toggle_video_fullscreen(window, cx),
            "m" => self.mute_video(cx),
            "r" => {
                self.viewer_state.repeat = !self.viewer_state.repeat;
            }
            "t" => {
                self.viewer_state.diagnostics = !self.viewer_state.diagnostics;
            }
            "left" | "right" if self.viewer_state.fullscreen => {
                if let Some(player) = &self.player {
                    let current = player.read(cx).position().as_secs_f64();
                    let next = (current + if key == "left" { -5.0 } else { 5.0 }).max(0.0);
                    player.update(cx, |player, cx| {
                        player.seek(Duration::from_secs_f64(next), cx)
                    });
                }
            }
            "up" | "down" if self.viewer_state.fullscreen => {
                if let Some(player) = &self.player {
                    let volume = (player.read(cx).volume()
                        + if key == "up" { 0.05 } else { -0.05 })
                    .clamp(0.0, 1.0);
                    player.update(cx, |player, cx| {
                        player.set_muted(false, cx);
                        player.set_volume(volume, cx);
                    });
                }
            }
            _ => return false,
        }
        self.show_video_controls(cx);
        true
    }

    fn mute_video(&mut self, cx: &mut Context<Self>) {
        if let Some(player) = &self.player {
            player.update(cx, |player, cx| player.set_muted(!player.is_muted(), cx));
        }
    }

    pub(super) fn seek_video_pointer(
        &mut self,
        position: Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.viewer_state.seek_bounds;
        if bounds.size.width <= px(0.0) {
            return;
        }
        let ratio = (f32::from(position.x - bounds.origin.x) / f32::from(bounds.size.width))
            .clamp(0.0, 1.0);
        if let Some(player) = &self.player {
            let duration = player.read(cx).duration().unwrap_or_default();
            player.update(cx, |player, cx| player.seek(duration.mul_f32(ratio), cx));
        }
        self.show_video_controls(cx);
    }

    pub(super) fn volume_video_pointer(
        &mut self,
        position: Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.viewer_state.volume_bounds;
        if bounds.size.width <= px(0.0) {
            return;
        }
        let volume = (f32::from(position.x - bounds.origin.x) / f32::from(bounds.size.width))
            .clamp(0.0, 1.0);
        if let Some(player) = &self.player {
            player.update(cx, |player, cx| {
                player.set_volume(volume, cx);
                player.set_muted(false, cx);
            });
        }
        self.show_video_controls(cx);
    }

    pub(super) fn video_toolbar(
        &self,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some(player) = &self.player else {
            return div();
        };
        let video = player.read(cx);
        let playing = video.is_playing();
        let fullscreen = self.viewer_state.fullscreen;
        let visible = !fullscreen || !playing || Instant::now() < self.viewer_state.controls_until;
        if fullscreen && playing && visible {
            window.request_animation_frame();
        }
        if !visible {
            return div();
        }
        let duration = video.duration().unwrap_or_default().as_secs_f64();
        let position = video.position().as_secs_f64();
        let progress = if duration > 0.0 {
            (position / duration) as f32
        } else {
            0.0
        };
        let volume = if video.is_muted() {
            0.0
        } else {
            video.volume()
        };
        let toolbar_width = if fullscreen {
            width
        } else {
            760.0_f32.min(width - 144.0)
        };
        let compact = width <= 1000.0;
        let padding = if compact { 10.0 } else { 18.0 };
        let volume_width = if compact { 54.0 } else { 76.0 };
        let gallery = cx.weak_entity();
        let volume_gallery = gallery.clone();
        let seek_width = toolbar_width - padding * 2.0 - 2.0;

        div()
            .absolute()
            .bottom(px(if fullscreen { 0.0 } else { 24.0 }))
            .left(px((width - toolbar_width) / 2.0))
            .w(px(toolbar_width))
            .px(px(padding))
            .py(px(if compact { 10.0 } else { 12.0 }))
            .bg(rgb(0x141420))
            .border_1()
            .border_color(rgb(0x514361))
            .rounded(px(if fullscreen { 0.0 } else { 24.0 }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("video-seek")
                    .relative()
                    .h(px(5.0))
                    .mb(px(10.0))
                    .cursor_pointer()
                    .child(
                        gpui::canvas(
                            |_, _, _| (),
                            move |bounds, _, _, cx| {
                                let _ = gallery
                                    .update(cx, |this, _| this.viewer_state.seek_bounds = bounds);
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(track(seek_width, progress).top_0())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.viewer_state.seeking = true;
                            this.seek_video_pointer(event.position, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(if compact { 5.0 } else { 10.0 }))
                    .child(
                        icon_button("video-play", if playing { "pause" } else { "play" }, false)
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_video(cx))),
                    )
                    .child(
                        icon_button("video-repeat", "repeat", self.viewer_state.repeat).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.viewer_state.repeat = !this.viewer_state.repeat;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        div()
                            .min_w(px(80.0))
                            .text_size(px(12.0))
                            .text_color(rgb(MUTED))
                            .child(format!("{} / {}", time(position), time(duration))),
                    )
                    .child(
                        icon_button(
                            "video-mute",
                            if volume > 0.0 { "volume-2" } else { "volume-x" },
                            false,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.mute_video(cx))),
                    )
                    .child(
                        div()
                            .id("video-volume")
                            .relative()
                            .w(px(volume_width))
                            .h(px(12.0))
                            .cursor_pointer()
                            .child(
                                gpui::canvas(
                                    |_, _, _| (),
                                    move |bounds, _, _, cx| {
                                        let _ = volume_gallery.update(cx, |this, _| {
                                            this.viewer_state.volume_bounds = bounds
                                        });
                                    },
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(track(volume_width, volume))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                    this.viewer_state.volume_drag = true;
                                    this.volume_video_pointer(event.position, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        label_button("video-speed", "1×")
                            .keyboard_focus(false)
                            .w(px(88.0))
                            .h(px(36.0))
                            .min_h(px(36.0))
                            .py_0()
                            .px(px(10.0))
                            .rounded(px(7.0))
                            .justify_between()
                            .border_1()
                            .border_color(rgb(crate::ui::LINE))
                            .bg(rgb(0x1a1927))
                            .gap(px(8.0))
                            .when(fullscreen, |view| view.opacity(0.35))
                            .child(crate::ui::shell::icon("down").size(px(14.0))),
                    )
                    .child(
                        icon_button("video-diagnostics", "info", self.viewer_state.diagnostics)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.viewer_state.diagnostics = !this.viewer_state.diagnostics;
                                cx.notify();
                            })),
                    )
                    .child(
                        icon_button(
                            "video-fullscreen",
                            if self.viewer_state.fullscreen {
                                "player-fullscreen-exit"
                            } else {
                                "player-fullscreen"
                            },
                            false,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_video_fullscreen(window, cx)
                        })),
                    ),
            )
    }

    pub(super) fn video_diagnostics(&self, cx: &Context<Self>) -> Div {
        let stats = self
            .player
            .as_ref()
            .and_then(|player| player.read(cx).player())
            .map(|player| player.stats());
        let rows = vec![
            ("FPS файла", "—".to_owned()),
            ("Кадров композитору / с", "—".into()),
            ("Вызовов кадров / с", "—".into()),
            (
                "Пропущено / всего кадров",
                stats
                    .as_ref()
                    .map(|stats| format!("{} / —", stats.frames_dropped))
                    .unwrap_or_else(|| "— / —".into()),
            ),
            ("Буфер впереди", "— s".into()),
            ("Декодирование кадра", "— ms".into()),
        ];
        let measurements = rows.into_iter().map(|(label, value)| {
            div()
                .my(px(4.0))
                .flex()
                .gap(px(12.0))
                .child(div().flex_1().text_color(rgb(MUTED)).child(label))
                .child(value)
        });
        let note = div().mt(px(10.0)).line_height(px(18.0)).text_color(rgb(MUTED))
            .child("Данные движка alhazen. Недоступные измерения отмечены знаком —. Частота физического отображения монитором не измеряется.");

        div()
            .absolute()
            .top(px(12.0))
            .left(px(12.0))
            .w(px(330.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(gpui::rgba(0xffffff30))
            .bg(gpui::rgba(0x0a0a12c2))
            .text_size(px(12.0))
            .children(measurements)
            .child(note)
    }
}

fn track(width: f32, value: f32) -> Div {
    let progress = value.clamp(0.0, 1.0) * width;
    div()
        .absolute()
        .top(px(3.5))
        .h(px(5.0))
        .w_full()
        .rounded(px(8.0))
        .bg(rgb(0x514361))
        .child(
            div()
                .h_full()
                .w(px(progress))
                .rounded(px(8.0))
                .bg(rgb(ACCENT)),
        )
        .child(
            div()
                .absolute()
                .top(px(-3.5))
                .left(px((progress - 6.0).clamp(0.0, width - 12.0)))
                .size(px(12.0))
                .rounded_full()
                .bg(rgb(ACCENT)),
        )
}

pub(super) fn time(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    if seconds >= 3600 {
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}
