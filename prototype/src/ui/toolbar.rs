use gpui::{Context, Div, Stateful, div, prelude::*, px, rgb};
use gpui_component::{
    Sizable,
    button::Button,
    input::Input,
    menu::{DropdownMenu, PopupMenuItem},
};

use crate::catalog::SortMode;

use super::{
    Gallery, LINE, MUTED, button,
    shell::{control, icon},
};

impl Gallery {
    pub(super) fn toolbar(&self, cx: &mut Context<Self>) -> Div {
        if self.viewer {
            return div()
                .flex()
                .items_center()
                .py_3()
                .gap_2()
                .child(button("back", "Назад в библиотеку").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.player = None;
                        this.viewer = false;
                        cx.notify();
                    },
                )))
                .child(
                    button("open-files", "Открыть файлы")
                        .on_click(cx.listener(|this, _, window, cx| this.choose_files(window, cx))),
                )
                .child(
                    button("engine", "Медиадвижок")
                        .on_click(cx.listener(|this, _, window, cx| this.choose_dll(window, cx))),
                );
        }
        let search_empty = self.search.read(cx).value().is_empty();
        let gallery = cx.weak_entity();
        let sort = self.sort;

        div()
            .flex()
            .flex_col()
            .py(px(16.0))
            .gap(px(12.0))
            .flex_shrink_0()
            .child(
                div()
                    .h(px(40.0))
                    .rounded(px(6.0))
                    .bg(rgb(0x1a1827))
                    .border_1()
                    .border_color(rgb(LINE))
                    .child(
                        Input::new(&self.search)
                            .appearance(false)
                            .with_size(gpui_component::Size::Size(px(40.0)))
                            .prefix(icon("search").size(px(17.0)).text_color(rgb(MUTED)))
                            .text_size(px(12.0))
                            .cleanable(true)
                            .when(search_empty, |input| {
                                input.suffix(
                                    div()
                                        .px_1()
                                        .py(px(2.0))
                                        .rounded_sm()
                                        .bg(rgb(0x242034))
                                        .text_color(rgb(MUTED))
                                        .text_size(px(10.0))
                                        .child("Ctrl F"),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .items_center()
                            .gap_2()
                            .child(
                                type_control(
                                    "all",
                                    "Все файлы",
                                    "files",
                                    self.type_counts[0],
                                    !self.videos_only && !self.images_only,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.videos_only = false;
                                        this.images_only = false;
                                        this.refilter(cx);
                                    },
                                )),
                            )
                            .child(
                                type_control(
                                    "videos",
                                    "Видео",
                                    "video",
                                    self.type_counts[1],
                                    self.videos_only,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.videos_only = true;
                                        this.images_only = false;
                                        this.refilter(cx);
                                    },
                                )),
                            )
                            .child(
                                type_control(
                                    "images",
                                    "Изображения",
                                    "image",
                                    self.type_counts[2],
                                    self.images_only,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.videos_only = false;
                                        this.images_only = true;
                                        this.refilter(cx);
                                    },
                                )),
                            ),
                    )
                    .child(
                        Button::new("sort")
                            .label(sort.label())
                            .dropdown_caret(true)
                            .w(px(150.0))
                            .h(px(36.0))
                            .text_size(px(11.0))
                            .bg(rgb(0x1a1927))
                            .border_color(rgb(LINE))
                            .dropdown_menu(move |mut menu, _, _| {
                                for mode in [
                                    SortMode::Newest,
                                    SortMode::Oldest,
                                    SortMode::NameAscending,
                                    SortMode::NameDescending,
                                ] {
                                    let gallery = gallery.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(mode.label())
                                            .checked(mode == sort)
                                            .on_click(move |_, _, cx| {
                                                let _ = gallery.update(cx, |this, cx| {
                                                    this.sort = mode;
                                                    this.refilter(cx);
                                                });
                                            }),
                                    );
                                }
                                menu
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .p(px(3.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(0x10101a))
                            .child(
                                control("grid", "", "grid", !self.list_mode)
                                    .border_0()
                                    .bg(rgb(if self.list_mode { 0x10101a } else { 0x2c263f }))
                                    .text_color(rgb(if self.list_mode {
                                        MUTED
                                    } else {
                                        super::TEXT
                                    }))
                                    .h(px(28.0))
                                    .w(px(34.0))
                                    .justify_center()
                                    .rounded(px(5.0))
                                    .px_2()
                                    .py_1()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.list_mode = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                control("list", "", "list", self.list_mode)
                                    .border_0()
                                    .bg(rgb(if self.list_mode { 0x2c263f } else { 0x10101a }))
                                    .text_color(rgb(if self.list_mode {
                                        super::TEXT
                                    } else {
                                        MUTED
                                    }))
                                    .h(px(28.0))
                                    .w(px(34.0))
                                    .justify_center()
                                    .rounded(px(5.0))
                                    .px_2()
                                    .py_1()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.list_mode = true;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        control("details", "", "info", self.details_open)
                            .size(px(34.0))
                            .px(px(7.0))
                            .py(px(7.0))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.details_open = !this.details_open;
                                cx.notify();
                            })),
                    ),
            )
            .when(self.loading, |view| {
                view.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(MUTED))
                        .child("Подготовка выбранных файлов…"),
                )
            })
    }
}

fn type_control(
    id: &'static str,
    label: &str,
    name: &'static str,
    count: usize,
    active: bool,
) -> Stateful<Div> {
    control(id, label, name, active)
        .h(px(44.0))
        .px(px(12.0))
        .gap_2()
        .child(
            div()
                .px(px(6.0))
                .py(px(3.0))
                .rounded(px(6.0))
                .text_size(px(12.0))
                .text_color(rgb(if active { 0xa786ff } else { MUTED }))
                .bg(rgb(0x1a1827))
                .child(count.to_string()),
        )
}
