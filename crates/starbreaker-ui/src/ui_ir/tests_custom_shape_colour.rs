//! A `WidgetCustomShape`'s own overlay colour (WP-I1 fix pass 2): the shape authors
//! its glyph, overlay switch and colour as its OWN fields (`svgPath`,
//! `enableColorOverlay`, `color`), next to an unused `svgFill` block with a null
//! colour. The door footer's `Icon_DoorStatus` authors `color` Bright at 0.7; the IR
//! read only the null `svgFill.color` and fell back to the generic icon colour
//! (MissionObjectives, yellow).

use super::*;

fn compile(shape: serde_json::Value) -> UiIrNode {
    let canvas = serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.TestCustomShapeOwnColour",
        "_RecordValue_": {"size": {"x": 100, "y": 100}, "scene": [shape], "operations": []}
    });
    let scene = crate::bb_scene::parse_bb_canvas(&canvas).expect("scene parse");
    let ir = compile_ui_ir_from_scene(
        &scene,
        None,
        "guid-custom-shape-own-colour",
        Some("BuildingBlocks_Canvas.TestCustomShapeOwnColour"),
        (100, 100),
        &crate::defaults::DefaultValueRegistry::with_well_known_path_defaults(),
        None,
        None,
        &[],
        Vec::new(),
        Vec::new(),
        100,
    );
    ir.nodes.into_iter().find(|n| n.name == "Icon_DoorStatus").expect("shape node")
}

fn door_status_shape(colour: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "_Pointer_": "ptr:1",
        "_Type_": "BuildingBlocks_WidgetCustomShape",
        "name": "Icon_DoorStatus",
        "isActive": true,
        "svgFill": {
            "svgPath": "", "renderShape": true, "enableColorOverlay": true, "color": null,
            "containPositionX": 0.0, "containPositionY": 0.0
        },
        "renderShape": true,
        "svgPath": "UI/Textures/Vector/General/CommonIcons/icon_common_door_closed.svg",
        "enableColorOverlay": true,
        "color": colour,
        "size": {"width": {"behavior": "Fixed", "value": 50.0}, "height": {"behavior": "Fixed", "value": 50.0}}
    })
}

#[test]
fn custom_shape_overlay_takes_its_own_authored_colour() {
    let node = compile(door_status_shape(serde_json::json!({
        "_Type_": "BuildingBlocks_ColorStyle", "color": "Bright", "alpha": 0.7
    })));
    assert_eq!(node.icon_tint_colour_token.as_deref(), Some("Bright"), "the shape's own colour role");
    assert!((node.alpha - 0.7).abs() < 1e-4, "the own colour's alpha: {}", node.alpha);
}

#[test]
fn custom_shape_without_own_colour_keeps_the_generic_icon_colour() {
    let node = compile(door_status_shape(serde_json::Value::Null));
    assert_eq!(node.icon_tint_colour_token.as_deref(), Some("MissionObjectives"));
}

/// A styled literal fill replaces the own colour, its alpha included.
#[test]
fn a_styled_literal_fill_replaces_the_own_colour() {
    let mut shape = door_status_shape(serde_json::json!({"_Type_": "BuildingBlocks_ColorStyle", "color": "Bright", "alpha": 0.2}));
    shape["FillColor"] = serde_json::json!({"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0});
    let node = compile(shape);
    assert!((node.alpha - 1.0).abs() < 1e-4, "the literal fill's alpha, not the own 0.2: {}", node.alpha);
}

/// A styled role fill RESOLVED at style time carries its alpha in the RGBA, which the SVG fill applies: the node's alpha
/// does not take it again (review: 0.3 applied twice would draw at 0.09).
#[test]
fn a_resolved_styled_role_fill_is_not_alpha_scaled_twice() {
    let mut shape = door_status_shape(serde_json::Value::Null);
    shape["FillColorToken"] = serde_json::json!("Base");
    shape["FillColor"] = serde_json::json!({"r": 0.1, "g": 0.4, "b": 0.8, "a": 0.3});
    let node = compile(shape);
    assert_eq!(node.icon_tint_colour_token.as_deref(), Some("Base"));
    assert!((node.alpha - 1.0).abs() < 1e-4, "the fill carries its alpha, the node none: {}", node.alpha);
}

/// The token-only form (an entries-only kit sheet could not resolve the RGBA at style time): the role's alpha is
/// `FillColorTokenAlpha`.
#[test]
fn a_token_only_styled_role_fill_tints_at_the_recorded_alpha() {
    let mut shape = door_status_shape(serde_json::Value::Null);
    shape["FillColorToken"] = serde_json::json!("Base");
    shape["FillColorTokenAlpha"] = serde_json::json!(0.6);
    let node = compile(shape);
    assert_eq!(node.icon_tint_colour_token.as_deref(), Some("Base"));
    assert!((node.alpha - 0.6).abs() < 1e-4, "the recorded alpha: {}", node.alpha);
}
