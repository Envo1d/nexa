use std::sync::Arc;

use gpui::RenderImage;
use image::{Frame, Rgba, RgbaImage};

use super::{ACCENT, SHELL};

/// One normalized, opaque texture behind both titlebar and workspace. The
/// reference ellipse's percentage center/radii scale with the shell, so resizing
/// only stretches this cached smooth gradient rather than regenerating pixels.
pub(super) fn shell_image() -> Arc<RenderImage> {
    const EDGE: u32 = 256;
    let shell = crate::appearance::color(SHELL);
    let accent = crate::appearance::color(ACCENT);

    let image = RgbaImage::from_fn(EDGE, EDGE, |x, y| {
        let x = x as f32 / (EDGE - 1) as f32;
        let y = y as f32 / (EDGE - 1) as f32;
        let amount = glow_amount(x, y) * crate::appearance::shell_glow_strength();
        let channel = |shift: u32| {
            let base = ((shell >> shift) & 0xff) as f32;
            let accent = ((accent >> shift) & 0xff) as f32;

            (base + (accent - base) * amount).round() as u8
        };

        // RenderImage uses premultiplied BGRA; the opaque shell needs no alpha
        // conversion. Keep the texture below all native caption hit regions.
        Rgba([channel(0), channel(8), channel(16), 255])
    });

    Arc::new(RenderImage::new(vec![Frame::new(image)]))
}

fn glow_amount(x: f32, y: f32) -> f32 {
    // CSS: ellipse farthest-corner at 40% -35%, fading to transparent at 55%.
    let radius_x = 0.6 * std::f32::consts::SQRT_2;
    let radius_y = 1.35 * std::f32::consts::SQRT_2;
    let dx = (x - 0.4) / radius_x;
    let dy = (y + 0.35) / radius_y;
    let distance = dx.hypot(dy);

    (1.0 - distance / 0.55).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests;
