//! Values reported by libmpv, never inferred from requested options.
pub(super) const PROPERTIES: [&str; 9] = [
    "hwdec-current",
    "video-codec",
    "current-vo",
    "container-fps",
    "estimated-vf-fps",
    "display-fps",
    "frame-drop-count",
    "decoder-frame-drop-count",
    "avsync",
];

#[derive(Default)]
pub(super) struct Diagnostics {
    values: [Option<String>; 9],
}

impl Diagnostics {
    pub(super) fn update(&mut self, id: u64, value: Option<String>) {
        if let Some(index) = id.checked_sub(1).and_then(|id| usize::try_from(id).ok())
            && let Some(slot) = self.values.get_mut(index)
        {
            *slot = value.filter(|value| !value.is_empty());
        }
    }

    pub(super) fn clear(&mut self) {
        self.values.fill(None);
    }

    pub(super) fn summary(&self) -> String {
        PROPERTIES
            .iter()
            .zip(&self.values)
            .map(|(name, value)| format!("{name}={}", value.as_deref().unwrap_or("unknown")))
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[cfg(test)]
#[path = "diagnostics/tests.rs"]
mod tests;
