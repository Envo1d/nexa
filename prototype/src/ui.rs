use std::{collections::HashMap, path::PathBuf, rc::Rc, sync::Arc, time::Duration};

use gpui::{
    App, Context, Div, Entity, FocusHandle, ObjectFit, PathPromptOptions, Render, RenderImage,
    Subscription, Task, UniformListScrollHandle, Window, div, img, prelude::*, px, rgb,
};
use gpui_component::input::{InputEvent, InputState};

mod assets;
mod background;
mod collection;
mod details;
mod filters;
mod health;
mod layout;
mod motion;
mod selection;
mod shell;
mod sort_select;
mod stage;
mod taxonomy;
mod toolbar;

pub(crate) use assets::Assets;

gpui::actions!(nexa, [FocusSearch, CloseDetails]);

pub(super) fn init(cx: &mut App) {
    tracing::debug!(
        operation = "window_controls",
        drag_backend = "native_caption_hit_test",
        focus_scope = "workspace",
        "Custom titlebar drag regions configured"
    );

    cx.bind_keys([
        gpui::KeyBinding::new("ctrl-f", FocusSearch, Some("NexaGallery")),
        gpui::KeyBinding::new("escape", CloseDetails, Some("NexaGallery")),
    ]);
}

use crate::{
    catalog::{self, Entry},
    media::Player,
};

const BACKGROUND: u32 = 0x080910;
const SHELL: u32 = 0x0b0b13;
const SURFACE: u32 = 0x141420;
const LINE: u32 = 0x252434;
const TEXT: u32 = 0xeeedf6;
const MUTED: u32 = 0xa39bb5;
const ACCENT: u32 = 0xa786ff;

pub(super) struct Gallery {
    health_open: bool,
    health: health::Health,
    taxonomy_open: bool,
    taxonomy: taxonomy::Editor,
    stage: bool,
    inactive_workspace: stage::Workspace,
    stage_selection: std::collections::BTreeSet<usize>,
    marquee: Option<selection::Marquee>,
    marquee_frame_pending: bool,
    collection_bounds: gpui::Bounds<gpui::Pixels>,
    entries: Vec<Entry>,
    tag_counts: Vec<(String, usize)>,
    visible: Vec<usize>,
    scroll: UniformListScrollHandle,
    details_scroll: gpui::ScrollHandle,
    tags_scroll: gpui::ScrollHandle,
    details_motion: Option<details::ScrollMotion>,
    details_frame_pending: bool,
    collection_width: f32,
    previews: HashMap<usize, Arc<RenderImage>>,
    shell_background: Arc<RenderImage>,
    search: Entity<InputState>,
    focus: FocusHandle,
    selected: Option<usize>,
    videos_only: bool,
    images_only: bool,
    favorites_only: bool,
    tagged_only: bool,
    hide_untagged: bool,
    show_previews: bool,
    show_all_tags: bool,
    sidebar_expanded: bool,
    list_mode: bool,
    sort: catalog::SortMode,
    sort_open: bool,
    sort_active: usize,
    sort_focus: FocusHandle,
    type_counts: [usize; 3],
    details_open: bool,
    drawer_transition: motion::Transition,
    sort_transition: motion::Transition,
    loading: bool,
    dialog_open: bool,
    viewer: bool,
    overlay: bool,
    dll: Option<PathBuf>,
    player: Option<Rc<Player>>,
    status: String,
    notice: Option<String>,
    poll_task: Option<Task<()>>,
    _search_subscription: Subscription,
}

impl Gallery {
    pub(super) fn new(dll: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx
            .new(|cx| InputState::new(window, cx).placeholder("Метаданные через точку с запятой"));
        let focus = cx.focus_handle();
        focus.focus(window);
        let subscription = cx.subscribe(&search, |this: &mut Self, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.refilter(cx);
            }
        });
        let gallery = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            let _ = gallery.update(cx, |this, _| {
                // Cancel polling and stop libmpv while its HWND is still valid.
                this.poll_task = None;
                this.player = None;
            });
            if cx.windows().len() == 1 {
                // GPUI 0.2.2 otherwise destroys HWND in default WM_CLOSE before
                // deferred activation/OLE/destructor work has finished with it.
                tracing::info!(operation = "shutdown", "Main window close requested");
                cx.quit();
                false
            } else {
                true
            }
        });
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        if let Some(result) = this.player.as_ref().and_then(|player| player.poll())
                        {
                            match result {
                                Ok(status) => this.status = status,
                                Err(error) => this.fail(error),
                            }
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        tracing::info!(records = catalog::DEMO_COUNT, "Demo gallery initialized");

        let entries = catalog::demo_entries();
        let tag_counts = catalog::tag_counts(&entries);
        let videos = entries.iter().filter(|entry| entry.video).count();
        let type_counts = [entries.len(), videos, entries.len() - videos];

        Self {
            health_open: false,
            health: health::Health::new(),
            taxonomy_open: false,
            taxonomy: taxonomy::Editor::new(window, cx),
            stage: false,
            inactive_workspace: stage::Workspace::incoming(),
            stage_selection: Default::default(),
            marquee: None,
            marquee_frame_pending: false,
            collection_bounds: Default::default(),
            entries,
            tag_counts,
            visible: (0..catalog::DEMO_COUNT).collect(),
            scroll: UniformListScrollHandle::new(),
            details_scroll: gpui::ScrollHandle::new(),
            tags_scroll: gpui::ScrollHandle::new(),
            details_motion: None,
            details_frame_pending: false,
            collection_width: 0.0,
            previews: HashMap::new(),
            shell_background: background::shell_image(),
            search,
            focus,
            selected: None,
            videos_only: false,
            images_only: false,
            favorites_only: false,
            tagged_only: false,
            hide_untagged: false,
            show_previews: true,
            show_all_tags: false,
            sidebar_expanded: false,
            list_mode: false,
            sort: catalog::SortMode::Newest,
            sort_open: false,
            sort_active: 0,
            sort_focus: cx.focus_handle(),
            type_counts,
            details_open: false,
            drawer_transition: motion::Transition::new(0.0, 220),
            sort_transition: motion::Transition::new(0.0, 150),
            loading: false,
            dialog_open: false,
            viewer: false,
            overlay: true,
            dll,
            player: None,
            status: "Demo library · 10,000 records · no production catalog opened".into(),
            notice: None,
            poll_task: Some(poll_task),
            _search_subscription: subscription,
        }
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        self.marquee = None;
        (self.visible, self.type_counts) = catalog::browse_with_counts(
            &self.entries,
            &self.search.read(cx).value(),
            catalog::BrowseFilter {
                path_search: self.stage,
                videos_only: self.videos_only,
                images_only: self.images_only,
                favorites_only: !self.stage && self.favorites_only,
                tagged_only: !self.stage && (self.tagged_only || self.hide_untagged),
                sort: self.sort,
            },
        );
        tracing::debug!(
            operation = "browse",
            results = self.visible.len(),
            "Session collection filtered"
        );
        self.scroll.scroll_to_item(0, gpui::ScrollStrategy::Top);
        cx.notify();
    }

    fn add_search(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.search.read(cx).value().to_string();
        let mut terms: Vec<_> = current
            .split(';')
            .map(str::trim)
            .filter(|term| !term.is_empty())
            .map(str::to_owned)
            .collect();
        if !terms
            .iter()
            .any(|term| term.to_lowercase() == value.to_lowercase())
        {
            terms.push(value.to_owned());
        }
        self.search.update(cx, |state, cx| {
            state.set_value(terms.join("; "), window, cx)
        });
        self.refilter(cx);
    }

    fn fail(&mut self, message: String) {
        if self.status == message {
            return;
        }
        if message.starts_with("Playback command ") {
            tracing::error!(reason = %message, "Playback input failed");
        } else {
            tracing::error!("Prototype operation failed; details shown in UI");
        }
        self.status = message;
        self.notice = Some(self.status.clone());
    }

    fn release_previews(&mut self, cx: &mut App) {
        for (_, image) in self.previews.drain() {
            cx.drop_image(image, None);
        }
    }

    fn choose_files(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.loading || self.dialog_open {
            return;
        }
        self.loading = true;
        self.dialog_open = true;
        self.notice = None;
        let prompt = crate::dialogs::open(
            window,
            cx,
            PathPromptOptions {
                files: true,
                directories: false,
                multiple: true,
                prompt: Some("Open media read-only".into()),
            },
            "open_media",
        );
        cx.spawn(async move |this, cx| {
            let paths = match prompt.await {
                Ok(Some(paths)) => paths,
                Ok(None) => {
                    let _ = this.update(cx, |this, cx| {
                        this.loading = false;
                        this.dialog_open = false;
                        cx.notify();
                    });
                    return;
                }
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.loading = false;
                        this.dialog_open = false;
                        this.fail(error);
                        cx.notify();
                    });
                    return;
                }
            };
            let _ = this.update(cx, |this, _| this.dialog_open = false);
            let truncated = paths.len() > catalog::MAX_OPEN_FILES;
            tracing::info!(
                selected = paths.len(),
                truncated,
                "Read-only media preparation started"
            );
            let task = cx
                .background_executor()
                .spawn(async move { crate::previews::prepare(paths) });
            let prepared = task.await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                let prepared = match prepared {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        this.fail(error);
                        cx.notify();
                        return;
                    }
                };
                let crate::previews::Prepared { entries, previews } = prepared;

                if entries.is_empty() {
                    this.status =
                        "No supported media selected. The current library is unchanged.".into();
                    this.notice = Some(
                        "Поддерживаемые файлы не выбраны. Текущая библиотека сохранена.".into(),
                    );
                } else {
                    this.player = None;
                    this.viewer = false;
                    this.taxonomy_open = false;
                    this.selected = None;
                    this.stage_selection.clear();
                    this.release_previews(cx);
                    this.entries = entries;
                    this.tag_counts = catalog::tag_counts(&this.entries);
                    this.previews = previews;
                    this.status = format!(
                        "{} read-only files · {} previews{}",
                        this.entries.len(),
                        this.previews.len(),
                        if truncated {
                            " · selection limited to 64 files"
                        } else {
                            ""
                        }
                    );
                    this.refilter(cx);
                    tracing::info!(
                        files = this.entries.len(),
                        previews = this.previews.len(),
                        "Read-only media preparation completed"
                    );
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn choose_dll(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.loading || self.dialog_open {
            return;
        }
        self.dialog_open = true;
        self.notice = None;
        let prompt = crate::dialogs::open(
            window,
            cx,
            PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some("Select a trusted libmpv DLL".into()),
            },
            "select_media_engine",
        );
        cx.spawn(async move |this, cx| {
            let result = prompt.await;
            let _ = this.update(cx, |this, cx| {
                this.dialog_open = false;
                match result {
                    Ok(Some(paths)) => {
                        this.dll = paths.into_iter().next();
                        this.status =
                            "libmpv path selected. Open a video to validate playback.".into();
                        this.notice = Some(
                            "Медиадвижок выбран. Откройте локальное видео для просмотра.".into(),
                        );
                    }
                    Ok(None) => {}
                    Err(error) => this.fail(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_open {
            return;
        }

        let Some(index) = self.selected else {
            return;
        };
        let entry = &self.entries[index];
        if entry.video && !entry.demo {
            let Some(dll) = &self.dll else {
                self.status = "Select a trusted libmpv DLL first using Media engine.".into();
                self.notice = Some("Сначала выберите libmpv: Настройки → Медиадвижок.".into());
                tracing::warn!(
                    operation = "open_preview",
                    reason = "media_engine_required",
                    "Video preview requires media engine selection"
                );
                cx.notify();
                return;
            };
            // Release the old native surface before creating its replacement.
            self.player = None;
            self.viewer = false;

            match Player::open(dll, &entry.path, window) {
                Ok(player) => {
                    self.overlay = true;
                    self.player = Some(Rc::new(player));
                    self.viewer = true;
                    self.status = "Opening native video…".into();
                }
                Err(error) => self.fail(error),
            }
        } else {
            self.viewer = true;
            self.status = if entry.demo {
                "Demo image preview"
            } else {
                "Bounded 512 px Shell preview · original file is unchanged"
            }
            .into();
        }
        cx.notify();
    }

    fn playback(&mut self, arguments: &[&str], cx: &mut Context<Self>) {
        if let Some(player) = &self.player
            && let Err(error) = player.send(arguments)
        {
            self.fail(error);
        }
        cx.notify();
    }

    fn picture(&self, index: usize) -> Div {
        self.picture_fitted(index, false)
    }

    fn picture_fitted(&self, index: usize, cover: bool) -> Div {
        self.picture_with_corners(index, cover, None)
    }

    fn picture_with_corners(&self, index: usize, cover: bool, card_list: Option<bool>) -> Div {
        let entry = &self.entries[index];
        div()
            .size_full()
            .overflow_hidden()
            .bg(rgb(BACKGROUND))
            .when(card_list.is_none() && cover, |view| view.rounded(px(5.0)))
            .when(entry.demo, |this| {
                this.child(
                    img(entry.path.clone())
                        .size_full()
                        .when(card_list.is_none() && cover, |image| image.rounded(px(5.0)))
                        .when(card_list == Some(false), |image| image.rounded_t(px(7.0)))
                        .when(card_list == Some(true), |image| image.rounded_l(px(7.0)))
                        .object_fit(if cover {
                            ObjectFit::Cover
                        } else {
                            ObjectFit::Contain
                        }),
                )
            })
            .when_some(self.previews.get(&index).cloned(), |this, preview| {
                this.child(
                    img(preview)
                        .size_full()
                        .when(card_list.is_none() && cover, |image| image.rounded(px(5.0)))
                        .when(card_list == Some(false), |image| image.rounded_t(px(7.0)))
                        .when(card_list == Some(true), |image| image.rounded_l(px(7.0)))
                        .object_fit(if cover {
                            ObjectFit::Cover
                        } else {
                            ObjectFit::Contain
                        }),
                )
            })
            .when(!entry.demo && !self.previews.contains_key(&index), |this| {
                this.flex()
                    .items_center()
                    .justify_center()
                    .child(if entry.video {
                        "Video"
                    } else {
                        "Preview unavailable"
                    })
            })
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.stage || self.viewer || !window.is_window_active() {
            self.marquee = None;
        }
        let now = std::time::Instant::now();
        self.drawer_transition.retarget(
            if self.details_open && !self.viewer {
                1.0
            } else {
                0.0
            },
            now,
        );
        self.sort_transition
            .retarget(if self.sort_open { 1.0 } else { 0.0 }, now);
        let (mut drawer_amount, drawer_moving) = self.drawer_transition.sample(now);
        let (_, sort_moving) = self.sort_transition.sample(now);
        if !motion::enabled() {
            drawer_amount = if self.details_open { 1.0 } else { 0.0 };
        }
        if motion::enabled() && (drawer_moving || sort_moving) {
            window.request_animation_frame();
        }

        // A rendered frame must not keep the native player alive after its owner
        // closes the viewer/window or replaces the active source.
        let media = self.player.as_ref().map(Rc::downgrade);
        let content = if self.viewer {
            div()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .when_some(media, |this, player| {
                    this.child(
                        gpui::canvas(
                            |_, _, _| (),
                            move |bounds, (), window, _| {
                                if let Some(player) = player.upgrade() {
                                    player.resize(bounds, window.scale_factor());
                                }
                            },
                        )
                        .size_full(),
                    )
                })
                .when(self.player.is_none(), |this| {
                    if let Some(index) = self.selected {
                        this.child(self.picture(index))
                    } else {
                        this
                    }
                })
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(self.collection(window, cx))
                .when(self.visible.is_empty(), |this| {
                    this.child(div().p_4().child("No matching files"))
                })
                .into_any_element()
        };

        div()
            .key_context("NexaGallery")
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                if this.health_open {
                    return;
                }
                let search = if this.taxonomy_open {
                    &this.taxonomy.search
                } else {
                    &this.search
                };
                search.update(cx, |state, cx| state.focus(window, cx));
            }))
            .on_action(cx.listener(|this, _: &CloseDetails, window, cx| {
                if this.taxonomy_open {
                    this.reset_taxonomy(window, cx);
                }
                this.marquee = None;
                this.details_open = false;
                this.notice = None;
                cx.notify();
            }))
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, _| {
                    if let Some(player) = &this.player {
                        let release = if event.pressed_button.is_none() {
                            Some(false)
                        } else {
                            None
                        };
                        if let Err(error) =
                            player.pointer(event.position, window.scale_factor(), release)
                        {
                            this.fail(error);
                        }
                    }
                }),
            )
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, _| {
                    if let Some(player) = &this.player
                        && let Err(error) =
                            player.pointer(event.position, window.scale_factor(), Some(true))
                    {
                        this.fail(error);
                    }
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, window, _| {
                    if let Some(player) = &this.player
                        && let Err(error) =
                            player.pointer(event.position, window.scale_factor(), Some(false))
                    {
                        this.fail(error);
                    }
                    if !this.dialog_open
                        && let Some(player) = &this.player
                        && player.take_fullscreen_request()
                    {
                        window.toggle_fullscreen();
                    }
                }),
            )
            .size_full()
            .relative()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(rgb(SHELL))
            .text_color(rgb(TEXT))
            .text_size(px(13.0))
            .font_family("Segoe UI")
            .child(
                div().absolute().size_full().child(
                    img(self.shell_background.clone())
                        .size_full()
                        .object_fit(ObjectFit::Fill),
                ),
            )
            .child(self.titlebar(window, cx))
            .child(
                div()
                    // A focused titlebar ancestor prevents the default native
                    // caption action in GPUI's Windows mouse-down dispatcher.
                    .track_focus(&self.focus)
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .px_2()
                    .pb_2()
                    .gap(px(2.0))
                    .child(self.navigation(cx))
                    .when(self.health_open, |view| {
                        view.child(self.health_workspace(cx))
                    })
                    .when(
                        !self.health_open && !self.taxonomy_open && !self.viewer && !self.stage,
                        |view| view.child(self.filters(window, cx)),
                    )
                    .when(
                        !self.health_open && !self.taxonomy_open && !self.viewer && self.stage,
                        |view| view.child(self.stage_sidebar(window, cx)),
                    )
                    .when(self.taxonomy_open, |view| {
                        view.child(self.taxonomy_workspace(cx))
                    })
                    .when(!self.health_open && !self.taxonomy_open, |view| {
                        view.child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .min_h_0()
                                .border_1()
                                .border_color(rgb(LINE))
                                .rounded_lg()
                                .bg(rgb(0x10101a))
                                .when(!self.stage || self.viewer, |view| view.px_4())
                                .pb(px(12.0))
                                .child(
                                    div()
                                        .when(self.stage && !self.viewer, |view| view.px_4())
                                        .child(self.toolbar(cx)),
                                )
                                .when(self.stage && !self.viewer, |view| {
                                    view.child(
                                        div()
                                            .px_4()
                                            .flex_shrink_0()
                                            .child(self.stage_selection_bar(cx)),
                                    )
                                })
                                .child(content)
                                .when(self.player.is_some(), |this| {
                                    this.child(
                            div()
                                .flex()
                                .gap_3()
                                .child(button("pause", "Play / Pause").on_click(cx.listener(
                                    |this, _, _, cx| this.playback(&["cycle", "pause"], cx),
                                )))
                                .child(button("rewind", "−10 s").on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.playback(&["seek", "-10", "relative"], cx)
                                    },
                                )))
                                .child(button("forward", "+10 s").on_click(cx.listener(
                                    |this, _, _, cx| this.playback(&["seek", "10", "relative"], cx),
                                )))
                                .child(button("mute", "Mute").on_click(cx.listener(
                                    |this, _, _, cx| this.playback(&["cycle", "mute"], cx),
                                )))
                                .child(
                                    button(
                                        "overlay",
                                        if self.overlay {
                                            "Hide video controls"
                                        } else {
                                            "Show video controls"
                                        },
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.overlay = !this.overlay;
                                        let visibility =
                                            if this.overlay { "always" } else { "never" };
                                        tracing::info!(
                                            visibility,
                                            "OSC visibility requested; native verification pending"
                                        );
                                        if let Some(player) = &this.player
                                            && let Err(error) =
                                                player.set_panel_visible(this.overlay)
                                        {
                                            this.fail(error);
                                        }
                                        cx.notify();
                                    })),
                                )
                                .child(button("fullscreen", "Fullscreen").on_click(cx.listener(
                                    |this, _, window, _| {
                                        if !this.dialog_open {
                                            window.toggle_fullscreen();
                                        }
                                    },
                                ))),
                        )
                                })
                                .when(self.viewer, |view| {
                                    view.child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(rgb(MUTED))
                                            .child(self.status.clone()),
                                    )
                                }),
                        )
                    }),
            )
            .when(
                (self.details_open || drawer_amount > 0.0) && !self.viewer,
                |view| {
                    view.child(
                        div()
                            .absolute()
                            .top(px(layout::DRAWER_TOP))
                            .right(px(8.0 - 367.0 * (1.0 - drawer_amount)))
                            .bottom(px(8.0))
                            .w(px(360.0))
                            .opacity(drawer_amount)
                            .when(!self.details_open, |view| {
                                view.capture_any_mouse_down(|_, _, cx| cx.stop_propagation())
                                    .capture_any_mouse_up(|_, _, cx| cx.stop_propagation())
                            })
                            .child(self.details(window, cx)),
                    )
                },
            )
            .when_some(self.notice.clone(), |view, notice| {
                view.child(
                    div()
                        .absolute()
                        .bottom(px(20.0))
                        .right(px(20.0))
                        .w(px(360.0))
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(SURFACE))
                        .occlude()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().text_size(px(12.0)).child(notice))
                        .child(
                            shell::control("dismiss-notice", "", "close", false)
                                .size(px(28.0))
                                .px_1()
                                .py_1()
                                .border_0()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.notice = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
    }
}

fn button(id: impl Into<gpui::ElementId>, title: &str) -> motion::MotionDiv {
    button_with_hover(id, title, 0x242034)
}

fn button_with_hover(
    id: impl Into<gpui::ElementId>,
    title: &str,
    hover_background: u32,
) -> motion::MotionDiv {
    motion::button(id)
        .px_3()
        .py_2()
        .rounded_md()
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgb(LINE))
        .cursor_pointer()
        .hover_colors(Some(rgb(hover_background).into()), None, None)
        .when(!title.is_empty(), |view| view.child(title.to_owned()))
}
