use std::path::PathBuf;

use gpui::{
    BoxShadow, Context, FocusHandle, IntoElement, MouseButton, Pixels, Point, Size, Window,
    anchored, deferred, div, point, prelude::*, px,
};

use super::{Gallery, TEXT, file_operations::Action, shell::icon};
use crate::appearance::{rgb, rgba};

gpui::actions!(
    card_context,
    [MenuUp, MenuDown, MenuHome, MenuEnd, MenuConfirm, MenuCancel]
);

pub(super) fn init(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("up", MenuUp, Some("NexaCardContextMenu")),
        gpui::KeyBinding::new("down", MenuDown, Some("NexaCardContextMenu")),
        gpui::KeyBinding::new("home", MenuHome, Some("NexaCardContextMenu")),
        gpui::KeyBinding::new("end", MenuEnd, Some("NexaCardContextMenu")),
        gpui::KeyBinding::new("enter", MenuConfirm, Some("NexaCardContextMenu")),
        gpui::KeyBinding::new("space", MenuConfirm, Some("NexaCardContextMenu")),
        gpui::KeyBinding::new("escape", MenuCancel, Some("NexaCardContextMenu")),
        gpui::KeyBinding::new("tab", MenuCancel, Some("NexaCardContextMenu")),
        gpui::KeyBinding::new("shift-tab", MenuCancel, Some("NexaCardContextMenu")),
    ]);
}

pub(super) struct Menu {
    index: usize,
    source: PathBuf,
    stage: bool,
    position: Point<Pixels>,
    viewport: Size<Pixels>,
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    active: usize,
}

#[derive(Clone, Copy)]
enum Command {
    File(Action),
    Metadata,
    Distribute,
}

struct Item {
    label: String,
    icon: &'static str,
    command: Command,
}

fn items(stage: bool, video: bool, count: usize) -> Vec<Item> {
    let mut items = vec![
        Item {
            label: crate::i18n::text("Открыть").into(),
            icon: "arrow",
            command: Command::File(Action::Open),
        },
        Item {
            label: crate::i18n::text("Показать в Проводнике").into(),
            icon: "folder",
            command: Command::File(Action::Reveal),
        },
        Item {
            label: if stage && count > 1 {
                if crate::i18n::english() {
                    format!("Distribute {count} files")
                } else {
                    format!("Распределить {count} файлов")
                }
            } else {
                crate::i18n::text(if stage {
                    "Распределить"
                } else {
                    "Редактировать метаданные"
                })
                .into()
            },
            icon: if stage { "share" } else { "tag" },
            command: if stage {
                Command::Distribute
            } else {
                Command::Metadata
            },
        },
    ];

    if !stage && video {
        items.push(Item {
            label: crate::i18n::text("Копировать имя файла").into(),
            icon: "copy",
            command: Command::File(Action::CopyName),
        });
    }
    if !stage || count == 1 {
        items.push(Item {
            label: crate::i18n::text(if stage {
                "Заменить в Main Library…"
            } else {
                "Изменить порядок файлов"
            })
            .into(),
            icon: "arrow-up-down",
            command: Command::File(if stage {
                Action::Replace
            } else {
                Action::Order
            }),
        });
    }

    items.push(Item {
        label: if count > 1 {
            if crate::i18n::english() {
                format!("Delete {count} files")
            } else {
                format!("Удалить {count} файлов")
            }
        } else {
            crate::i18n::text("Удалить файл").into()
        },
        icon: "trash",
        command: Command::File(Action::Delete),
    });
    items
}

impl Gallery {
    fn context_menu_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.context_menu.as_ref() else {
            return;
        };
        let Some(entry) = self.entries.get(menu.index) else {
            self.dismiss_card_context_menu(window, cx);
            return;
        };
        if self.stage != menu.stage || entry.path != menu.source {
            self.dismiss_card_context_menu(window, cx);
            return;
        }
        let count = if menu.stage && self.stage_selection.contains(&menu.index) {
            self.stage_selection.len()
        } else {
            1
        };
        let options = items(menu.stage, entry.video, count);
        if key == "escape" {
            self.dismiss_card_context_menu(window, cx);
        } else if key == "enter" {
            self.choose_context_command(options[menu.active].command, window, cx);
        } else if let Some(menu) = self.context_menu.as_mut() {
            menu.active = match key {
                "up" => (menu.active + options.len() - 1) % options.len(),
                "down" => (menu.active + 1) % options.len(),
                "home" => 0,
                "end" => options.len() - 1,
                _ => return,
            };
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn open_card_context_menu(
        &mut self,
        index: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading || self.file_operation.is_some() || self.distribution.is_some() {
            return;
        }
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        let source = entry.path.clone();
        let return_focus = self
            .context_menu
            .take()
            .and_then(|menu| menu.return_focus)
            .or_else(|| window.focused(cx));
        let focus = cx.focus_handle();
        focus.focus(window);
        self.marquee = None;
        self.search_suggestions.open = false;
        self.context_menu = Some(Menu {
            index,
            source,
            stage: self.stage,
            position,
            viewport: window.viewport_size(),
            focus,
            return_focus,
            active: 0,
        });
        tracing::debug!(
            operation = "card_context_menu",
            stage = self.stage,
            "File context menu opened"
        );
        cx.notify();
    }

    fn dismiss_card_context_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = self.context_menu.take()
            && menu.focus.is_focused(window)
            && let Some(focus) = menu.return_focus
        {
            focus.focus(window);
        }
        cx.notify();
    }

    fn choose_context_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.context_menu.as_ref() else {
            return;
        };
        let index = menu.index;
        let valid = self.stage == menu.stage
            && self
                .entries
                .get(index)
                .is_some_and(|entry| entry.path == menu.source);
        self.dismiss_card_context_menu(window, cx);
        if !valid {
            return;
        }

        match command {
            Command::File(action) => self.run_file_action(index, action, window, cx),
            Command::Metadata => self.edit_file_metadata(index, window, cx),
            Command::Distribute => {
                if !self.stage_selection.contains(&index) {
                    self.select_card(index, gpui::Modifiers::default());
                }
                self.open_distribution(window, cx);
            }
        }
    }

    pub(super) fn card_context_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(menu) = self.context_menu.as_ref() else {
            return div().into_any_element();
        };
        let valid = menu.stage == self.stage
            && window.is_window_active()
            && menu.viewport == window.viewport_size()
            && menu.focus.is_focused(window)
            && self
                .entries
                .get(menu.index)
                .is_some_and(|entry| entry.path == menu.source);
        if !valid {
            self.dismiss_card_context_menu(window, cx);
            return div().into_any_element();
        }
        let count = if self.stage && self.stage_selection.contains(&menu.index) {
            self.stage_selection.len()
        } else {
            1
        };
        let options = items(self.stage, self.entries[menu.index].video, count);
        let active = menu.active;
        let position = menu.position;
        let focus = menu.focus.clone();

        deferred(
            anchored()
                .position(position)
                .snap_to_window_with_margin(px(8.))
                .child(
                    div()
                        .id("card-context-menu")
                        .key_context("NexaCardContextMenu")
                        .on_action(cx.listener(|this, _: &MenuUp, window, cx| {
                            this.context_menu_key("up", window, cx)
                        }))
                        .on_action(cx.listener(|this, _: &MenuDown, window, cx| {
                            this.context_menu_key("down", window, cx)
                        }))
                        .on_action(cx.listener(|this, _: &MenuHome, window, cx| {
                            this.context_menu_key("home", window, cx)
                        }))
                        .on_action(cx.listener(|this, _: &MenuEnd, window, cx| {
                            this.context_menu_key("end", window, cx)
                        }))
                        .on_action(cx.listener(|this, _: &MenuConfirm, window, cx| {
                            this.context_menu_key("enter", window, cx)
                        }))
                        .on_action(cx.listener(|this, _: &MenuCancel, window, cx| {
                            this.context_menu_key("escape", window, cx)
                        }))
                        .track_focus(&focus)
                        .occlude()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .w(px(218.))
                        .p(px(6.))
                        .border_1()
                        .border_color(rgb(0x514361))
                        .rounded(px(10.))
                        .bg(rgb(0x242034))
                        .text_color(rgb(TEXT))
                        .shadow(vec![BoxShadow {
                            color: rgba(0x00000080).into(),
                            offset: point(px(0.), px(12.)),
                            blur_radius: px(36.),
                            spread_radius: px(0.),
                        }])
                        .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                            this.dismiss_card_context_menu(window, cx)
                        }))
                        .on_scroll_wheel(cx.listener(|this, _, window, cx| {
                            this.dismiss_card_context_menu(window, cx)
                        }))
                        .children(options.into_iter().enumerate().map(|(index, item)| {
                            let destructive = matches!(item.command, Command::File(Action::Delete));
                            div()
                                .id(("context-action", index))
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .w_full()
                                .p(px(10.))
                                .rounded(px(6.))
                                .cursor_pointer()
                                .text_size(px(12.))
                                .line_height(px(15.))
                                .text_color(rgb(if destructive { 0xf292a7 } else { TEXT }))
                                .when(destructive, |row| {
                                    row.mt(px(3.)).border_t_1().border_color(rgb(0x514361))
                                })
                                .when(active == index, |row| row.bg(rgb(0x2c263e)))
                                .on_hover(cx.listener(move |this, hovered, _, cx| {
                                    if *hovered && let Some(menu) = this.context_menu.as_mut() {
                                        menu.active = index;
                                        cx.notify();
                                    }
                                }))
                                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                                .child(
                                    icon(item.icon)
                                        .size(px(16.))
                                        .flex_shrink_0()
                                        .text_color(rgb(if destructive {
                                            0xf292a7
                                        } else {
                                            0xd0b4fa
                                        })),
                                )
                                .child(div().flex_1().min_w_0().child(item.label))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.choose_context_command(item.command, window, cx);
                                }))
                        })),
                ),
        )
        .with_priority(50)
        .into_any_element()
    }
}
