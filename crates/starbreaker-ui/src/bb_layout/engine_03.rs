#[allow(unused_imports)]
use super::*;
#[allow(unused_imports)]
use std::collections::BTreeMap;
#[allow(unused_imports)]
use image::{Rgba, RgbaImage};
#[allow(unused_imports)]
use log::warn;
#[allow(unused_imports)]
use crate::bb_scene::{BbCoordinateMethod, BbNode, BbNodeId, BbNodeType, BbScene, BbValue};

// Consolidated engine chunk 03: additional bb_layout tests that would push
// engine_02.part past the 3000-line cap (crates/starbreaker-ui/docs/ui-workflow.md rule 5).

#[cfg(test)]
mod materialised_entry_tests {
    use super::*;
    use crate::bb_scene::parse_bb_canvas;

    /// Two materialised LIST ENTRIES (Auto height, flagged `_MaterialisedEntry_`)
    /// in a SpaceEvenly column content-fit their text and DISTRIBUTE down the
    /// column instead of overlaying at one anchor (Auto's fill fallback) — the
    /// countermeasure list shows one launcher panel per entry. Without the
    /// materialised-entry content-fit both clones fill the column and pile onto
    /// one spot.
    #[test]
    fn materialised_list_entries_distribute_not_overlap() {
        let pct = serde_json::json!({"value": 1.0, "behavior": "Percent"});
        let pair = |e: u32, t: u32, txt: &str| {
            [
                serde_json::json!({"_Pointer_": format!("ptr:{e}"), "_Type_": "BuildingBlocks_DisplayWidget",
                    "name": "base_CountermeasureEntry", "parent": "_PointsTo_:ptr:1", "isActive": true,
                    "_MaterialisedEntry_": true, "sizing": {"width": pct, "height": {"value": 1.0, "behavior": "Auto"}}}),
                serde_json::json!({"_Pointer_": format!("ptr:{t}"), "_Type_": "BuildingBlocks_WidgetTextField",
                    "name": "text", "parent": format!("_PointsTo_:ptr:{e}"), "isActive": true,
                    "_ResolvedText_": txt, "text": txt, "fontSize": {"value": 40.0, "behavior": "Fixed"},
                    "sizing": {"width": pct, "height": {"value": 0.0, "behavior": "Auto"}}}),
            ]
        };
        let mut scene = vec![serde_json::json!({"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_WidgetList",
            "name": "List_Countermeasures", "isActive": true, "sizing": {"width": pct, "height": pct},
            "layoutPolicy": {"_Type_": "BuildingBlocks_FlexContainer", "direction": "Column", "axisJustification": "SpaceEvenly"}})];
        scene.extend(pair(2, 3, "48"));
        scene.extend(pair(4, 5, "5"));
        let canvas = serde_json::json!({"_RecordValue_": {"_Type_": "BuildingBlocks_Canvas",
            "size": {"x": 600.0, "y": 600.0}, "coordinateMethod": "useRaw", "operations": [], "scene": scene}});
        let result = layout(&parse_bb_canvas(&canvas).expect("fixture parses"), 600, 600);
        let (e0, e1) = (result.rects[&2], result.rects[&4]);
        assert!(e0.h < 300.0 && e1.h < 300.0, "entries content-fit, got {} / {}", e0.h, e1.h);
        let overlap = (e0.y.max(e1.y)) < (e0.y + e0.h).min(e1.y + e1.h);
        assert!(!overlap, "materialised entries must not overlap: {e0:?} vs {e1:?}");
    }

    /// A 16:9 `useRaw` canvas on a SQUARE target with `cover_fit` FILLS the target
    /// non-uniformly (`sx`/`sy`): a full-width/height `Percent` panel spans the
    /// whole square — NOT cover-overflowed (w>target, the old uniform-max path) nor
    /// contain-letterboxed (w<target). This is the cockpit countermeasure/ball
    /// screen fit (the 16:9 UI is stretched onto the square physical screen).
    #[test]
    fn useraw_cover_fit_fills_square_target_non_uniformly() {
        let pct = serde_json::json!({"value": 1.0, "behavior": "Percent"});
        let canvas = serde_json::json!({"_RecordValue_": {"_Type_": "BuildingBlocks_Canvas",
            "size": {"x": 1920.0, "y": 1080.0}, "coordinateMethod": "useRaw", "operations": [],
            "scene": [serde_json::json!({"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "panel", "isActive": true, "sizing": {"width": pct, "height": pct}})]}});
        let parsed = parse_bb_canvas(&canvas).expect("fixture parses");
        let result = layout_with_animation_sample(&parsed, 1920, 1920, None, true, true);
        let panel = result.rects[&1];
        assert!(
            (panel.w - 1920.0).abs() < 1.0 && (panel.h - 1920.0).abs() < 1.0,
            "fill: full-bleed panel must span the whole square target (no overflow / no letterbox), got {panel:?}"
        );
    }

    /// A CENTER cross-justified column item with a centred PIVOT (0.5) sits at
    /// the container cross-centre: the `(container.w - w) * 0.5` base already
    /// centres the item box, so the child pivot must NOT additionally shift it.
    /// The medical ghost-button close ✕ (pivot 0.5 in its Center-justified icon
    /// column) rendered a half-width LEFT of centre until the spurious
    /// `- pivot.x * w` was dropped from the cross-centre branch.
    #[test]
    fn flex_column_center_cross_axis_centers_pivoted_item() {
        let fixed = |v: f32| serde_json::json!({"value": v, "behavior": "Fixed"});
        let scene = vec![
            serde_json::json!({"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "root", "isActive": true, "sizing": {"width": fixed(100.0), "height": fixed(100.0)},
                "layoutPolicy": {"_Type_": "BuildingBlocks_FlexContainer", "direction": "Column",
                    "axisJustification": "Center", "crossAxisJustification": "Center", "itemAlignment": "Center"}}),
            serde_json::json!({"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_WidgetIcon",
                "name": "IconWidgetInstance", "parent": "_PointsTo_:ptr:1", "isActive": true,
                "pivot": {"x": 0.5, "y": 0.5, "z": 0.0},
                "sizing": {"width": fixed(40.0), "height": fixed(40.0)}}),
        ];
        let canvas = serde_json::json!({"_RecordValue_": {"_Type_": "BuildingBlocks_Canvas",
            "size": {"x": 100.0, "y": 100.0}, "coordinateMethod": "useRaw", "operations": [], "scene": scene}});
        let result = layout(&parse_bb_canvas(&canvas).expect("fixture parses"), 100, 100);
        let rect = result.rects[&2];
        // Cross-axis (X) and main-axis (Y) both centre a 40px item in 100px: (100-40)/2 = 30.
        assert!((rect.x - 30.0).abs() < 0.5, "expected pivot-0.5 item centred at x=30, got {}", rect.x);
        assert!((rect.y - 30.0).abs() < 0.5, "expected item centred at y=30, got {}", rect.y);
    }

    /// WP-I1 fix pass: a START cross-justified column item with no authored cross
    /// anchor sits at the column's start edge; its pivot (0.5) does not push it
    /// half its width out of the column. The light panel's full-width row frame
    /// (anchor 0, pivot 0.5 in a ColumnReverse Start/Start column) hung half off
    /// the left edge. Synthetic sizes.
    #[test]
    fn flex_column_start_cross_axis_keeps_unanchored_pivoted_item_in_column() {
        let fixed = |v: f32| serde_json::json!({"value": v, "behavior": "Fixed"});
        let pct = serde_json::json!({"value": 1.0, "behavior": "Percent"});
        let scene = vec![
            serde_json::json!({"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "column", "isActive": true, "position": {"x": 10.0, "y": 0.0, "z": 0.0},
                "sizing": {"width": fixed(80.0), "height": fixed(100.0)},
                "layoutPolicy": {"_Type_": "BuildingBlocks_FlexContainer", "direction": "ColumnReverse",
                    "axisJustification": "Start", "crossAxisJustification": "Start", "itemAlignment": "Start"}}),
            serde_json::json!({"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "frame", "parent": "_PointsTo_:ptr:1", "isActive": true,
                "pivot": {"x": 0.5, "y": 0.5, "z": 0.0}, "sizing": {"width": pct, "height": fixed(40.0)}}),
        ];
        let canvas = serde_json::json!({"_RecordValue_": {"_Type_": "BuildingBlocks_Canvas",
            "size": {"x": 100.0, "y": 100.0}, "coordinateMethod": "useRaw", "operations": [], "scene": scene}});
        let result = layout(&parse_bb_canvas(&canvas).expect("fixture parses"), 100, 100);
        let (column, frame) = (result.rects[&1], result.rects[&2]);
        assert!((frame.x - column.x).abs() < 0.5, "frame starts at the column's edge {}, got {frame:?}", column.x);
        assert!((frame.w - column.w).abs() < 0.5, "full width, got {frame:?}");
    }

    /// WP-I1 fix pass: non-grow items flow BESIDE a growing sibling, which takes
    /// what they leave. The door pad's column is header (Auto, content 30 high),
    /// DoorButton (grow 1) and button shelf (Auto, content 20 high); the header and
    /// shelf were overlaid on the whole column and the button filled it. Synthetic
    /// sizes.
    #[test]
    fn flex_column_non_grow_items_flow_around_a_growing_item() {
        let fixed = |v: f32| serde_json::json!({"value": v, "behavior": "Fixed"});
        let pct = |v: f32| serde_json::json!({"value": v, "behavior": "Percent"});
        let auto = serde_json::json!({"value": 1.0, "behavior": "Auto"});
        let scene = vec![
            serde_json::json!({"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "column", "isActive": true, "sizing": {"width": fixed(100.0), "height": fixed(200.0)},
                "layoutPolicy": {"_Type_": "BuildingBlocks_FlexContainer", "direction": "Column",
                    "axisJustification": "Center", "crossAxisJustification": "Stretch", "rowSpacing": 10.0}}),
            serde_json::json!({"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "header", "parent": "_PointsTo_:ptr:1", "isActive": true,
                "sizing": {"width": pct(1.0), "height": auto}}),
            serde_json::json!({"_Pointer_": "ptr:3", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "header_row", "parent": "_PointsTo_:ptr:2", "isActive": true,
                "sizing": {"width": pct(1.0), "height": fixed(30.0)}}),
            serde_json::json!({"_Pointer_": "ptr:4", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "button", "parent": "_PointsTo_:ptr:1", "isActive": true,
                "layoutPolicyItem": {"_Type_": "BuildingBlocks_FlexItem", "growProportion": 1.0, "shrinkProportion": 1.0},
                "sizing": {"width": pct(1.0), "height": pct(0.75)}}),
            serde_json::json!({"_Pointer_": "ptr:5", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "shelf", "parent": "_PointsTo_:ptr:1", "isActive": true,
                "sizing": {"width": pct(1.0), "height": auto}}),
            serde_json::json!({"_Pointer_": "ptr:6", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "shelf_row", "parent": "_PointsTo_:ptr:5", "isActive": true,
                "sizing": {"width": pct(1.0), "height": fixed(20.0)}}),
        ];
        let canvas = serde_json::json!({"_RecordValue_": {"_Type_": "BuildingBlocks_Canvas",
            "size": {"x": 100.0, "y": 200.0}, "coordinateMethod": "useRaw", "operations": [], "scene": scene}});
        let result = layout(&parse_bb_canvas(&canvas).expect("fixture parses"), 100, 200);
        let (header, button, shelf) = (result.rects[&2], result.rects[&4], result.rects[&5]);
        assert!((header.y - 0.0).abs() < 0.5 && (header.h - 30.0).abs() < 0.5, "header on top, content high: {header:?}");
        assert!((button.y - 40.0).abs() < 0.5, "button below the header and spacing: {button:?}");
        // 200 - 30 - 20 - 2 x 10 spacing = 130 for the growing button.
        assert!((button.h - 130.0).abs() < 0.5, "button takes what the others leave: {button:?}");
        assert!((shelf.y - 180.0).abs() < 0.5 && (shelf.h - 20.0).abs() < 0.5, "shelf at the bottom: {shelf:?}");
    }

    /// WP-I1 fix pass: a wrapping row with main-axis `Stretch` shares its free
    /// space among its items — the door pad's button shelf (20% buttons, 20 px
    /// spacing): two buttons split the bar, one spans it. They sat at the left at
    /// their authored 20% before. Synthetic sizes.
    #[test]
    fn flex_wrap_row_stretch_shares_the_free_space() {
        let pct = |v: f32| serde_json::json!({"value": v, "behavior": "Percent"});
        let button = |ptr: u32| serde_json::json!({"_Pointer_": format!("ptr:{ptr}"), "_Type_": "BuildingBlocks_DisplayWidget",
            "name": "button", "parent": "_PointsTo_:ptr:1", "isActive": true, "sizing": {"width": pct(0.2), "height": pct(1.0)}});
        let shelf = |buttons: &[u32]| {
            let mut scene = vec![serde_json::json!({"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "shelf", "isActive": true, "sizing": {"width": pct(1.0), "height": pct(1.0)},
                "layoutPolicy": {"_Type_": "BuildingBlocks_FlexContainer", "direction": "Row", "wrap": "Wrap",
                    "axisJustification": "Stretch", "crossAxisJustification": "Center", "columnSpacing": 20.0}})];
            scene.extend(buttons.iter().map(|p| button(*p)));
            let canvas = serde_json::json!({"_RecordValue_": {"_Type_": "BuildingBlocks_Canvas",
                "size": {"x": 200.0, "y": 50.0}, "coordinateMethod": "useRaw", "operations": [], "scene": scene}});
            layout(&parse_bb_canvas(&canvas).expect("fixture parses"), 200, 50)
        };
        let two = shelf(&[2, 3]);
        assert!((two.rects[&2].w - 90.0).abs() < 0.5 && (two.rects[&3].x - 110.0).abs() < 0.5, "two split the bar: {:?} {:?}", two.rects[&2], two.rects[&3]);
        let one = shelf(&[2]);
        assert!((one.rects[&2].w - 200.0).abs() < 0.5, "one spans it: {:?}", one.rects[&2]);
    }

    /// Review fix: a non-grow item beside a growing one keeps its own
    /// cross-dependent sizing and its authored cross offset — a `PercentOfY`
    /// width is a fraction of the item's OWN height (a 20-high bullet is 20
    /// wide, not the row's height), and a cross-axis position offset still
    /// moves it. Synthetic sizes.
    #[test]
    fn flex_non_grow_item_beside_a_grow_item_keeps_percent_of_y_and_cross_offset() {
        let fixed = |v: f32| serde_json::json!({"value": v, "behavior": "Fixed"});
        let scene = vec![
            serde_json::json!({"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "row", "isActive": true, "sizing": {"width": fixed(200.0), "height": fixed(100.0)},
                "layoutPolicy": {"_Type_": "BuildingBlocks_FlexContainer", "direction": "Row",
                    "axisJustification": "Start", "crossAxisJustification": "Start"}}),
            serde_json::json!({"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "bullet", "parent": "_PointsTo_:ptr:1", "isActive": true,
                "position": {"x": 0.0, "y": 30.0, "z": 0.0},
                "sizing": {"width": {"value": 1.0, "behavior": "PercentOfY"}, "height": fixed(20.0)}}),
            serde_json::json!({"_Pointer_": "ptr:3", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "fill", "parent": "_PointsTo_:ptr:1", "isActive": true,
                "layoutPolicyItem": {"_Type_": "BuildingBlocks_FlexItem", "growProportion": 1.0},
                "sizing": {"width": fixed(10.0), "height": fixed(100.0)}}),
        ];
        let canvas = serde_json::json!({"_RecordValue_": {"_Type_": "BuildingBlocks_Canvas",
            "size": {"x": 200.0, "y": 100.0}, "coordinateMethod": "useRaw", "operations": [], "scene": scene}});
        let result = layout(&parse_bb_canvas(&canvas).expect("fixture parses"), 200, 100);
        let (bullet, fill) = (result.rects[&2], result.rects[&3]);
        assert!((bullet.w - 20.0).abs() < 0.5 && (bullet.h - 20.0).abs() < 0.5, "a 20 x 20 bullet: {bullet:?}");
        assert!((bullet.y - 30.0).abs() < 0.5, "its cross offset stands: {bullet:?}");
        assert!((fill.x - 20.0).abs() < 0.5 && (fill.w - 180.0).abs() < 0.5, "the grow item takes the rest: {fill:?}");
    }

    /// The companion to the ✕ fix: a Center cross-justified column item that
    /// DOES author a cross-axis anchor keeps its overlay `anchor.x*W - pivot.x*w`
    /// offset. A self-consistent anchor==pivot==0.5 (the countermeasure firing
    /// box / list entries) cancels to stay centred and must NOT be box-centred
    /// away — guards the scope of the anchor.x==0 carve-out.
    #[test]
    fn flex_column_center_cross_axis_keeps_authored_anchor_offset() {
        let fixed = |v: f32| serde_json::json!({"value": v, "behavior": "Fixed"});
        let scene = vec![
            serde_json::json!({"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "root", "isActive": true, "sizing": {"width": fixed(100.0), "height": fixed(100.0)},
                "layoutPolicy": {"_Type_": "BuildingBlocks_FlexContainer", "direction": "Column",
                    "axisJustification": "Center", "crossAxisJustification": "Center", "itemAlignment": "Center"}}),
            serde_json::json!({"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget",
                "name": "entry", "parent": "_PointsTo_:ptr:1", "isActive": true,
                "anchor": {"x": 0.5, "y": 0.5, "z": 0.0}, "pivot": {"x": 0.5, "y": 0.5, "z": 0.0},
                "sizing": {"width": fixed(40.0), "height": fixed(40.0)}}),
        ];
        let canvas = serde_json::json!({"_RecordValue_": {"_Type_": "BuildingBlocks_Canvas",
            "size": {"x": 100.0, "y": 100.0}, "coordinateMethod": "useRaw", "operations": [], "scene": scene}});
        let result = layout(&parse_bb_canvas(&canvas).expect("fixture parses"), 100, 100);
        let rect = result.rects[&2];
        // anchor.x != 0, so the overlay offset is KEPT (the carve-out only applies
        // to anchor.x == 0). Legacy value: box-centre (100-40)/2 = 30, plus the
        // overlay term anchor.x*W - pivot.x*w = 0.5*100 - 0.5*40 = 30, so x = 60.
        // The assertion pins that an authored-anchor item is unchanged by the fix
        // (a box-centre carve-out would have given 30).
        assert!((rect.x - 60.0).abs() < 0.5, "expected authored-anchor item to keep overlay offset x=60, got {}", rect.x);
    }
}
