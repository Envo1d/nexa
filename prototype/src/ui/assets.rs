use std::borrow::Cow;

use gpui::{AssetSource, SharedString};

// Supplemental variants from the original Tauri Lucide version, omitted by
// gpui-component 0.5.1's default set. No locally redrawn icon geometry.
const SUPPLEMENTAL: &[(&str, &[u8])] = &[
    (
        "icons/player-fullscreen.svg",
        include_bytes!("../../assets/icons/player-fullscreen.svg"),
    ),
    (
        "icons/player-fullscreen-exit.svg",
        include_bytes!("../../assets/icons/player-fullscreen-exit.svg"),
    ),
    (
        "icons/viewer-heart-filled.svg",
        include_bytes!("../../assets/icons/viewer-heart-filled.svg"),
    ),
    (
        "icons/keyboard.svg",
        include_bytes!("../../assets/icons/keyboard.svg"),
    ),
    (
        "icons/external-link.svg",
        include_bytes!("../../assets/icons/external-link.svg"),
    ),
    (
        "icons/play.svg",
        include_bytes!("../../assets/icons/play.svg"),
    ),
    (
        "icons/pause.svg",
        include_bytes!("../../assets/icons/pause.svg"),
    ),
    (
        "icons/repeat.svg",
        include_bytes!("../../assets/icons/repeat.svg"),
    ),
    (
        "icons/volume-2.svg",
        include_bytes!("../../assets/icons/volume-2.svg"),
    ),
    (
        "icons/volume-x.svg",
        include_bytes!("../../assets/icons/volume-x.svg"),
    ),
    (
        "icons/maximize.svg",
        include_bytes!("../../assets/icons/maximize.svg"),
    ),
    (
        "icons/minimize.svg",
        include_bytes!("../../assets/icons/minimize.svg"),
    ),
    (
        "icons/arrow-left.svg",
        include_bytes!("../../assets/icons/arrow-left.svg"),
    ),
    (
        "icons/arrow-right.svg",
        include_bytes!("../../assets/icons/arrow-right.svg"),
    ),
    (
        "icons/plus.svg",
        include_bytes!("../../assets/icons/plus.svg"),
    ),
    (
        "icons/minus.svg",
        include_bytes!("../../assets/icons/minus.svg"),
    ),
    (
        "icons/check.svg",
        include_bytes!("../../assets/icons/check.svg"),
    ),
    (
        "icons/file-text.svg",
        include_bytes!("../../assets/icons/file-text.svg"),
    ),
    (
        "icons/database.svg",
        include_bytes!("../../assets/icons/database.svg"),
    ),
    (
        "icons/folder-input.svg",
        include_bytes!("../../assets/icons/folder-input.svg"),
    ),
    (
        "icons/git-branch.svg",
        include_bytes!("../../assets/icons/git-branch.svg"),
    ),
    (
        "icons/circle-check.svg",
        include_bytes!("../../assets/icons/circle-check.svg"),
    ),
    (
        "icons/trash-2.svg",
        include_bytes!("../../assets/icons/trash-2.svg"),
    ),
    (
        "icons/copyright.svg",
        include_bytes!("../../assets/icons/copyright.svg"),
    ),
    (
        "icons/pen-tool.svg",
        include_bytes!("../../assets/icons/pen-tool.svg"),
    ),
    (
        "icons/nav-menu.svg",
        include_bytes!("../../assets/icons/nav-menu.svg"),
    ),
    (
        "icons/nav-star.svg",
        include_bytes!("../../assets/icons/nav-star.svg"),
    ),
    (
        "icons/nav-settings.svg",
        include_bytes!("../../assets/icons/nav-settings.svg"),
    ),
    (
        "icons/share-2.svg",
        include_bytes!("../../assets/icons/share-2.svg"),
    ),
    (
        "icons/clock.svg",
        include_bytes!("../../assets/icons/clock.svg"),
    ),
    (
        "icons/film.svg",
        include_bytes!("../../assets/icons/film.svg"),
    ),
    (
        "icons/scan.svg",
        include_bytes!("../../assets/icons/scan.svg"),
    ),
    (
        "icons/square.svg",
        include_bytes!("../../assets/icons/square.svg"),
    ),
    (
        "icons/tag.svg",
        include_bytes!("../../assets/icons/tag.svg"),
    ),
    (
        "icons/circle-alert.svg",
        include_bytes!("../../assets/icons/circle-alert.svg"),
    ),
    (
        "icons/files.svg",
        include_bytes!("../../assets/icons/files.svg"),
    ),
    (
        "icons/video.svg",
        include_bytes!("../../assets/icons/video.svg"),
    ),
    (
        "icons/image.svg",
        include_bytes!("../../assets/icons/image.svg"),
    ),
    (
        "icons/layout-grid.svg",
        include_bytes!("../../assets/icons/layout-grid.svg"),
    ),
    (
        "icons/list.svg",
        include_bytes!("../../assets/icons/list.svg"),
    ),
    (
        "icons/rotate-ccw.svg",
        include_bytes!("../../assets/icons/rotate-ccw.svg"),
    ),
    (
        "icons/user-round.svg",
        include_bytes!("../../assets/icons/user-round.svg"),
    ),
];

pub(crate) struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = SUPPLEMENTAL.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }

        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut assets = gpui_component_assets::Assets.list(path)?;
        assets.extend(
            SUPPLEMENTAL
                .iter()
                .filter(|(name, _)| name.starts_with(path))
                .map(|(name, _)| SharedString::from(*name)),
        );

        Ok(assets)
    }
}

#[cfg(test)]
mod tests;
