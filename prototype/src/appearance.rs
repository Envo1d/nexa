use std::{collections::HashMap, sync::OnceLock};

use gpui::{App, Rgba};
use gpui_component::{Theme, ThemeMode};

thread_local! {
    static SELECTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn palettes() -> &'static Vec<HashMap<String, serde_json::Value>> {
    static PALETTES: OnceLock<Vec<HashMap<String, serde_json::Value>>> = OnceLock::new();
    PALETTES.get_or_init(|| {
        serde_json::from_str(include_str!("../locales/palettes.json"))
            .expect("bundled CSS palettes are valid")
    })
}

fn token(index: usize, name: &str) -> u32 {
    palettes()[index.min(6)]
        .get(name)
        .or_else(|| palettes()[0].get(name))
        .and_then(serde_json::Value::as_u64)
        .expect("palette token exists") as u32
}

fn mix(left: u32, right: u32, amount: f32) -> u32 {
    [0, 8, 16].into_iter().fold(0, |result, shift| {
        let a = ((left >> shift) & 255) as f32;
        let b = ((right >> shift) & 255) as f32;
        result | (((a + (b - a) * amount).round() as u32) << shift)
    })
}

/// Adapt the source-derived literal to the corresponding CSS semantic role.
/// Literal aliases retain their exact original appearance in Midnight Violet.
pub(crate) fn color_for(index: usize, source: u32) -> u32 {
    if index == 0 || source == 0xffffff || source == 0 {
        return source;
    }
    let role = match source {
        0x080910 => "background",
        0x0b0b13 => "shell",
        0x141420 => "surface",
        0x10101a => "surface-alt",
        0x1a1827 => "surface-raised",
        0x1a1927 => "surface-input",
        0x242034 => "surface-hover",
        0x30263e => "surface-pressed",
        0x252434 => "line",
        0x514361 => "line-strong",
        0xeeedf6 => "text",
        0xa39bb5 => "muted",
        0x928aa3 => "subtle",
        0xa786ff => "accent",
        0x7b43d9 => "accent-strong",
        0x6839ce => "accent-end",
        0x8853e1 => "accent-hover",
        0xd0b4fa => "text-accent",
        0xf292a7 => "danger",
        0x382128 => "danger-bg",
        0x824254 => "danger-line",
        0xa53d5a => "danger-button",
        0x85324b => "danger-button-end",
        0xb74766 => "danger-button-hover",
        0xefbc79 => "warning",
        0x33291c => "warning-bg",
        0x78603c => "warning-line",
        0x8bbbff => "info",
        0x152b46 => "info-bg",
        0x374f72 => "info-line",
        0x71333d => "copyright-line",
        0xf0c2cd => "copyright-text",
        0x262544 => "artist-bg",
        0x403e80 => "artist-line",
        0xd6d1ff => "artist-text",
        0x203b32 => "character-bg",
        0x285b4a => "character-line",
        0xb0dfc7 => "character-text",
        // Reference color-mix values and retained GPUI aliases.
        0x2c2644 | 0x302943 | 0x2c263f | 0x2c263e | 0x2a203d | 0x29203d | 0x251b3b | 0x221d32 => {
            return mix(token(index, "surface"), token(index, "accent"), 0.16);
        }
        0x372f56 | 0x37304d | 0x30223f | 0x393159 => {
            return mix(token(index, "surface"), token(index, "accent"), 0.24);
        }
        0x6d5aa4 | 0x6d568f | 0x49345f | 0x60477e | 0x6d5a90 => {
            return mix(token(index, "line"), token(index, "accent"), 0.55);
        }
        0x4d4a5c => return mix(token(index, "surface"), token(index, "muted"), 0.4),
        0x11111c | 0x15131f => "surface-alt",
        0x20202d | 0x292332 => "surface-raised",
        0xa78bfa => "accent",
        0x7541cf => "accent-strong",
        0x925bea => "accent-hover",
        0xff8899 | 0xf2a0b1 | 0xf09aa9 => "danger",
        0x8e354b => "danger-button",
        0x757575 => "subtle",
        0x393939 => "surface-input",
        0xe7bf7e => "warning",
        0x80b8ef => "info",
        0xe49acb => "copyright-text",
        _ => return source,
    };

    token(index, role)
}

pub(crate) fn color(source: u32) -> u32 {
    color_for(SELECTED.get(), source)
}

pub(crate) fn rgb(source: u32) -> Rgba {
    gpui::rgb(color(source))
}

pub(crate) fn rgba(source: u32) -> Rgba {
    if SELECTED.get() == 6 && source >> 8 == 0 {
        // Paper overrides all three CSS shadow tokens with a purple tint and
        // 32% of the dark palette's opacity. Transparent borders stay clear.
        return gpui::rgba(0x221b5300 | ((source & 255) as f32 * 0.32).round() as u32);
    }
    gpui::rgba((color(source >> 8) << 8) | (source & 255))
}

pub(crate) fn shell_glow_strength() -> f32 {
    if SELECTED.get() == 6 { 0.04 } else { 0.13 }
}

pub(crate) fn media_accent() -> Rgba {
    if SELECTED.get() == 6 {
        gpui::rgb(0xc099ff)
    } else {
        rgb(0xa786ff)
    }
}

pub(crate) fn apply(index: usize, cx: &mut App) {
    let index = index.min(6);
    SELECTED.set(index);
    Theme::change(
        if index == 6 {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        },
        None,
        cx,
    );
    let theme = Theme::global_mut(cx);
    theme.font_family = "Segoe UI".into();
    theme.background = rgb(0x1a1827).into();
    theme.foreground = rgb(0xeeedf6).into();
    theme.border = rgb(0x252434).into();
    theme.input = rgb(0x252434).into();
    theme.ring = rgb(0xa786ff).into();
    theme.muted_foreground = rgb(0xa39bb5).into();
    theme.popover = rgb(0x1a1827).into();
    theme.primary = rgb(0x7b43d9).into();
    theme.scrollbar_thumb = rgb(0x4d4a5c).into();
    theme.scrollbar_thumb_hover = rgb(0x4d4a5c).into();
    cx.refresh_windows();
}

#[cfg(test)]
mod tests;
