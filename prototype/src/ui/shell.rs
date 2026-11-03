use std::path::PathBuf;

use gpui::{Context, Div, Stateful, Window, WindowControlArea, div, img, prelude::*, px, rgb};
use gpui_component::{
    Icon, IconName,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
};

use super::{ACCENT, Gallery, LINE, MUTED, SURFACE, button, button_with_hover};

pub(super) fn icon(name: &'static str) -> Icon {
    let bundled = match name {
        "menu" => Some(IconName::Menu),
        "folder" => Some(IconName::Folder),
        "star" => Some(IconName::Star),
        "heart" => Some(IconName::Heart),
        "settings" => Some(IconName::Settings),
        "search" => Some(IconName::Search),
        "info" => Some(IconName::Info),
        "copy" => Some(IconName::Copy),
        "calendar" => Some(IconName::Calendar),
        "arrow" => Some(IconName::ArrowRight),
        "more" => Some(IconName::Ellipsis),
        "down" => Some(IconName::ChevronDown),
        "minimize" => Some(IconName::Minus),
        "maximize" => Some(IconName::WindowMaximize),
        "close" => Some(IconName::Close),
        _ => None,
    };
    let icon = if let Some(name) = bundled {
        Icon::new(name)
    } else {
        let file = match name {
            "warning" => "circle-alert",
            "grid" => "layout-grid",
            "reset" => "rotate-ccw",
            "person" => "user-round",
            "share" => "share-2",
            other => other,
        };

        Icon::default().path(format!("icons/{file}.svg"))
    };

    icon.size(px(18.0))
}

pub(super) fn control(
    id: &'static str,
    label: &str,
    name: &'static str,
    active: bool,
) -> Stateful<Div> {
    control_with_hover(id, label, name, active, 0x242034)
}

fn control_with_hover(
    id: &'static str,
    label: &str,
    name: &'static str,
    active: bool,
    hover_background: u32,
) -> Stateful<Div> {
    button_with_hover(id, "", hover_background)
        .h(px(36.0))
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(12.0))
        .when(active, |view| {
            view.bg(rgb(0x30263e)).border_color(rgb(0x6d568f))
        })
        .child(icon(name))
        .when(!label.is_empty(), |view| view.child(label.to_owned()))
}

fn window_control(id: &'static str, name: &'static str, size: f32, hover: u32) -> Stateful<Div> {
    button_with_hover(id, "", hover)
        .w(px(44.0))
        .h_full()
        .p_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .text_color(rgb(MUTED))
        .bg(gpui::rgba(0))
        .border_0()
        .child(icon(name).size(px(size)))
}

impl Gallery {
    pub(super) fn titlebar(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let tabs = div()
            .absolute()
            .occlude()
            .left(gpui::relative(0.5))
            .ml(px(-110.0))
            .top_0()
            .bottom_0()
            .flex()
            .h_full()
            .pt_2()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(112.0))
                    .rounded_t(px(9.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .bg(rgb(0x2c263f))
                    .border_b_2()
                    .border_color(rgb(ACCENT))
                    .child("Библиотека"),
            )
            .child(
                button("stage", "Входящие")
                    .bg(gpui::rgba(0))
                    .border_0()
                    .rounded_t(px(9.0))
                    .rounded_b(px(0.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .w(px(104.0))
                    .h_full()
                    .py_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(MUTED))
                    .on_click(cx.listener(|this, _, _, cx| this.unavailable("Входящие", cx))),
            );

        div()
            .relative()
            .flex()
            .items_center()
            .h(px(super::layout::TITLEBAR_HEIGHT))
            .flex_shrink_0()
            .pl(px(20.0))
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .flex_1()
                    .h_full()
                    .window_control_area(WindowControlArea::Drag)
                    .child(
                        img(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../public/icon.png"))
                            .size(px(32.0)),
                    )
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Nexa"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(
                div()
                    .flex()
                    .h(px(44.8))
                    .mr(px(6.5))
                    .child(
                        window_control("minimize", "minimize", 17.0, 0x242034)
                            .on_click(|_, window, _| window.minimize_window()),
                    )
                    .child(
                        window_control(
                            "maximize",
                            if window.is_maximized() {
                                "copy"
                            } else {
                                "square"
                            },
                            15.0,
                            0x242034,
                        )
                        .on_click(|_, window, _| window.zoom_window()),
                    )
                    .child(
                        window_control("close-window", "close", 18.0, 0xa53d5a).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.poll_task = None;
                                this.player = None;
                                tracing::info!(operation = "shutdown", "Titlebar close requested");
                                cx.quit();
                            }),
                        ),
                    ),
            )
            .child(tabs)
    }

    pub(super) fn unavailable(&mut self, feature: &'static str, cx: &mut Context<Self>) {
        self.status = format!("{feature}: перенос этого раздела ещё не выполнен.");
        self.notice = Some(self.status.clone());
        cx.notify();
    }

    pub(super) fn navigation(&self, cx: &mut Context<Self>) -> Div {
        let expanded = self.sidebar_expanded;
        let label = |text| if expanded { text } else { "" };
        let gallery = cx.weak_entity();

        div()
            .flex()
            .flex_col()
            .w(px(if expanded { 146.0 } else { 60.0 }))
            .flex_shrink_0()
            .p_2()
            .pt_3()
            .gap_1()
            .rounded_lg()
            .border_1()
            .border_color(rgb(LINE))
            .bg(gpui::linear_gradient(
                120.0,
                gpui::linear_color_stop(rgb(SURFACE), 0.0),
                gpui::linear_color_stop(rgb(0x10101a), 1.0),
            ))
            .child(
                control("collapse", label("Библиотека"), "menu", false)
                    .border_0()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sidebar_expanded = !this.sidebar_expanded;
                        cx.notify();
                    })),
            )
            .child(div().h(px(8.0)))
            .child(
                control("gallery", label("Галерея"), "folder", !self.favorites_only)
                    .border_0()
                    .when(!self.favorites_only, |view| view.bg(rgb(0x7b43d9)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.favorites_only = false;
                        this.refilter(cx);
                    })),
            )
            .child(
                control("favorites", label("Избранное"), "star", self.favorites_only)
                    .border_0()
                    .when(self.favorites_only, |view| view.bg(rgb(0x7b43d9)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.favorites_only = true;
                        this.refilter(cx);
                    })),
            )
            .child(
                control("tags", label("Теги"), "tag", false)
                    .border_0()
                    .on_click(cx.listener(|this, _, _, cx| this.unavailable("Редактор тегов", cx))),
            )
            .child(
                control("health", label("Состояние"), "warning", false)
                    .border_0()
                    .on_click(
                        cx.listener(|this, _, _, cx| this.unavailable("Состояние библиотеки", cx)),
                    ),
            )
            .child(div().flex_1())
            .child(
                Button::new("settings")
                    .icon(IconName::Settings)
                    .label(label("Настройки"))
                    .ghost()
                    .w_full()
                    .h(px(36.0))
                    .text_color(rgb(MUTED))
                    .dropdown_menu(move |menu, _, _| {
                        let settings = gallery.clone();
                        let files = gallery.clone();
                        let engine = gallery.clone();

                        menu.item(PopupMenuItem::new("Настройки").on_click(move |_, _, cx| {
                            let _ =
                                settings.update(cx, |this, cx| this.unavailable("Настройки", cx));
                        }))
                        .separator()
                        .item(PopupMenuItem::new("Открыть файлы…").on_click(
                            move |_, window, cx| {
                                let _ = files.update(cx, |this, cx| this.choose_files(window, cx));
                            },
                        ))
                        .item(
                            PopupMenuItem::new("Медиадвижок…").on_click(move |_, window, cx| {
                                let _ = engine.update(cx, |this, cx| this.choose_dll(window, cx));
                            }),
                        )
                    }),
            )
    }
}
