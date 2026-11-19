//! Layout tests for authored and styled size bounds (`sizing.min/max*` with their
//! `enable*` flags, the `MinSize*` / `MaxSize*` / `Enable*` style fields) and for
//! the canvas unit under the `aspectOverrides*` coordinate methods (WP-I1 fix pass 2).

use super::*;
use crate::bb_scene::parse_bb_canvas;

fn canvas(method: &str, nodes: serde_json::Value) -> crate::bb_scene::BbScene {
    parse_bb_canvas(&serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.test_size_bounds",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 1920.0, "y": 1080.0, "z": 0.0},
            "coordinateMethod": method,
            "scene": nodes,
            "operations": []
        }
    }))
    .expect("test canvas parses")
}

fn size(w: (f64, &str), h: (f64, &str), extra: serde_json::Value) -> serde_json::Value {
    let mut sizing = serde_json::json!({
        "_Type_": "BuildingBlocks_Size",
        "width": {"_Type_": "BuildingBlocks_FixedOrRelativeValue", "value": w.0, "behavior": w.1},
        "height": {"_Type_": "BuildingBlocks_FixedOrRelativeValue", "value": h.0, "behavior": h.1}
    });
    if let (Some(map), Some(extra)) = (sizing.as_object_mut(), extra.as_object()) {
        map.extend(extra.clone());
    }
    sizing
}

fn rect_of(scene: &crate::bb_scene::BbScene, result: &LayoutResult, name: &str) -> Rect {
    let id = scene.nodes.values().find(|n| n.name == name).map(|n| n.id).expect("node");
    result.rects[&id]
}

/// An enabled authored `maxHeight` caps a percent height; a disabled one does not.
/// The button standards' icon instances rely on it (`RootIconPosVertical` enables
/// the kit's 35-unit cap on a full-height icon).
#[test]
fn enabled_authored_max_height_caps_the_size() {
    let scene = canvas("useRaw", serde_json::json!([
        {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "root",
         "sizing": size((1.0, "Percent"), (1.0, "Percent"), serde_json::json!({}))},
        {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget", "name": "capped", "parent": "_PointsTo_:ptr:1",
         "sizing": size((1.0, "Percent"), (1.0, "Percent"), serde_json::json!({
            "maxHeight": {"_Type_": "BuildingBlocks_FixedOrRelativeValue", "value": 35.0, "behavior": "Fixed"},
            "enableMaxHeight": true}))},
        {"_Pointer_": "ptr:3", "_Type_": "BuildingBlocks_DisplayWidget", "name": "uncapped", "parent": "_PointsTo_:ptr:1",
         "sizing": size((1.0, "Percent"), (1.0, "Percent"), serde_json::json!({
            "maxHeight": {"_Type_": "BuildingBlocks_FixedOrRelativeValue", "value": 35.0, "behavior": "Fixed"},
            "enableMaxHeight": false}))}
    ]));
    let result = layout(&scene, 1920, 1080);
    assert!((rect_of(&scene, &result, "capped").h - 35.0).abs() < 0.5, "{:?}", rect_of(&scene, &result, "capped"));
    assert!((rect_of(&scene, &result, "capped").w - 1920.0).abs() < 0.5);
    assert!((rect_of(&scene, &result, "uncapped").h - 1080.0).abs() < 0.5);
}

/// Styled bounds (`MaxSizeY` / `EnableMaxHeight`, `MinSizeX` / `EnableMinWidth`)
/// override the authored ones, keeping the authored behaviour (Fixed units).
#[test]
fn styled_size_bounds_override_the_authored_bounds() {
    let mut scene = canvas("useRaw", serde_json::json!([
        {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "root",
         "sizing": size((1.0, "Percent"), (1.0, "Percent"), serde_json::json!({}))},
        {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget", "name": "styled", "parent": "_PointsTo_:ptr:1",
         "sizing": size((10.0, "Fixed"), (1.0, "Percent"), serde_json::json!({}))}
    ]));
    let node = scene.nodes.values_mut().find(|n| n.name == "styled").expect("node");
    let raw = node.raw.as_object_mut().expect("raw object");
    raw.insert("MaxSizeY".into(), serde_json::json!(20.0));
    raw.insert("EnableMaxHeight".into(), serde_json::json!(true));
    raw.insert("MinSizeX".into(), serde_json::json!(18.0));
    raw.insert("EnableMinWidth".into(), serde_json::json!(true));
    let result = layout(&scene, 1920, 1080);
    let r = rect_of(&scene, &result, "styled");
    assert!((r.h - 20.0).abs() < 0.5, "styled max height: {r:?}");
    assert!((r.w - 18.0).abs() < 0.5, "styled min width: {r:?}");
}

/// Under `aspectOverridesWidth` the target's aspect replaces the canvas WIDTH and
/// the authored height stays: one canvas unit is `target_h / canvas_h` on both
/// axes, so a Fixed square stays square on a portrait screen. The door footer's
/// 90 x 90 `Icon_DoorStatus_BG` drew 48 x 167 px on the 1034 x 2005 ring pad.
#[test]
fn aspect_overrides_width_keeps_fixed_units_square() {
    let scene = canvas("aspectOverridesWidth", serde_json::json!([
        {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "root",
         "sizing": size((1.0, "Percent"), (1.0, "Percent"), serde_json::json!({}))},
        {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget", "name": "square", "parent": "_PointsTo_:ptr:1",
         "sizing": size((90.0, "Fixed"), (90.0, "Fixed"), serde_json::json!({}))}
    ]));
    let result = layout(&scene, 1034, 2005);
    let unit = 2005.0 / 1080.0;
    let root = rect_of(&scene, &result, "root");
    assert!((root.w - 1034.0).abs() < 0.5 && (root.h - 2005.0).abs() < 0.5, "the canvas fills the target: {root:?}");
    let r = rect_of(&scene, &result, "square");
    assert!((r.w - 90.0 * unit).abs() < 0.5 && (r.h - 90.0 * unit).abs() < 0.5, "square in canvas units: {r:?}");
}

/// `aspectOverridesHeight` is the transpose: the authored width stays, one unit is
/// `target_w / canvas_w`.
#[test]
fn aspect_overrides_height_keeps_fixed_units_square() {
    let scene = canvas("aspectOverridesHeight", serde_json::json!([
        {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "root",
         "sizing": size((1.0, "Percent"), (1.0, "Percent"), serde_json::json!({}))},
        {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget", "name": "square", "parent": "_PointsTo_:ptr:1",
         "sizing": size((90.0, "Fixed"), (90.0, "Fixed"), serde_json::json!({}))}
    ]));
    let result = layout(&scene, 1034, 2005);
    let unit = 1034.0 / 1920.0;
    let r = rect_of(&scene, &result, "square");
    assert!((r.w - 90.0 * unit).abs() < 0.5 && (r.h - 90.0 * unit).abs() < 0.5, "square in canvas units: {r:?}");
}

/// A growing flex item's allocation respects its bounds (the flow used to hand it
/// the whole remaining space past a 35-unit maximum).
#[test]
fn a_growing_flex_item_respects_its_maximum() {
    let scene = canvas("useRaw", serde_json::json!([
        {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "root",
         "sizing": size((1.0, "Percent"), (1.0, "Percent"), serde_json::json!({})),
         "layoutPolicy": {"_Type_": "BuildingBlocks_FlexContainer", "direction": "Column", "wrap": "NoWrap",
            "axisJustification": "Start", "crossAxisJustification": "Stretch", "itemAlignment": "Stretch", "columnSpacing": 0, "rowSpacing": 0}},
        {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget", "name": "grow", "parent": "_PointsTo_:ptr:1",
         "layoutPolicyItem": {"growProportion": 1.0, "shrinkProportion": 1.0},
         "sizing": size((1.0, "Percent"), (10.0, "Fixed"), serde_json::json!({
            "maxHeight": {"_Type_": "BuildingBlocks_FixedOrRelativeValue", "value": 35.0, "behavior": "Fixed"},
            "enableMaxHeight": true}))}
    ]));
    let result = layout(&scene, 1920, 1080);
    assert!((rect_of(&scene, &result, "grow").h - 35.0).abs() < 0.5, "{:?}", rect_of(&scene, &result, "grow"));
}

/// A styled bound behaviour (`MaxWidthBehavior` Percent, `sk_strm_buttonsecondarystyles`)
/// makes the styled value a fraction of the parent.
#[test]
fn a_styled_percent_bound_is_a_fraction_of_the_parent() {
    let mut scene = canvas("useRaw", serde_json::json!([
        {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "root",
         "sizing": size((1.0, "Percent"), (1.0, "Percent"), serde_json::json!({}))},
        {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget", "name": "styled", "parent": "_PointsTo_:ptr:1",
         "sizing": size((1.0, "Percent"), (1.0, "Percent"), serde_json::json!({
            "maxWidth": {"_Type_": "BuildingBlocks_FixedOrRelativeValue", "value": 64.0, "behavior": "Fixed"}}))}
    ]));
    let node = scene.nodes.values_mut().find(|n| n.name == "styled").expect("node");
    let raw = node.raw.as_object_mut().expect("raw");
    raw.insert("MaxSizeX".into(), serde_json::json!(0.7));
    raw.insert("MaxWidthBehavior".into(), serde_json::json!("Percent"));
    raw.insert("EnableMaxWidth".into(), serde_json::json!(true));
    let result = layout(&scene, 1920, 1080);
    assert!((rect_of(&scene, &result, "styled").w - 0.7 * 1920.0).abs() < 0.5, "{:?}", rect_of(&scene, &result, "styled"));
}
