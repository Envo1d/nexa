use super::*;

#[test]
fn all_seven_palettes_resolve_reference_roles_and_light_semantics() {
    assert_eq!(palettes().len(), 7);
    for index in 1..7 {
        assert_eq!(color_for(index, 0xeeedf6), token(index, "text"));
        assert_eq!(color_for(index, 0x141420), token(index, "surface"));
        assert_eq!(color_for(index, 0xa786ff), token(index, "accent"));
        assert_eq!(color_for(index, 0xffffff), 0xffffff);
    }
    assert_eq!(color_for(6, 0xeeedf6), 0x252332);
    assert_eq!(color_for(6, 0x141420), 0xffffff);
    assert_eq!(color_for(0, 0x2c2644), 0x2c2644);
}

#[test]
fn paper_keeps_media_readable_and_uses_reference_shadow_and_glow() {
    SELECTED.set(6);
    assert_eq!(media_accent(), gpui::rgb(0xc099ff));
    assert_eq!(shell_glow_strength(), 0.04);
    assert_eq!(rgba(0x00000080), gpui::rgba(0x221b5329));
    SELECTED.set(0);
    assert_eq!(media_accent(), gpui::rgb(0xa786ff));
    assert_eq!(shell_glow_strength(), 0.13);
    assert_eq!(rgba(0x00000080), gpui::rgba(0x00000080));
}
