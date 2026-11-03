use gpui::{AssetSource, SharedString};
use gpui_component::{IconName, IconNamed};

use super::Assets;

#[test]
fn bundled_and_supplemental_icons_have_visible_svg_coverage() {
    let assets = Assets;
    let names = assets.list("icons/").unwrap();
    assert!(names.len() > 80);

    for name in names {
        let bytes = assets.load(&name).unwrap().unwrap();
        let tree = resvg::usvg::Tree::from_data(&bytes, &resvg::usvg::Options::default()).unwrap();
        let size = tree.size().to_int_size();
        let mut pixels = resvg::tiny_skia::Pixmap::new(size.width(), size.height()).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixels.as_mut(),
        );

        assert!(
            pixels.pixels().iter().any(|pixel| pixel.alpha() > 0),
            "Empty SVG: {name}"
        );
    }
}

#[test]
fn component_icon_names_and_reference_variants_resolve_from_the_same_provider() {
    let names = [
        IconName::Menu,
        IconName::Folder,
        IconName::Star,
        IconName::Heart,
        IconName::Settings,
        IconName::Search,
        IconName::Info,
        IconName::Copy,
        IconName::Ellipsis,
        IconName::ChevronDown,
        IconName::Minus,
        IconName::WindowMaximize,
        IconName::Close,
        IconName::Check,
        IconName::ChevronRight,
    ];
    for name in names {
        assert!(Assets.load(&name.path()).unwrap().is_some());
    }
    for name in [
        "tag",
        "circle-alert",
        "files",
        "video",
        "image",
        "layout-grid",
        "list",
        "rotate-ccw",
        "user-round",
        "clock",
        "film",
        "scan",
        "square",
    ] {
        let path = format!("icons/{name}.svg");
        assert!(Assets.load(&path).unwrap().is_some());
        assert!(
            Assets
                .list(&path)
                .unwrap()
                .contains(&SharedString::from(path))
        );
    }
    assert!(Assets.load("").unwrap().is_none());
    assert!(Assets.load("icons/does-not-exist.svg").is_err());
    assert!(Assets.list("unrelated/").unwrap().is_empty());
}
