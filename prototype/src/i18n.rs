use std::{
    collections::HashMap,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

// Backup/native workers must resolve messages in the same locale as the UI.
static ENGLISH: AtomicBool = AtomicBool::new(false);

pub(crate) fn english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub(crate) fn resolve(preference: usize, system: &str) -> &'static str {
    match preference {
        1 => "en",
        2 => "ru",
        _ if system
            .split(['-', '_'])
            .next()
            .is_some_and(|tag| tag.eq_ignore_ascii_case("ru")) =>
        {
            "ru"
        }
        _ => "en",
    }
}

pub(crate) fn apply(preference: usize) -> &'static str {
    let mut name = [0_u16; 85];
    // SAFETY: Windows receives a live UTF-16 buffer and its exact length.
    let length = unsafe { windows::Win32::Globalization::GetUserDefaultLocaleName(&mut name) };
    let system = if length > 0 {
        String::from_utf16_lossy(&name[..length as usize - 1])
    } else {
        tracing::warn!(
            operation = "interface_preferences",
            "System locale unavailable; using English"
        );
        String::new()
    };
    let locale = resolve(preference, &system);
    ENGLISH.store(locale == "en", Ordering::Relaxed);
    gpui_component::set_locale(locale);

    locale
}

fn presentation() -> &'static HashMap<String, String> {
    static MESSAGES: OnceLock<HashMap<String, String>> = OnceLock::new();
    MESSAGES.get_or_init(|| {
        serde_json::from_str(include_str!("../locales/presentation.json"))
            .expect("bundled presentation translations are valid")
    })
}

/// Only call at presentation boundaries. Metadata, search values and user input
/// never pass through this lookup. Existing GPUI Russian copy is the source text.
pub(crate) fn text(source: &str) -> &str {
    if english() {
        presentation()
            .get(source)
            .map(String::as_str)
            .unwrap_or(source)
    } else {
        source
    }
}

#[cfg(test)]
mod tests;
