use std::time::{Duration, Instant};

use alhazen_gpui::{PlayerConfig, PlayerEvent, Source, VideoPlayer};
use gpui::{AppContext, Context, Entity, FocusHandle, Point, Subscription, Window, point};
use gpui_component::input::{InputEvent, InputState};

use super::Gallery;

mod panels;
mod presentation;
mod video;

const UNSUPPORTED: &str = "Этот формат или кодек не поддерживается просмотрщиком Nexa. Откройте файл во внешней программе.";

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Panel {
    Tags,
    Info,
    Help,
}

pub(super) struct Viewer {
    panel: Option<Panel>,
    zoom: f32,
    pan: Point<f32>,
    drag: Option<(Point<gpui::Pixels>, Point<f32>)>,
    natural: Option<(u32, u32)>,
    tag_input: Entity<InputState>,
    focus: FocusHandle,
    error: Option<String>,
    save_error: Option<String>,
    loading: bool,
    generation: u64,
    repeat: bool,
    diagnostics: bool,
    fullscreen: bool,
    controls_until: Instant,
    seek_bounds: gpui::Bounds<gpui::Pixels>,
    volume_bounds: gpui::Bounds<gpui::Pixels>,
    tag_bounds: gpui::Bounds<gpui::Pixels>,
    seeking: bool,
    volume_drag: bool,
    subscriptions: Vec<Subscription>,
    tag_focused: bool,
    tag_active: Option<usize>,
    _tag_subscription: Subscription,
}

impl Viewer {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Gallery>) -> Self {
        let tag_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(crate::i18n::text("Добавить тег…")));
        let tag_subscription = cx.subscribe_in(&tag_input, window, |this, _, event, window, cx| {
            match event {
                InputEvent::PressEnter { .. } => {
                    if let Some(tag) = this
                        .viewer_state
                        .tag_active
                        .and_then(|index| this.viewer_tag_matches(cx).get(index).cloned())
                    {
                        this.viewer_state
                            .tag_input
                            .update(cx, |input, cx| input.set_value(tag, window, cx));
                    }
                    this.add_viewer_tag(window, cx);
                }
                InputEvent::Focus => this.viewer_state.tag_focused = true,
                InputEvent::Blur => this.viewer_state.tag_focused = false,
                InputEvent::Change => this.viewer_state.tag_active = None,
            }
            cx.notify();
        });

        Self {
            panel: None,
            zoom: 1.0,
            pan: point(0.0, 0.0),
            drag: None,
            natural: None,
            tag_input,
            focus: cx.focus_handle(),
            error: None,
            save_error: None,
            loading: false,
            generation: 0,
            repeat: false,
            diagnostics: false,
            fullscreen: false,
            controls_until: Instant::now(),
            seek_bounds: Default::default(),
            volume_bounds: Default::default(),
            tag_bounds: Default::default(),
            seeking: false,
            volume_drag: false,
            subscriptions: Vec::new(),
            tag_focused: false,
            tag_active: None,
            _tag_subscription: tag_subscription,
        }
    }

    fn reset_image(&mut self) {
        self.zoom = 1.0;
        self.pan = point(0.0, 0.0);
        self.drag = None;
    }

    fn fitted(&self, width: f32, height: f32) -> (f32, f32) {
        fit(self.natural, width, height)
    }

    fn clamp_pan(&mut self, width: f32, height: f32) {
        let (fitted_width, fitted_height) = self.fitted(width, height);
        let x = ((fitted_width * self.zoom - width) / 2.0).max(0.0);
        let y = ((fitted_height * self.zoom - height) / 2.0).max(0.0);
        self.pan.x = self.pan.x.clamp(-x, x);
        self.pan.y = self.pan.y.clamp(-y, y);
    }

    fn set_zoom(&mut self, zoom: f32, window: &Window) {
        if !self.image_ready() {
            return;
        }
        self.zoom = zoom.clamp(0.25, 16.0);
        self.clamp_pan(
            window.viewport_size().width.into(),
            f32::from(window.viewport_size().height) - 60.0,
        );
    }

    fn image_ready(&self) -> bool {
        self.natural.is_some() && self.error.is_none() && !self.loading
    }
}

fn fit(natural: Option<(u32, u32)>, width: f32, height: f32) -> (f32, f32) {
    let Some((w, h)) = natural.filter(|(w, h)| *w > 0 && *h > 0) else {
        return (width.max(0.0), height.max(0.0));
    };
    let scale = (width.max(0.0) / w as f32).min(height.max(0.0) / h as f32);
    (
        (w as f32 * scale).min(width.max(0.0)),
        (h as f32 * scale).min(height.max(0.0)),
    )
}

impl Gallery {
    pub(super) fn refresh_viewer_locale(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.viewer_state.tag_input.update(cx, |state, cx| {
            state.set_placeholder(crate::i18n::text("Добавить тег…"), window, cx);
        });
        self.viewer_state.error = None;
        self.viewer_state.save_error = None;
    }

    fn viewer_video_title(&self, cx: &Context<Self>) -> String {
        let entry = &self.entries[self.selected.unwrap_or(0)];
        if entry.demo {
            return entry.title.clone();
        }

        self.player
            .as_ref()
            .and_then(|player| player.read(cx).metadata())
            .and_then(|metadata| metadata.title)
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| crate::i18n::text("Видео").into())
    }

    pub(crate) fn preview_local_files(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries: Vec<_> = paths
            .into_iter()
            .filter(|path| path.is_absolute() && path.is_file())
            .filter_map(crate::catalog::entry)
            .collect();
        if entries.is_empty() {
            return;
        }

        self.entries = entries;
        self.visible = (0..self.entries.len()).collect();
        self.tag_counts = crate::catalog::tag_counts(&self.entries);
        let videos = self.entries.iter().filter(|entry| entry.video).count();
        self.type_counts = [self.entries.len(), videos, self.entries.len() - videos];
        self.selected = Some(0);
        self.open_viewer(window, cx);
    }

    pub(super) fn open_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected else { return };
        self.player = None;
        self.viewer_state.subscriptions.clear();
        self.viewer_state.generation += 1;
        self.viewer_state.reset_image();
        self.viewer_state.error = None;
        self.viewer_state.save_error = None;
        self.viewer_state.natural = None;
        self.viewer_state.loading = true;
        self.viewer_state.repeat = false;
        self.viewer_state.diagnostics = false;
        self.viewer_state.controls_until = Instant::now() + Duration::from_secs(2);
        self.viewer_state
            .tag_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.viewer_state.focus.focus(window);
        self.viewer = true;
        self.marquee = None;
        self.sort_open = false;
        let entry = &self.entries[index];
        let _span =
            tracing::info_span!("open_media_viewer", video = entry.video, demo = entry.demo)
                .entered();
        tracing::info!(
            operation = "media_viewer",
            media_kind = if entry.video { "video" } else { "image" },
            demo = entry.demo,
            "Opening isolated media viewer"
        );

        if entry.video {
            if entry.demo {
                self.viewer_state.loading = false;
                self.viewer_state.error = Some(
                    crate::i18n::text("Воспроизведение видео доступно для реальных файлов в настольном приложении.")
                        .into(),
                );
            } else {
                let source = Source::File(entry.path.clone());
                let player = cx.new(|cx| {
                    VideoPlayer::new(
                        source,
                        PlayerConfig {
                            autoplay: true,
                            ..Default::default()
                        },
                        cx,
                    )
                });
                self.viewer_state
                    .subscriptions
                    .push(cx.observe(&player, |_, _, cx| cx.notify()));
                self.viewer_state.subscriptions.push(cx.subscribe(
                    &player,
                    |this, entity, event, cx| {
                        match event {
                            PlayerEvent::Error(_) => {
                                if this.viewer_state.error.is_none() {
                                    this.viewer_state.error =
                                        Some(crate::i18n::text(UNSUPPORTED).into());
                                    tracing::error!(
                                        operation = "media_viewer",
                                        "Video playback failed"
                                    );
                                }
                            }
                            PlayerEvent::Warning(_) => tracing::warn!(
                                operation = "media_viewer",
                                "Video engine reported a recoverable playback problem"
                            ),
                            PlayerEvent::Ended if this.viewer_state.repeat => {
                                entity.update(cx, |player, cx| {
                                    player.seek(Duration::ZERO, cx);
                                    player.play(cx);
                                })
                            }
                            PlayerEvent::FrameReady => {
                                if this.viewer_state.loading {
                                    let backend = entity
                                        .read(cx)
                                        .player()
                                        .and_then(|player| player.stats().video_backend);
                                    tracing::info!(
                                        operation = "media_viewer",
                                        backend,
                                        "First video frame ready"
                                    );
                                }
                                this.viewer_state.loading = false;
                            }
                            _ => {}
                        }
                        cx.notify();
                    },
                ));
                self.player = Some(player);
            }
        } else {
            let path = entry.path.clone();
            let generation = self.viewer_state.generation;
            cx.spawn(async move |this, cx| {
                let dimensions = cx
                    .background_executor()
                    .spawn(async move {
                        image::ImageReader::open(path)
                            .and_then(|reader| reader.with_guessed_format())
                            .map_err(|_| ())?
                            .into_dimensions()
                            .map_err(|_| ())
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if generation != this.viewer_state.generation || !this.viewer {
                        return;
                    }
                    this.viewer_state.loading = false;
                    match dimensions {
                        Ok(dimensions) => {
                            this.viewer_state.natural = Some(dimensions);
                            tracing::info!(
                                operation = "media_viewer",
                                "Image source ready for GPUI rendering"
                            );
                        }
                        Err(()) => {
                            this.viewer_state.error = Some(crate::i18n::text(UNSUPPORTED).into());
                            tracing::error!(
                                operation = "media_viewer",
                                "Image header could not be read"
                            );
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn close_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.player = None;
        self.viewer_state.subscriptions.clear();
        self.viewer_state.generation += 1;
        self.viewer_state.panel = None;
        if self.viewer_state.fullscreen && window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        self.viewer_state.fullscreen = false;
        self.viewer = false;
        self.restore_duplicate_viewer();
        self.restore_health_preview();
        self.restore_replacement_preview(window, cx);
        (self.visible, self.type_counts) = crate::catalog::browse_with_counts(
            &self.entries,
            &self.collection_query(cx),
            crate::catalog::BrowseFilter {
                path_search: self.stage,
                videos_only: self.videos_only,
                images_only: self.images_only,
                favorites_only: !self.stage && self.favorites_only,
                tagged_only: !self.stage && (self.tagged_only || self.hide_untagged),
                sort: self.sort,
            },
        );
        if let Some(draft) = self.file_operation.as_ref() {
            draft.focus.focus(window);
        } else {
            self.focus.focus(window);
        }
        tracing::info!(operation = "media_viewer", "Media viewer closed");
        cx.notify();
    }

    fn move_viewer(&mut self, direction: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(next) = neighbor(&self.visible, self.selected, direction) else {
            return;
        };
        self.selected = Some(next);
        self.open_viewer(window, cx);
    }

    fn add_viewer_tag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use unicode_normalization::UnicodeNormalization;

        let Some(index) = self.selected else { return };
        let tag: String = self
            .viewer_state
            .tag_input
            .read(cx)
            .value()
            .trim()
            .nfc()
            .collect();
        if tag.is_empty() {
            return;
        }
        if tag.chars().count() > 24
            || tag.chars().any(char::is_control)
            || self.entries[index].tags.len() >= 100
        {
            self.viewer_state.save_error = Some(
                crate::i18n::text(
                    "Можно добавить до 100 тегов длиной до 24 символов, без управляющих символов.",
                )
                .into(),
            );
            cx.notify();
            return;
        }
        let key = crate::naming::name_key(&tag);
        if !self.entries[index]
            .tags
            .iter()
            .any(|tag| crate::naming::name_key(tag) == key)
        {
            self.entries[index].tags.push(tag);
            self.tag_counts = crate::catalog::tag_counts(&self.entries);
            tracing::info!(
                operation = "viewer_annotations",
                scope = "session",
                "Viewer tag added"
            );
        }
        self.viewer_state.save_error = None;
        self.viewer_state
            .tag_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }
}

fn neighbor(visible: &[usize], selected: Option<usize>, direction: isize) -> Option<usize> {
    let position = visible.iter().position(|index| Some(*index) == selected)?;
    position
        .checked_add_signed(direction)
        .and_then(|next| visible.get(next))
        .copied()
}
