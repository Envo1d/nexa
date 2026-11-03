use super::*;

#[test]
fn side_padding_starts_gestures_while_cards_and_scrollbar_do_not() {
    for list in [false, true] {
        let marquee = gesture((0.0, 5.0), (100.0, 50.0), list);
        let eligible = |x, y| {
            background_at(
                marquee.viewport.origin + point(px(x), px(y)),
                marquee.viewport,
                marquee.layout,
                px(0.0),
                40,
            )
        };
        assert!(eligible(8.0, 30.0), "left margin must accept gestures");
        assert!(eligible(1020.0, 30.0), "right margin must accept gestures");
        assert!(!eligible(30.0, 30.0), "card must keep click handling");
        assert!(
            !eligible(1035.0, 30.0),
            "scrollbar gutter must retain input"
        );
        assert!(!eligible(-1.0, 30.0));

        let from_left = gesture((8.0, 30.0), (100.0, 50.0), list);
        assert_eq!(
            from_left.selected(&(0..40).collect::<Vec<_>>(), px(0.0)),
            BTreeSet::from([0])
        );
        let from_right = gesture((1020.0, 30.0), (990.0, 50.0), list);
        assert_eq!(
            from_right.selected(&(0..40).collect::<Vec<_>>(), px(0.0)),
            BTreeSet::from([marquee.layout.columns - 1])
        );
    }
}

fn gesture(start: (f32, f32), end: (f32, f32), list: bool) -> Marquee {
    let viewport = Bounds::new(
        point(px(100.0), px(200.0)),
        gpui::size(px(1038.0), px(400.0)),
    );
    let mut marquee = Marquee::new(
        viewport.origin + point(px(start.0), px(start.1)),
        viewport,
        CollectionLayout::new(1000.0, list),
        px(0.0),
        BTreeSet::new(),
    );
    marquee.pointer = viewport.origin + point(px(end.0), px(end.1));

    marquee
}

#[test]
fn rectangle_selects_intersecting_grid_cards_in_both_directions() {
    let visible: Vec<_> = (0..40).collect();
    let forward = gesture((0.0, 5.0), (285.0, 100.0), false);
    let backward = gesture((285.0, 100.0), (0.0, 5.0), false);
    assert_eq!(forward.selected(&visible, px(0.0)), BTreeSet::from([0, 1]));
    assert_eq!(
        forward.selected(&visible, px(0.0)),
        backward.selected(&visible, px(0.0))
    );
}

#[test]
fn additive_selection_retains_base_but_replaces_previous_rectangle_hits() {
    let visible: Vec<_> = (0..40).collect();
    let mut marquee = gesture((0.0, 5.0), (285.0, 100.0), false);
    marquee.base.insert(39);
    assert_eq!(
        marquee.selected(&visible, px(0.0)),
        BTreeSet::from([0, 1, 39])
    );
    marquee.pointer.x = marquee.viewport.origin.x + px(100.0);
    assert_eq!(marquee.selected(&visible, px(0.0)), BTreeSet::from([0, 39]));
}

#[test]
fn scroll_extends_selection_into_virtualized_rows_in_filtered_order() {
    let visible: Vec<_> = (0..40).map(|index| 100 - index * 2).collect();
    let marquee = gesture((0.0, 5.0), (100.0, 50.0), false);
    let offset = px(-3.0 * marquee.layout.row_height);
    assert_eq!(
        marquee.selected(&visible, offset),
        BTreeSet::from([100, 92, 84, 76])
    );
    assert_eq!(marquee.rectangle(offset).top(), px(5.0));
}

#[test]
fn list_gaps_and_zero_area_do_not_select_neighboring_rows() {
    let visible: Vec<_> = (0..10).collect();
    let gap = gesture((0.0, 78.0), (500.0, 85.0), true);
    assert!(gap.selected(&visible, px(0.0)).is_empty());
    let click = gesture((0.0, 5.0), (0.0, 5.0), true);
    assert!(click.selected(&visible, px(0.0)).is_empty());
    let rows = gesture((0.0, 5.0), (500.0, 180.0), true);
    assert_eq!(rows.selected(&visible, px(0.0)), BTreeSet::from([0, 1, 2]));
}

#[test]
fn edge_scrolling_and_rectangle_are_bounded_by_viewport() {
    let mut marquee = gesture((0.0, 5.0), (2000.0, 500.0), false);
    assert_eq!(marquee.scroll_speed(), 720.0);
    assert_eq!(marquee.rectangle(px(0.0)).right(), px(1032.0));
    assert_eq!(marquee.rectangle(px(0.0)).bottom(), px(400.0));
    marquee.pointer.y = marquee.viewport.origin.y - px(20.0);
    assert_eq!(marquee.scroll_speed(), -720.0);
    marquee.pointer.y = marquee.viewport.origin.y + px(200.0);
    assert_eq!(marquee.scroll_speed(), 0.0);
}
