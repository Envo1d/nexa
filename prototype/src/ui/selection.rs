use std::{collections::BTreeSet, time::Instant};

use gpui::{Bounds, Context, MouseButton, MouseDownEvent, Pixels, Point, Window, point, px};

use super::{
    Gallery,
    layout::{COLLECTION_INSET, CollectionLayout, GAP},
};

pub(super) struct Marquee {
    start: Point<Pixels>,
    pub(super) pointer: Point<Pixels>,
    pub(super) viewport: Bounds<Pixels>,
    layout: CollectionLayout,
    base: BTreeSet<usize>,
    last_frame: Instant,
}

impl Marquee {
    fn new(
        pointer: Point<Pixels>,
        viewport: Bounds<Pixels>,
        layout: CollectionLayout,
        offset: Pixels,
        base: BTreeSet<usize>,
    ) -> Self {
        Self {
            start: pointer - viewport.origin - point(px(0.0), offset),
            pointer,
            viewport,
            layout,
            base,
            last_frame: Instant::now(),
        }
    }

    pub(super) fn rectangle(&self, offset: Pixels) -> Bounds<Pixels> {
        let local = self.pointer - self.viewport.origin;
        let end = point(
            local
                .x
                .clamp(px(0.0), (self.viewport.size.width - px(6.0)).max(px(0.0))),
            local.y.clamp(px(0.0), self.viewport.size.height) - offset,
        );

        Bounds::from_corners(
            point(self.start.x.min(end.x), self.start.y.min(end.y)),
            point(self.start.x.max(end.x), self.start.y.max(end.y)),
        )
    }

    fn selected(&self, visible: &[usize], offset: Pixels) -> BTreeSet<usize> {
        let rect = self.rectangle(offset);
        let mut selected = self.base.clone();
        if rect.size.width <= px(0.0) || rect.size.height <= px(0.0) {
            return selected;
        }

        let first_row = (f32::from(rect.top()) / self.layout.row_height)
            .floor()
            .max(0.0) as usize;
        let end_row = (f32::from(rect.bottom()) / self.layout.row_height)
            .ceil()
            .max(0.0) as usize;
        let first = first_row
            .saturating_mul(self.layout.columns)
            .min(visible.len());
        let end = end_row
            .saturating_mul(self.layout.columns)
            .min(visible.len());
        for (position, &index) in visible.iter().enumerate().take(end).skip(first) {
            let card = card_bounds(self.layout, position);
            let intersects = card.left() < rect.right()
                && card.right() > rect.left()
                && card.top() < rect.bottom()
                && card.bottom() > rect.top();
            if intersects {
                selected.insert(index);
            }
        }

        selected
    }

    fn scroll_speed(&self) -> f32 {
        let y = f32::from(self.pointer.y - self.viewport.origin.y);
        if y < 32.0 {
            -720.0
        } else if y > f32::from(self.viewport.size.height) - 32.0 {
            720.0
        } else {
            0.0
        }
    }
}

fn card_bounds(layout: CollectionLayout, position: usize) -> Bounds<Pixels> {
    Bounds::new(
        point(
            px(COLLECTION_INSET
                + 1.95
                + (position % layout.columns) as f32 * (layout.card_width + GAP)),
            px((position / layout.columns) as f32 * layout.row_height),
        ),
        gpui::size(px(layout.card_width), px(layout.row_height - GAP)),
    )
}

impl Gallery {
    pub(super) fn start_marquee(
        &mut self,
        event: &MouseDownEvent,
        layout: CollectionLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let viewport = self.collection_bounds;
        if !self.stage || self.viewer || event.button != MouseButton::Left {
            return;
        }

        let offset = self.scroll.0.borrow().base_handle.offset().y;
        if !background_at(event.position, viewport, layout, offset, self.visible.len()) {
            return;
        }

        let additive = event.modifiers.control || event.modifiers.platform;
        let base = if additive {
            self.stage_selection.clone()
        } else {
            BTreeSet::new()
        };
        self.marquee = Some(Marquee::new(event.position, viewport, layout, offset, base));
        self.focus.focus(window);
        window.prevent_default();
        self.update_marquee(cx);
        self.schedule_marquee_frame(window, cx);
        tracing::debug!(
            operation = "incoming_selection",
            additive,
            "Rectangle selection started"
        );
    }

    pub(super) fn update_marquee(&mut self, cx: &mut Context<Self>) {
        let Some(marquee) = &self.marquee else {
            return;
        };
        let offset = self.scroll.0.borrow().base_handle.offset().y;
        self.stage_selection = marquee.selected(&self.visible, offset);
        self.selected = self.stage_selection.first().copied();
        cx.notify();
    }

    pub(super) fn finish_marquee(&mut self, cx: &mut Context<Self>) {
        if self.marquee.take().is_some() {
            tracing::debug!(
                operation = "incoming_selection",
                selected = self.stage_selection.len(),
                "Rectangle selection finished"
            );
            cx.notify();
        }
    }

    fn schedule_marquee_frame(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.marquee_frame_pending {
            return;
        }

        self.marquee_frame_pending = true;
        let gallery = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = gallery.update(cx, |this, cx| {
                this.marquee_frame_pending = false;
                if !this.stage || this.viewer || !window.is_window_active() {
                    this.finish_marquee(cx);
                    return;
                }
                let Some(marquee) = this.marquee.as_mut() else {
                    return;
                };
                let now = Instant::now();
                let delta = marquee.scroll_speed()
                    * now
                        .duration_since(marquee.last_frame)
                        .as_secs_f32()
                        .min(0.05);
                marquee.last_frame = now;
                let scroll = this.scroll.0.borrow().base_handle.clone();
                let current = scroll.offset();
                let max = scroll.max_offset().height;
                scroll.set_offset(point(
                    current.x,
                    (current.y - px(delta)).clamp(-max, px(0.0)),
                ));
                this.update_marquee(cx);
                this.schedule_marquee_frame(window, cx);
            });
        });
        cx.notify();
    }
}

fn background_at(
    pointer: Point<Pixels>,
    viewport: Bounds<Pixels>,
    layout: CollectionLayout,
    offset: Pixels,
    count: usize,
) -> bool {
    if !viewport.contains(&pointer) || pointer.x >= viewport.right() - px(6.0) {
        return false;
    }

    let local = pointer - viewport.origin - point(px(0.0), offset);
    let row = (f32::from(local.y) / layout.row_height).floor().max(0.0) as usize;
    let over_card = (0..layout.columns).any(|column| {
        let position = row * layout.columns + column;
        position < count && card_bounds(layout, position).contains(&local)
    });

    !over_card
}

#[cfg(test)]
mod tests;
