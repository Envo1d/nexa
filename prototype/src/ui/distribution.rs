use gpui::{Context, Div, Entity, Focusable, Window, div, img, prelude::*, px, rgb};
use gpui_component::input::{Input, InputEvent, InputState};

use super::{Gallery, LINE, MUTED, SHELL, SURFACE, TEXT, motion, shell};

mod batch;
mod metadata;
mod presets;
mod workflow;

const STEPS: [&str; 6] = [
    "Информация",
    "Произведения",
    "Теги",
    "Персонажи",
    "Сведения",
    "Подтверждение",
];

pub(super) struct Wizard {
    queue: Vec<usize>,
    position: usize,
    step: usize,
    done: bool,
    completed: usize,
    all: bool,
    retain: bool,
    batch_expanded: bool,
    batch_scroll: presets::scrolling::ScrollState,
    scope_focus: [gpui::FocusHandle; 2],
    values: [Vec<String>; 3],
    query: Entity<InputState>,
    artist: Entity<InputState>,
    title: Entity<InputState>,
    scroll: gpui::ScrollHandle,
    background: std::sync::Arc<gpui::RenderImage>,
    backdrop: Option<std::sync::Arc<gpui::RenderImage>>,
    source_previews: std::collections::HashMap<usize, std::sync::Arc<gpui::RenderImage>>,
    presets: bool,
    preset_state: presets::State,
    save_preset_focus: gpui::FocusHandle,
    focus: gpui::FocusHandle,
    close_focus: gpui::FocusHandle,
    next_focus: gpui::FocusHandle,
    copyright: usize,
    assignment: Option<String>,
    focused: bool,
    artist_focused: bool,
    active: Option<usize>,
    _artist_subscription: gpui::Subscription,
    error: Option<String>,
    _query_subscription: gpui::Subscription,
}

impl Gallery {
    pub(super) fn open_distribution(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let queue: Vec<_> = self.stage_selection.iter().copied().collect();
        if queue.is_empty() {
            return;
        }
        let source_previews = queue
            .iter()
            .filter_map(|&index| {
                let source = image::open(&self.entries[index].path).ok()?;
                let mut pixels = source
                    .resize_to_fill(440, 330, image::imageops::FilterType::Triangle)
                    .to_rgba8();
                for pixel in pixels.pixels_mut() {
                    pixel.0.swap(0, 2);
                }
                Some((
                    index,
                    std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(pixels)])),
                ))
            })
            .collect();
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Добавить значение…"));
        let subscription = cx.subscribe_in(&query, window, |this, input, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                let active = this.distribution.as_ref().and_then(|w| w.active);
                let value = active
                    .and_then(|i| this.wizard_matches(false, cx).get(i).cloned())
                    .unwrap_or_else(|| input.read(cx).value().to_string());
                this.wizard_add(value, window, cx);
            } else if let Some(w) = &mut this.distribution {
                match event {
                    InputEvent::Focus => w.focused = true,
                    InputEvent::Blur => w.focused = false,
                    InputEvent::Change => w.active = None,
                    _ => {}
                }
            }
            cx.notify();
        });
        let artist = cx.new(|cx| InputState::new(window, cx).placeholder("Введите: автор…"));
        let artist_subscription = cx.subscribe_in(&artist, window, |this, _, event, _, cx| {
            if let Some(w) = &mut this.distribution {
                match event {
                    InputEvent::Focus => w.artist_focused = true,
                    InputEvent::Blur => w.artist_focused = false,
                    InputEvent::Change => w.active = None,
                    _ => {}
                }
            }
            cx.notify();
        });
        self.distribution = Some(Wizard {
            queue,
            position: 0,
            step: 0,
            done: false,
            completed: 0,
            all: false,
            retain: false,
            batch_expanded: true,
            batch_scroll: presets::scrolling::ScrollState::default(),
            scope_focus: std::array::from_fn(|_| cx.focus_handle()),
            values: Default::default(),
            query,
            _query_subscription: subscription,
            artist,
            _artist_subscription: artist_subscription,
            focused: false,
            artist_focused: false,
            active: None,
            title: cx.new(|cx| InputState::new(window, cx).placeholder("Введите название видео")),
            scroll: gpui::ScrollHandle::new(),
            background: wizard_background(),
            backdrop: super::modal_backdrop::capture(window.scale_factor()),
            source_previews,
            presets: false,
            preset_state: presets::State::new(window, cx),
            save_preset_focus: cx.focus_handle().tab_stop(true),
            focus: cx.focus_handle(),
            close_focus: cx.focus_handle().tab_stop(true),
            next_focus: cx.focus_handle().tab_stop(true),
            copyright: 0,
            assignment: None,
            error: None,
        });
        self.distribution.as_ref().unwrap().focus.focus(window);
        tracing::info!(
            operation = "distribution",
            count = self.stage_selection.len(),
            "Demo wizard opened"
        );
        cx.notify();
    }

    fn wizard_step(&mut self, step: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(w) = self.distribution.as_mut() else {
            return;
        };
        w.step = step;
        w.error = None;
        w.active = None;
        w.focused = false;
        w.artist_focused = false;
        w.assignment = None;
        w.query.update(cx, |s, cx| s.set_value("", window, cx));
        w.query.update(cx, |s, cx| {
            s.set_placeholder(
                match step {
                    1 => "Добавить: произведения…",
                    2 => "Добавить: теги…",
                    3 if w.values[0].is_empty() => "Сначала выберите произведение",
                    3 => "Добавить: персонажи…",
                    _ => "Добавить значение…",
                },
                window,
                cx,
            )
        });
        w.scroll.set_offset(gpui::point(px(0.), px(0.)));
        w.focus.focus(window);
        cx.notify();
    }

    fn wizard_add(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(w) = self.distribution.as_mut() else {
            return;
        };
        let category = w.step.saturating_sub(1).min(2);
        let value = value.trim().to_string();
        if value.is_empty() || (category == 2 && w.values[0].is_empty()) {
            return;
        }
        let qualified = w.values[0].iter().any(|c| metadata::belongs_to(&value, c));
        if category == 2 && !qualified && w.values[0].len() > 1 {
            w.assignment = Some(value);
            cx.notify();
            return;
        }
        let value = if category == 2 && !w.values[0].iter().any(|c| metadata::belongs_to(&value, c))
        {
            format!(
                "{} ({})",
                value,
                w.values[0][w.copyright.min(w.values[0].len() - 1)]
            )
        } else {
            value
        };
        metadata::insert_value(&mut w.values[category], value);
        w.query.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    fn wizard_complete(&mut self, cx: &mut Context<Self>) {
        let Some(w) = self.distribution.as_mut() else {
            return;
        };
        let count = if w.all { w.queue.len() - w.position } else { 1 };
        w.error = None;
        let artist = w.artist.read(cx).value().trim().to_string();
        let title = w.title.read(cx).value().trim().to_string();
        let needs_title = w.queue[w.position..w.position + count]
            .iter()
            .any(|&index| self.entries[index].video);
        if artist.trim().is_empty() || (needs_title && title.trim().is_empty()) {
            w.error = Some("Укажите художника и название для видео.".into());
            w.scroll.scroll_to_bottom();
            cx.notify();
            return;
        }
        for &index in &w.queue[w.position..w.position + count] {
            let entry = &mut self.entries[index];
            entry.tags = w.values[1].clone();
            entry.details.copyrights = w.values[0].clone();
            entry.details.character_copyrights = w.values[2]
                .iter()
                .filter_map(|value| {
                    let (name, copyright) = value.rsplit_once(" (")?;
                    Some((
                        name.to_string(),
                        copyright.trim_end_matches(')').to_string(),
                    ))
                })
                .collect();
            entry.characters = entry
                .details
                .character_copyrights
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            entry.details.artist = Some(artist.clone());
            if entry.video && !title.trim().is_empty() {
                entry.title = title.clone();
            }
            self.stage_selection.remove(&index);
        }
        w.completed = count;
        w.done = true;
        tracing::info!(
            operation = "distribution",
            count,
            "Demo metadata applied; completion displayed"
        );
        cx.notify();
    }

    fn wizard_next_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(w) = self.distribution.as_mut() else {
            return;
        };
        let completed: Vec<_> = w.queue[w.position..w.position + w.completed].to_vec();
        if !workflow::advance_position(&mut w.position, w.completed, w.queue.len()) {
            self.close_distribution(cx);
            return;
        }
        w.done = false;
        w.step = 0;
        w.all = false;
        w.error = None;
        if !w.retain {
            w.values = Default::default();
            w.artist.update(cx, |s, cx| s.set_value("", window, cx));
            w.title.update(cx, |s, cx| s.set_value("", window, cx));
        }
        // Keep stable queue indices until the wizard closes.
        tracing::debug!(
            operation = "distribution",
            count = completed.len(),
            "Advancing demo queue"
        );
        cx.notify();
    }

    pub(super) fn close_distribution(&mut self, cx: &mut Context<Self>) {
        if let Some(w) = self.distribution.take() {
            let end = w.position + if w.done { w.completed } else { 0 };
            let mut completed = w.queue[..end].to_vec();
            completed.sort_unstable();
            for index in completed.into_iter().rev() {
                self.inactive_workspace
                    .entries
                    .push(self.entries.remove(index));
            }
            self.stage_selection.clear();
            self.selected = None;
            self.details_open = false;
            self.previews.clear();
            self.tag_counts = crate::catalog::tag_counts(&self.entries);
            self.refilter(cx);
            tracing::info!(
                operation = "distribution",
                completed = end,
                "Demo wizard closed"
            );
        }
    }

    pub(super) fn distribution_dialog(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let w = self.distribution.as_ref().expect("open wizard");
        let entry = &self.entries[w.queue[w.position]];
        let remaining = w.queue.len() - w.position;
        let compact = f32::from(window.viewport_size().height) <= 650.;
        let height = (f32::from(window.viewport_size().height) - 24.).min(820.);
        let width = (f32::from(window.viewport_size().width) - 48.).min(680.);
        let mut body = div()
            .id("wizard-body")
            .w_full()
            .px(px(24.))
            .pt(px(if compact {
                16.
            } else if w.step == 0 {
                15.
            } else {
                25.
            }))
            .pb(px(if w.step == 0 { 14. } else { 24. }))
            .flex()
            .flex_col();
        if w.done {
            let mut summary = div()
                .w_full()
                .mt(px(14.))
                .px(px(14.))
                .py_2()
                .rounded_lg()
                .border_1()
                .border_color(rgb(LINE))
                .bg(rgb(SURFACE))
                .child(row("Действие", "Распределение завершено".into()))
                .child(row("Файлы", w.completed.to_string()))
                .child(row(
                    "Очередь",
                    format!("{} осталось", remaining - w.completed),
                ));
            if remaining > w.completed {
                summary = summary
                    .child(row(
                        "Следующий файл",
                        self.entries[w.queue[w.position + w.completed]]
                            .details
                            .filename
                            .clone(),
                    ))
                    .child(row(
                        "Введённые сведения",
                        if w.retain {
                            "Сохранить для следующего файла"
                        } else {
                            "Очистить для следующего файла"
                        }
                        .into(),
                    ));
            }
            body = body.child(
                div()
                    .w_full()
                    .pt_2()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .relative()
                            .w(px(160.))
                            .h(px(120.))
                            .mb_3()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(super::ACCENT))
                            .child(shell::icon("folder").size(px(92.)))
                            .child(
                                div()
                                    .absolute()
                                    .top(px(48.))
                                    .left(px(63.))
                                    .size(px(38.))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(rgb(0xd0b4fa))
                                    .bg(rgb(0x7b43d9))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(0xffffff))
                                    .child(shell::icon("check").size(px(28.))),
                            ),
                    )
                    .child(
                        heading("Готово!")
                            .text_size(px(22.))
                            .line_height(px(27.))
                            .mb_2(),
                    )
                    .child(note(&if w.completed == 1 {
                        "Файл распределён на время демосеанса.".into()
                    } else {
                        format!("Файлы ({}) распределены на время демосеанса.", w.completed)
                    }))
                    .child(summary),
            );
        } else {
            let descriptions = [
                "Проверьте исходный файл перед добавлением сведений в библиотеку.",
                "Выберите произведения, к которым относятся персонажи.",
                "Добавьте теги, чтобы проще находить этот файл в библиотеке.",
                "Добавьте персонажей и укажите для каждого выбранное произведение.",
                "Укажите автора и название, используемое в именах видеофайлов.",
                "Проверьте сведения перед подтверждением.",
            ];
            body = body
                .child(
                    heading(if w.step == 0 {
                        "Информация о файле"
                    } else {
                        match w.step {
                            4 => "Дополнительные сведения",
                            5 => "Подтвердить распределение",
                            _ => STEPS[w.step],
                        }
                    })
                    .when(compact && w.step == 0, |d| d.mb(px(10.))),
                )
                .when(!(compact && w.step == 0), |d| {
                    d.child(
                        div()
                            .mt(px(if w.step == 0 { 6. } else { 8. }))
                            .mb(px(if w.step == 0 { 12. } else { 22. }))
                            .child(note(descriptions[w.step]).line_height(px(19.2))),
                    )
                });
            match w.step {
                0 => {
                    let preview = div()
                        .w(px(220.))
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap_3()
                        .child(
                            div()
                                .relative()
                                .w(px(220.))
                                .h(px(165.))
                                .flex_shrink_0()
                                .rounded_lg()
                                .overflow_hidden()
                                .child(
                                    img(w
                                        .source_previews
                                        .get(&w.queue[w.position])
                                        .cloned()
                                        .map(gpui::ImageSource::Render)
                                        .unwrap_or_else(|| entry.path.clone().into()))
                                    .size_full()
                                    .rounded_lg()
                                    .object_fit(gpui::ObjectFit::Fill),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .top(px(8.))
                                        .left(px(8.))
                                        .px(px(6.))
                                        .py(px(4.))
                                        .rounded(px(4.))
                                        .bg(gpui::rgba(0x0a0a12c2))
                                        .text_size(px(10.))
                                        .line_height(px(12.))
                                        .child(entry.extension.clone()),
                                )
                                .when_some(entry.duration.clone(), |d, duration| {
                                    d.child(
                                        div()
                                            .absolute()
                                            .bottom(px(8.))
                                            .right(px(8.))
                                            .px(px(6.))
                                            .py(px(4.))
                                            .rounded(px(4.))
                                            .bg(gpui::rgba(0x0a0a12c2))
                                            .text_size(px(10.))
                                            .line_height(px(12.))
                                            .child(duration),
                                    )
                                }),
                        )
                        .child(
                            button("wizard-open", "")
                                .w_auto()
                                .child(shell::icon("arrow").size(px(14.)))
                                .child("Открыть файл")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let index = this.distribution.as_ref().unwrap().queue
                                        [this.distribution.as_ref().unwrap().position];
                                    this.selected = Some(index);
                                    this.open_viewer(window, cx);
                                })),
                        );
                    let data = div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(if compact { 6. } else { 8. }))
                        .child(technical_inline(
                            compact,
                            "Имя файла",
                            entry.details.filename.clone(),
                        ))
                        .child(technical_inline(
                            compact,
                            "Путь во входящих",
                            entry.details.relative_path.clone(),
                        ))
                        .child(
                            div()
                                .flex()
                                .gap(px(if compact { 6. } else { 8. }))
                                .child(technical(
                                    compact,
                                    "Тип файла",
                                    format!(
                                        "{} ({})",
                                        if entry.video {
                                            "Видео"
                                        } else {
                                            "Изображение"
                                        },
                                        entry.extension
                                    ),
                                ))
                                .child(technical(
                                    compact,
                                    "Изменён",
                                    entry.date_label.clone().unwrap_or("—".into()),
                                )),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(if compact { 6. } else { 8. }))
                                .child(technical(
                                    compact,
                                    "Размер",
                                    entry
                                        .details
                                        .size
                                        .map(|v| format!("{:.2} МБ", v as f64 / 1048576.))
                                        .unwrap_or("—".into()),
                                ))
                                .child(technical(
                                    compact,
                                    "Разрешение",
                                    entry
                                        .details
                                        .resolution
                                        .map(|(x, y)| format!("{x} × {y}"))
                                        .unwrap_or("—".into()),
                                )),
                        )
                        .when(entry.video, |d| {
                            d.child(
                                div()
                                    .flex()
                                    .gap(px(if compact { 6. } else { 8. }))
                                    .child(technical(
                                        compact,
                                        "Длительность",
                                        entry
                                            .duration
                                            .as_ref()
                                            .map(|v| {
                                                if v.len() == 5 {
                                                    format!("00:{v}")
                                                } else {
                                                    v.clone()
                                                }
                                            })
                                            .unwrap_or("—".into()),
                                    ))
                                    .child(technical(compact, "FPS", "29.97".into())),
                            )
                        });
                    body = body.child(div().flex().gap(px(22.)).child(preview).child(data));
                }
                1..=3 => {
                    let category = w.step - 1;
                    let mut selected = div()
                        .id("wizard-selected")
                        .flex_1()
                        .min_h(px(100.))
                        .overflow_y_scroll()
                        .p(px(14.))
                        .bg(rgb(SURFACE))
                        .border_1()
                        .border_color(rgb(LINE))
                        .rounded_lg()
                        .flex()
                        .flex_wrap()
                        .content_start()
                        .gap(px(6.));
                    if w.values[category].is_empty() {
                        selected = selected.child(note(&format!(
                            "Пока не выбрано: {}.",
                            STEPS[w.step].to_lowercase()
                        )));
                    }
                    for (index, value) in w.values[category].iter().enumerate() {
                        selected = selected.child(
                            button(("wizard-chip", index), value)
                                .child(shell::icon("close").size(px(12.)))
                                .bg(rgb(0x2c263e))
                                .text_color(rgb(0xd0b4fa))
                                .border_0()
                                .rounded(px(12.))
                                .px(px(9.))
                                .text_size(px(10.))
                                .h(px(30.))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let w = this.distribution.as_mut().unwrap();
                                    w.values[category].remove(index);
                                    if category == 0 {
                                        metadata::prune_characters(&mut w.values);
                                    }
                                    cx.notify();
                                })),
                        );
                    }
                    body = body
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .child(note(STEPS[w.step]))
                                .when(!w.values[category].is_empty(), |d| {
                                    d.child(
                                        note(&format!("{} выбрано", w.values[category].len()))
                                            .text_size(px(10.)),
                                    )
                                }),
                        )
                        .child(div().h_3())
                        .child(selected)
                        .child(div().h_3())
                        .child(self.wizard_input(false, cx));
                }
                4 => {
                    body = body
                        .child(note("Автор"))
                        .child(div().h_2())
                        .child(self.wizard_input(true, cx))
                        .child(div().h_6());
                    if entry.video || w.queue[w.position..].iter().any(|&i| self.entries[i].video) {
                        body = body.child(note("Название видео")).child(div().h_2()).child(
                            input_surface("wizard-title-input", &w.title, false, cx)
                                .child(input(&w.title).flex_1()),
                        );
                    }
                }
                _ => {
                    if remaining > 1 {
                        body = body.child(self.wizard_scope(window, cx));
                    }
                    body = body.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(15.))
                            .p_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(0x242034))
                            .child(
                                div()
                                    .w(px(115.))
                                    .h(px(70.))
                                    .flex_shrink_0()
                                    .rounded(px(5.))
                                    .overflow_hidden()
                                    .child(
                                        img(entry.path.clone())
                                            .size_full()
                                            .object_fit(gpui::ObjectFit::Cover)
                                            .rounded(px(5.)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        heading(&if w.all {
                                            format!("{remaining} files · Shared details")
                                        } else {
                                            entry.details.filename.clone()
                                        })
                                        .text_size(px(13.))
                                        .line_height(px(16.25))
                                        .mb(px(6.)),
                                    )
                                    .child(note(&if w.all {
                                        "Одинаковые метаданные и название видео будут применены ко всем перечисленным файлам.".into()
                                    } else {
                                        entry
                                            .details
                                            .resolution
                                            .map(|(x, y)| {
                                                format!(
                                                    "{:.2} МБ · {x} × {y}",
                                                    entry.details.size.unwrap_or(0) as f64
                                                        / 1048576.
                                                )
                                            })
                                            .unwrap_or("—".into())
                                    })),
                            ),
                    );
                    let mut summary = div()
                        .mt(px(14.))
                        .px(px(14.))
                        .py_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(SURFACE));
                    for (label, value) in [
                        ("Произведения", w.values[0].join(", ")),
                        ("Автор", w.artist.read(cx).value().trim().to_string()),
                        ("Персонажи", w.values[2].join(", ")),
                        ("Теги", w.values[1].join(", ")),
                        (
                            "Название видео",
                            w.title.read(cx).value().trim().to_string(),
                        ),
                    ] {
                        if label == "Название видео" && !entry.video {
                            continue;
                        }

                        summary = summary.child(row(
                            label,
                            if value.is_empty() {
                                "—".into()
                            } else {
                                value
                            },
                        ));
                    }
                    body = body.child(summary);
                    body = body.child(
                        button("wizard-save-preset", "")
                            .with_focus(
                                &w.save_preset_focus
                                    .clone()
                                    .tab_stop(!w.artist.read(cx).value().trim().is_empty()),
                            )
                            .w(px(170.))
                            .mt(px(14.))
                            .child(shell::icon("plus").size(px(15.)))
                            .child("Сохранить как пресет…")
                            .when(w.artist.read(cx).value().trim().is_empty(), |d| {
                                d.opacity(0.35)
                                    .cursor_default()
                                    .hover_colors(None, None, None)
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                let w = this.distribution.as_mut().unwrap();
                                if w.artist.read(cx).value().trim().is_empty() {
                                    return;
                                }
                                this.open_preset_manager(true, window, cx);
                            })),
                    );
                    if w.all {
                        body = body.child(self.wizard_batch_files(cx));
                    }
                }
            }
        }
        if let Some(error) = &w.error {
            body = body.child(div().text_color(rgb(0xff8899)).child(error.clone()));
        }
        body = body.child(
            div()
                .mt(px(if w.step == 0 && compact {
                    8.
                } else if w.step == 0 {
                    12.
                } else {
                    18.
                }))
                .mb(px(if w.step == 0 { 0. } else { 15. }))
                .flex_shrink_0()
                .child(
                    note("Демосеанс · Изменения не сохраняются на диск.")
                        .text_size(px(11.))
                        .line_height(px(18.7)),
                ),
        );
        let mut steps = div()
            .flex()
            .px_4()
            .pt(px(if compact { 12. } else { 20. }))
            .pb(px(if compact { 12. } else { 19. }))
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(LINE));
        for (i, label) in STEPS.iter().enumerate() {
            steps = steps.child(
                motion::button(("wizard-step", i))
                    .keyboard_focus(!w.done)
                    .focus_ring(rgb(SURFACE))
                    .cursor_pointer()
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_1()
                    .flex_col()
                    .gap(px(9.))
                    .border_0()
                    .text_color(rgb(if i == w.step { TEXT } else { MUTED }))
                    .when(i > 0, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(14.))
                                .right(gpui::relative(0.5))
                                .mr(px(17.))
                                .w(px((width - 32.) / 6. - 34.))
                                .h(px(1.))
                                .bg(rgb(LINE)),
                        )
                    })
                    .child(
                        div()
                            .size(px(29.))
                            .border_1()
                            .border_color(rgb(if i <= w.step || w.done {
                                0x6d568f
                            } else {
                                LINE
                            }))
                            .rounded_full()
                            .bg(rgb(0x10101a))
                            .when(i == w.step && !w.done, |d| {
                                d.bg(gpui::linear_gradient(
                                    140.,
                                    gpui::linear_color_stop(rgb(0x7b43d9), 0.),
                                    gpui::linear_color_stop(rgb(0x6839ce), 1.),
                                ))
                            })
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(i < w.step || w.done, |d| {
                                d.child(shell::icon("check").size(px(14.)))
                            })
                            .when(i >= w.step && !w.done, |d| d.child((i + 1).to_string())),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .line_height(px(13.75))
                            .when(i == w.step && !w.done, |d| {
                                d.font_weight(gpui::FontWeight::SEMIBOLD)
                            })
                            .child(*label),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !this.distribution.as_ref().unwrap().done {
                            this.wizard_step(i, window, cx);
                        }
                    })),
            );
        }
        let mut footer = div()
            .flex()
            .gap_2()
            .px(px(22.))
            .py(px(14.))
            .border_t_1()
            .border_color(rgb(LINE));
        if w.step > 0 && !w.done {
            footer = footer.child(
                button("wizard-back", "")
                    .px(px(18.))
                    .child(
                        gpui_component::Icon::default()
                            .path("icons/arrow-left.svg")
                            .size(px(15.)),
                    )
                    .child("Назад")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let step = this.distribution.as_ref().unwrap().step;
                        this.wizard_step(step - 1, window, cx);
                    })),
            );
        }
        let all_label = format!("Распределить все ({remaining})");
        footer = footer
            .child(div().flex_1())
            .when(!w.done || remaining > w.completed, |d| {
                d.child(
                    button(
                        "wizard-cancel",
                        if w.done {
                            "Закрыть"
                        } else {
                            "Отмена"
                        },
                    )
                    .px(px(18.))
                    .on_click(cx.listener(|this, _, _, cx| this.close_distribution(cx))),
                )
            })
            .child(
                button(
                    "wizard-next",
                    if w.done {
                        if remaining > w.completed {
                            "Следующий файл"
                        } else {
                            "Готово"
                        }
                    } else if w.step == 5 {
                        if w.all {
                            &all_label
                        } else {
                            "Распределить файл"
                        }
                    } else {
                        "Далее"
                    },
                )
                .with_focus(&w.next_focus)
                .px(px(18.))
                .when(!w.done || remaining > w.completed, |d| {
                    d.child(if w.step == 5 && !w.done {
                        shell::icon("check").size(px(16.))
                    } else {
                        shell::icon("arrow").size(px(16.))
                    })
                })
                .bg(motion::Surface::gradient(
                    120.,
                    rgb(0x7b43d9),
                    rgb(0x6839ce),
                ))
                .hover_colors(Some(rgb(0x8853e1).into()), None, Some(gpui::rgba(0).into()))
                .on_click(cx.listener(|this, _, window, cx| {
                    let w = this.distribution.as_ref().unwrap();
                    if w.done {
                        this.wizard_next_file(window, cx);
                    } else if w.step == 5 {
                        this.wizard_complete(cx);
                    } else {
                        this.wizard_step(w.step + 1, window, cx);
                    }
                })),
            );
        div()
            .absolute()
            .inset_0()
            .bg(gpui::rgba(0x05040cab))
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .when_some(w.backdrop.clone(), |d, backdrop| {
                d.child(img(backdrop).absolute().inset_0().size_full().object_fit(gpui::ObjectFit::Fill))
            })
            .child(
                div()
                    .w(px(width))
                    .h(px(height))
                    .track_focus(&w.focus)
                    .relative()
                    .flex()
                    .flex_col()
                    .bg(rgb(SHELL))
                    .child(
                        div()
                            .absolute()
                            .top_0().left_0()
                            .size_full()
                            .child(img(w.background.clone()).size_full().object_fit(gpui::ObjectFit::Fill)),
                    )
                    .border_1()
                    .border_color(rgb(LINE))
                    .rounded(px(14.))
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .px_6()
                            .py_4()
                            .h(px(74.))
                            .flex_shrink_0()
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .child(
                                div()
                                    .size(px(34.))
                                    .rounded(px(9.))
                                    .bg(rgb(0x29203d))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        gpui_component::Icon::default()
                                            .path("icons/folder-input.svg")
                                            .size(px(19.))
                                            .text_color(rgb(super::ACCENT)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .child(
                                        heading("Распределить в библиотеку")
                                            .text_size(px(14.))
                                            .line_height(px(17.)),
                                    )
                                    .child(div().mt(px(5.)).child(note(
                                        &(if w.queue.len() == 1 {
                                            entry.details.filename.clone()
                                        } else {
                                            format!(
                                                "Файл {} из {} · {}",
                                                w.position + 1,
                                                w.queue.len(),
                                                entry.details.filename
                                            )
                                        }),
                                    ))),
                            )
                            .when(!w.done, |header| header.child(self.preset_control(window, cx)))
                            .child(
                                motion::button("wizard-close").with_focus(&w.close_focus).focus_ring(rgb(SURFACE)).cursor_pointer().rounded(px(5.)).hover_colors(Some(gpui::rgba(0xa786ff1a).into()), Some(rgb(TEXT).into()), None)
                                    .size(px(28.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(MUTED))
                                    .child(shell::icon("close").size(px(18.)))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.close_distribution(cx)),
                                    ),
                            ),
                    )
                    .child(steps)
                    .child(if !w.done && (1..=3).contains(&w.step) {
                        body.flex_1().min_h_0()
                    } else {
                        div()
                            .id("wizard-body-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&w.scroll)
                            .child(body.flex_shrink_0())
                    })
                    .when(remaining > 1 && !w.done, |d| {
                        d.child(
                            motion::button("wizard-retain").keyboard_focus(true).focus_ring(rgb(SURFACE)).cursor_pointer()
                            .mx(px(22.)).py_3().border_t_1().border_color(rgb(LINE)).flex().items_center().justify_between().gap_4().flex_shrink_0()
                            .child(div().flex_1().text_size(px(13.)).line_height(px(16.)).child("Сохранять введённые сведения между файлами")
                                .child(div().mt_1().text_size(px(11.)).text_color(rgb(MUTED)).child("Произведения, автор, персонажи, теги и название видео")))
                            .child(super::filters::Switch { active: w.retain })
                            .on_click(cx.listener(|this, _, _, cx| {
                                let w = this.distribution.as_mut().unwrap();
                                w.retain = !w.retain;
                                cx.notify();
                            })),
                        )
                    })
                    .child(footer),
            )
            .when(w.preset_state.editor.is_some(), |d| {
                d.child(self.preset_manager(window, cx))
            })
    }
}

fn heading(text: &str) -> Div {
    div()
        .text_size(px(15.))
        .line_height(px(18.75))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT))
        .child(text.to_owned())
}
fn note(text: &str) -> Div {
    div()
        .text_size(px(12.))
        .line_height(px(15.6))
        .text_color(rgb(MUTED))
        .child(text.to_owned())
}
fn row(label: &str, value: String) -> Div {
    div()
        .w_full()
        .py(px(9.))
        .when(label != "Название видео", |d| d.border_b_1())
        .border_color(rgb(LINE))
        .flex()
        .gap(px(16.))
        .child(div().w(px(115.)).child(note(label)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.))
                .line_height(px(18.))
                .child(value),
        )
}

fn button(id: impl Into<gpui::ElementId>, title: &str) -> motion::MotionDiv {
    motion::button(id)
        .keyboard_focus(true)
        .focus_ring(rgb(SURFACE))
        .cursor_pointer()
        .hover_colors(Some(rgb(0x242034).into()), None, Some(rgb(0x6d568f).into()))
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .px(px(12.))
        .h(px(34.))
        .rounded(px(6.))
        .bg(rgb(0x1a1827))
        .border_1()
        .border_color(rgb(LINE))
        .text_size(px(11.))
        .line_height(px(13.75))
        .font_weight(gpui::FontWeight::MEDIUM)
        .when(!title.is_empty(), |d| d.child(title.to_owned()))
}

fn input(state: &Entity<InputState>) -> Input {
    Input::new(state)
        .appearance(false)
        .h(px(38.))
        .px(px(12.))
        .py(px(11.))
        .text_size(px(12.))
        .line_height(px(15.))
        .line_height(px(16.))
}

fn input_surface(
    id: &'static str,
    state: &Entity<InputState>,
    search_icon: bool,
    cx: &Context<Gallery>,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .track_focus(&state.focus_handle(cx))
        .relative()
        .w_full()
        .h(px(40.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(0x10101a))
        .focus(|style| style.border_color(rgb(super::ACCENT)))
        .when(search_icon, |field| {
            field.child(
                div()
                    .ml_3()
                    .text_color(rgb(MUTED))
                    .child(shell::icon("search").size(px(15.))),
            )
        })
}

fn technical(compact: bool, label: &str, value: String) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(if compact { 3. } else { 5. }))
        .child(
            div()
                .text_size(px(if compact { 10. } else { 11. }))
                .line_height(px(if compact { 13. } else { 15. }))
                .text_color(rgb(MUTED))
                .child(label.to_owned()),
        )
        .child(
            div()
                .px(px(if compact { 8. } else { 9. }))
                .py(px(if compact { 5. } else { 7. }))
                .bg(rgb(SURFACE))
                .border_1()
                .border_color(rgb(LINE))
                .rounded(px(5.))
                .text_size(px(if compact { 11. } else { 12. }))
                .line_height(px(if compact { 14. } else { 16. }))
                .child(value),
        )
}

fn technical_inline(compact: bool, label: &str, value: String) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(80.))
                .text_size(px(if compact { 10. } else { 11. }))
                .line_height(px(if compact { 13. } else { 15. }))
                .mb(px(5.))
                .text_color(rgb(MUTED))
                .child(label.to_owned()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .px(px(if compact { 8. } else { 9. }))
                .py(px(if compact { 5. } else { 7. }))
                .bg(rgb(SURFACE))
                .border_1()
                .border_color(rgb(LINE))
                .rounded(px(5.))
                .text_size(px(if compact { 11. } else { 12. }))
                .line_height(px(if compact { 14. } else { 16. }))
                .child(value),
        )
}

impl Gallery {
    fn wizard_matches(&self, artist: bool, cx: &Context<Self>) -> Vec<String> {
        let Some(w) = &self.distribution else {
            return vec![];
        };
        let query = if artist { &w.artist } else { &w.query }
            .read(cx)
            .value()
            .to_lowercase();
        if query.trim().is_empty() {
            return vec![];
        }
        let category = w.step.saturating_sub(1).min(2);
        let mut values: Vec<String> = self
            .entries
            .iter()
            .chain(self.inactive_workspace.entries.iter())
            .flat_map(|e| {
                if artist {
                    e.details.artist.iter().cloned().collect()
                } else {
                    match category {
                        0 => e.details.copyrights.clone(),
                        1 => e.tags.clone(),
                        _ => e
                            .details
                            .character_copyrights
                            .iter()
                            .filter(|(_, c)| {
                                w.values[0]
                                    .iter()
                                    .any(|value| value.to_lowercase() == c.to_lowercase())
                            })
                            .map(|(n, c)| format!("{n} ({c})"))
                            .collect(),
                    }
                }
            })
            .filter(|v: &String| {
                v.to_lowercase().contains(&query)
                    && (artist
                        || !w.values[category]
                            .iter()
                            .any(|selected| selected.to_lowercase() == v.to_lowercase()))
            })
            .collect();
        values.sort();
        values.dedup();
        values.truncate(10);
        values
    }

    fn wizard_input(&self, artist: bool, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let w = self.distribution.as_ref().unwrap();
        let state = if artist { &w.artist } else { &w.query };
        let options = self.wizard_matches(artist, cx);
        let focused = if artist { w.artist_focused } else { w.focused };
        let disabled = !artist && w.step == 3 && w.values[0].is_empty();
        let mut field = input_surface(
            if artist {
                "wizard-artist-input"
            } else {
                "wizard-metadata-input"
            },
            state,
            true,
            cx,
        )
        .child(input(state).pl(px(9.)).flex_1().disabled(disabled))
        .on_key_down(
            cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                let options = this.wizard_matches(artist, cx);
                let w = this.distribution.as_mut().unwrap();
                match event.keystroke.key.as_str() {
                    "down" | "up" if !options.is_empty() => {
                        let i = w.active.unwrap_or(if event.keystroke.key == "down" {
                            options.len() - 1
                        } else {
                            0
                        });
                        w.active = Some(if event.keystroke.key == "down" {
                            (i + 1) % options.len()
                        } else {
                            (i + options.len() - 1) % options.len()
                        });
                        cx.stop_propagation();
                    }
                    "enter" if artist => {
                        if let Some(value) = w.active.and_then(|i| options.get(i)) {
                            w.artist
                                .update(cx, |s, cx| s.set_value(value.clone(), window, cx));
                            w.artist_focused = false;
                        }
                    }
                    "escape" => {
                        w.focused = false;
                        w.artist_focused = false;
                        cx.stop_propagation();
                    }
                    _ => {}
                }
                cx.notify();
            }),
        );
        if !artist {
            field = field.child(
                button("wizard-add", "")
                    .child(shell::icon("plus").size(px(18.)))
                    .text_color(rgb(super::ACCENT))
                    .bg(rgb(0x242034))
                    .hover_colors(Some(rgb(0x37304d).into()), None, None)
                    .when(disabled || state.read(cx).value().trim().is_empty(), |d| {
                        d.opacity(0.35)
                            .cursor_default()
                            .hover_colors(None, None, None)
                    })
                    .w(px(40.))
                    .h_full()
                    .rounded_l(px(0.))
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.prevent_default()
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        let value = this
                            .distribution
                            .as_ref()
                            .unwrap()
                            .query
                            .read(cx)
                            .value()
                            .to_string();
                        this.wizard_add(value, window, cx);
                    })),
            );
        }
        if let Some(name) = &w.assignment {
            let mut popup = div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(46.))
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(rgb(LINE))
                .bg(rgb(0x1a1827))
                .occlude()
                .child(note(&format!("К какому произведению относится {name}?")));
            for (i, copyright) in w.values[0].iter().enumerate() {
                let value = format!("{name} ({copyright})");
                popup = popup.child(
                    button(("wizard-assignment", i), copyright)
                        .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                            window.prevent_default()
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.distribution.as_mut().unwrap().assignment = None;
                            this.wizard_add(value.clone(), window, cx);
                        })),
                );
            }
            popup = popup.child(button("wizard-assignment-cancel", "Отмена").on_click(
                cx.listener(|this, _, _, cx| {
                    this.distribution.as_mut().unwrap().assignment = None;
                    cx.notify();
                }),
            ));
            field = field.child(popup);
        } else if focused && !options.is_empty() && !disabled {
            let mut popup = div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(46.))
                .p(px(5.))
                .bg(rgb(0x1a1827))
                .border_1()
                .border_color(rgb(LINE))
                .rounded_lg()
                .shadow_lg()
                .flex()
                .flex_col()
                .occlude();
            for (i, value) in options.into_iter().enumerate() {
                popup = popup.child(
                    button(("wizard-suggestion", i), &value)
                        .h(px(36.25))
                        .text_size(px(13.))
                        .font_weight(gpui::FontWeight::NORMAL)
                        .justify_start()
                        .border_0()
                        .bg(rgb(if w.active == Some(i) {
                            0x2c263e
                        } else {
                            0x1a1827
                        }))
                        .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                            window.prevent_default()
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if artist {
                                let w = this.distribution.as_mut().unwrap();
                                w.artist
                                    .update(cx, |s, cx| s.set_value(value.clone(), window, cx));
                                w.artist_focused = false;
                            } else {
                                this.wizard_add(value.clone(), window, cx);
                            }
                            cx.notify();
                        })),
                );
            }
            field = field.child(popup);
        }
        field
    }
}

fn wizard_background() -> std::sync::Arc<gpui::RenderImage> {
    let pixels = image::RgbaImage::from_fn(256, 256, |x, y| {
        let dx = (x as f32 / 255. - 0.5) / (0.5 * std::f32::consts::SQRT_2);
        let dy = (y as f32 / 255.) / std::f32::consts::SQRT_2;
        let amount = (1. - dx.hypot(dy) / 0.55).clamp(0., 1.) * 0.13;
        let channel = |shift: u32| {
            let base = ((SHELL >> shift) & 255_u32) as f32;
            let accent = ((super::ACCENT >> shift) & 255_u32) as f32;
            (base + (accent - base) * amount).round() as u8
        };
        image::Rgba([channel(0), channel(8), channel(16), 255])
    });
    std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(pixels)]))
}
