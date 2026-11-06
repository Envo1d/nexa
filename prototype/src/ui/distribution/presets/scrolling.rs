use std::{cell::Cell, rc::Rc};

use gpui::{IntoElement, Pixels, ScrollHandle, Styled, canvas, fill, point, px, rgb, size};

/// Match the reference's five-pixel scrollbar without a component-library rail.
#[derive(Default)]
pub(in crate::ui::distribution) struct ScrollState {
    pub(in crate::ui::distribution) handle: ScrollHandle,
    drag: Rc<Cell<Option<Pixels>>>,
    overflow: Rc<Cell<bool>>,
}

impl ScrollState {
    pub(in crate::ui::distribution) fn gutter(&self) -> Pixels {
        px(if self.overflow.get() { 5. } else { 0. })
    }
}

pub(in crate::ui::distribution) fn scrollbar(
    state: &ScrollState,
    view: gpui::EntityId,
) -> impl IntoElement {
    let scroll = state.handle.clone();
    let drag = state.drag.clone();
    let overflow = state.overflow.clone();

    canvas(
        |bounds, _, _| bounds,
        move |_, _, window, cx| {
            let bounds = scroll.bounds();
            let maximum = scroll.max_offset().height;
            let overflowing = maximum > px(0.);
            if overflow.replace(overflowing) != overflowing {
                cx.notify(view);
            }
            if maximum <= px(0.) || bounds.size.height <= px(0.) {
                return;
            }

            let mut rail = bounds;
            rail.origin.x = bounds.right() - px(5.);
            rail.size.width = px(5.);
            let thumb_height = (bounds.size.height
                * (bounds.size.height / (bounds.size.height + maximum)))
                .max(px(18.));
            let travel = (bounds.size.height - thumb_height).max(px(1.));
            let top = -scroll.offset().y / maximum * travel;
            let thumb = gpui::Bounds::new(
                point(rail.origin.x, rail.origin.y + top),
                size(px(5.), thumb_height),
            );
            let mut paint = fill(thumb, rgb(0x4d4a5c));
            paint.corner_radii = px(3.).into();
            window.paint_quad(paint);

            let handle = scroll.clone();
            let dragging = drag.clone();
            window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, _, cx| {
                if phase.bubble()
                    && event.button == gpui::MouseButton::Left
                    && rail.contains(&event.position)
                {
                    cx.stop_propagation();
                    let grip = if thumb.contains(&event.position) {
                        event.position.y - thumb.top()
                    } else {
                        thumb_height / 2.
                    };
                    dragging.set(Some(grip));
                    let fraction = ((event.position.y - rail.top() - grip) / travel).clamp(0., 1.);
                    handle.set_offset(point(handle.offset().x, -maximum * fraction));
                    cx.notify(view);
                }
            });

            let handle = scroll.clone();
            let dragging = drag.clone();
            window.on_mouse_event(move |event: &gpui::MouseMoveEvent, _, _, cx| {
                if let Some(grip) = dragging.get().filter(|_| event.dragging()) {
                    cx.stop_propagation();
                    let fraction = ((event.position.y - rail.top() - grip) / travel).clamp(0., 1.);
                    handle.set_offset(point(handle.offset().x, -maximum * fraction));
                    cx.notify(view);
                }
            });

            let dragging = drag.clone();
            window.on_mouse_event(move |_: &gpui::MouseUpEvent, _, _, _| {
                dragging.set(None);
            });
        },
    )
    .absolute()
    .inset_0()
}
