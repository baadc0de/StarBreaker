use super::*;

#[test]
fn pipeline_defaults_seeds_placeholder_keys_and_merges_localization() {
    let defaults = DefaultValueRegistry::with_pipeline_defaults(Some(std::collections::HashMap::from([(
        "hud_custom".to_string(),
        "CUSTOM".to_string(),
    )])));

    assert_eq!(defaults.lookup_localization("@hud_custom"), Some("CUSTOM"));
    assert_eq!(defaults.lookup_localization("@loc_placeholder"), Some(""));
    assert_eq!(defaults.lookup_localization("@loc_empty"), Some(""));
}

#[test]
fn fallback_counter_warnings_emit_human_readable_messages() {
    let warnings = fallback_counter_warnings([
        ("swf_candidate_miss", 1),
        ("manufacturer_style_fallback_drak", 2),
        ("ignored_zero", 0),
    ]);

    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0], "fallback path used: swf_candidate_miss=1");
    assert_eq!(warnings[1], "fallback path used: manufacturer_style_fallback_drak=2");
}

/// A transit binding overlays its LOCALIZED floor name at the transit panel
/// location path; non-transit bindings reuse the shared registry untouched.
#[test]
fn binding_overlay_registry_pins_localized_floor_name() {
    let mut defaults = DefaultValueRegistry::new();
    defaults.set_localization(std::collections::HashMap::from([(
        "ui_interactor_test_floor".to_string(),
        "Test Floor".to_string(),
    )]));

    let binding = UiBindingView {
        canvas_guid: Some("guid"),
        content_canvas_guid: None,
        binding_kind: Some("physical"),
        manufacturer_id: None,
        helper_name: None,
        default_view_index: None,
        default_screen_slot: None,
        screen_name_loc_key: None,
        transit_location_loc_key: Some("@ui_interactor_test_floor"),
        host_swf_path: None,
        screen_aspect_w_over_h: None,
        texture_aspect_w_over_h: None,
        binding_values: &[],
        hovered: None,
    };
    let overlay = binding_overlay_registry(&binding, &defaults)
        .expect("transit binding must produce an overlay");
    assert_eq!(
        overlay.lookup_path("transitdisplay.panelLocation"),
        Some(&crate::canvas::Value::Str("Test Floor".to_string())),
    );

    // Unlocalized key falls back to the bare key (better than nothing).
    let bare = UiBindingView {
        transit_location_loc_key: Some("@ui_interactor_unknown"),
        ..binding
    };
    let overlay = binding_overlay_registry(&bare, &defaults).expect("overlay");
    assert_eq!(
        overlay.lookup_path("transitdisplay.panelLocation"),
        Some(&crate::canvas::Value::Str("ui_interactor_unknown".to_string())),
    );

    // Non-transit binding → no overlay (shared registry reused).
    let none = UiBindingView {
        transit_location_loc_key: None,
        ..binding
    };
    assert!(binding_overlay_registry(&none, &defaults).is_none());
}

/// I1-5 root cause 4: the screen owner's per-binding values (its user variables
/// with instance overrides, its interaction state) overlay the shared registry
/// for that render only; a binding without any reuses the registry untouched.
#[test]
fn binding_overlay_registry_pins_the_owners_values() {
    let defaults = DefaultValueRegistry::new();
    let values = [
        ("test_HideSomething".to_string(), crate::canvas::Value::Bool(true)),
        ("state.TestState.TestOn".to_string(), crate::canvas::Value::Bool(true)),
    ];
    let binding = UiBindingView {
        canvas_guid: Some("guid"),
        content_canvas_guid: None,
        binding_kind: Some("physical"),
        manufacturer_id: None,
        helper_name: None,
        default_view_index: None,
        default_screen_slot: None,
        screen_name_loc_key: None,
        transit_location_loc_key: None,
        host_swf_path: None,
        screen_aspect_w_over_h: None,
        texture_aspect_w_over_h: None,
        binding_values: &values,
        hovered: None,
    };
    let overlay = binding_overlay_registry(&binding, &defaults).expect("values make an overlay");
    assert_eq!(overlay.lookup_path("test_hidesomething"), Some(&crate::canvas::Value::Bool(true)));
    assert_eq!(overlay.lookup_path("state.teststate.teston"), Some(&crate::canvas::Value::Bool(true)));
    assert!(defaults.lookup_path("test_HideSomething").is_none(), "the shared registry is not touched");
}

/// I1-2: a screen whose render target lands on its face at a known TEXTURE
/// aspect (UV space) renders at that aspect with the authored canvas's pixel
/// budget: a 16:9 canvas on a 9:16 face renders 1080 × 1920, not 1920 × 3413
/// and not a stretched 1920 × 1080.
#[test]
fn texture_aspect_target_keeps_the_canvas_pixel_area() {
    assert_eq!(texture_aspect_target_size((1920, 1080), 0.5625), Some((1080, 1920)));
    assert_eq!(texture_aspect_target_size((1920, 1080), 16.0 / 9.0), Some((1920, 1080)));
    assert_eq!(texture_aspect_target_size((1920, 1080), 1.0), Some((1440, 1440)));
    assert_eq!(texture_aspect_target_size((512, 740), 1.0), Some((616, 616)));
    // Review fix: an extreme aspect (a nearly degenerate UV mapping) is refused, not rendered 60000 px wide.
    assert_eq!(texture_aspect_target_size((1920, 1080), 2000.0), None);
}

/// A minimal IR node (synthetic geometry) for the click-region rule.
fn click_node(id: u32, parent: Option<u32>, active: bool, rect: [f32; 4], triggers: &[&str]) -> crate::ui_ir::UiIrNode {
    let mut node: crate::ui_ir::UiIrNode = serde_json::from_value(serde_json::json!({
        "id": id, "parent_id": parent, "children": [], "node_type": "widget_canvas", "name": format!("n{id}"),
        "is_active": active, "layer": 0, "alpha": 1.0, "anchor": [0.0, 0.0], "pivot": [0.0, 0.0],
        "authored_position": [0.0, 0.0],
        "authored_size": [{"kind": "fixed", "value": rect[2]}, {"kind": "fixed", "value": rect[3]}],
        "padding": [0.0, 0.0, 0.0, 0.0], "margin": [0.0, 0.0, 0.0, 0.0],
        "computed_rect": {"x": rect[0], "y": rect[1], "w": rect[2], "h": rect[3]},
        "style_tag_uuids": [], "resolved_style_tags": []
    }))
    .expect("a minimal IR node");
    node.left_click_triggers = triggers.iter().map(|t| t.to_string()).collect();
    node
}

/// scviewer C2 fixes: a click region is a node with left-click triggers whose whole ancestor chain is active, one per
/// trigger, its rect scaled from the IR target to the image.
#[test]
fn click_targets_are_active_triggering_nodes_scaled_to_the_image() {
    let mut ir: crate::ui_ir::UiIrDocument = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "canvas_guid": "g", "canvas_name": null, "target_width": 100, "target_height": 200,
        "selected_style_source": null, "renderer_hint": "bb", "confidence": 100, "warnings": [],
        "unresolved_references": [], "resolved_asset_refs": [], "missing_asset_refs": [], "nodes": []
    }))
    .unwrap();
    ir.nodes = vec![
        click_node(1, None, true, [0.0, 0.0, 100.0, 200.0], &[]),
        click_node(2, Some(1), true, [10.0, 20.0, 30.0, 40.0], &["Go"]),
        click_node(3, None, false, [0.0, 0.0, 100.0, 200.0], &[]),
        click_node(4, Some(3), true, [10.0, 20.0, 30.0, 40.0], &["Hidden"]),
        click_node(5, Some(1), true, [0.0, 0.0, 1.0, 1.0], &["A", "B"]),
    ];
    let targets = click_targets(&ir, (200, 400));
    let summary: Vec<(&str, &str, [f32; 4])> =
        targets.iter().map(|t| (t.node.as_str(), t.trigger.as_str(), t.rect_px)).collect();
    assert_eq!(
        summary,
        [("n2", "Go", [20.0, 40.0, 60.0, 80.0]), ("n5", "A", [0.0, 0.0, 2.0, 2.0]), ("n5", "B", [0.0, 0.0, 2.0, 2.0])],
        "an inactive ancestor hides a node's region"
    );
}

/// WP-I1-4: a region's caption is the first resolved, non-empty text of the clickable node's active subtree, in node
/// order (a light panel row's "Auxiliary" under its button node); none without one.
#[test]
fn click_targets_carry_the_buttons_caption() {
    let mut ir: crate::ui_ir::UiIrDocument = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "canvas_guid": "g", "canvas_name": null, "target_width": 100, "target_height": 100,
        "selected_style_source": null, "renderer_hint": "bb", "confidence": 100, "warnings": [],
        "unresolved_references": [], "resolved_asset_refs": [], "missing_asset_refs": [], "nodes": []
    }))
    .unwrap();
    let text = |mut n: crate::ui_ir::UiIrNode, t: &str| {
        n.text_payload = Some(crate::ui_ir::UiIrTextPayload::Resolved { text: t.to_string() });
        n
    };
    let mut button = click_node(1, None, true, [0.0, 0.0, 50.0, 50.0], &["Go"]);
    button.children = vec![2, 3, 4];
    let mut bare = click_node(5, None, true, [50.0, 0.0, 50.0, 50.0], &["Stop"]);
    bare.children = vec![6];
    ir.nodes = vec![
        button,
        text(click_node(2, Some(1), false, [0.0; 4], &[]), "Hidden"),
        text(click_node(3, Some(1), true, [0.0; 4], &[]), "  "),
        text(click_node(4, Some(1), true, [0.0; 4], &[]), "Auxiliary"),
        bare,
        click_node(6, Some(5), true, [0.0; 4], &[]),
    ];
    let labels: Vec<Option<String>> = click_targets(&ir, (100, 100)).into_iter().map(|t| t.label).collect();
    assert_eq!(labels, [Some("Auxiliary".to_string()), None]);
}

/// The triggers are the `BuildingBlocks_TriggerURL` names of an interactable widget's `onLeftClick` (synthetic).
#[test]
fn left_click_triggers_need_an_enabled_interactable_widget() {
    let raw = |can: bool, disabled: bool| {
        serde_json::json!({"interactions": {"canInteract": can, "disabled": disabled, "onLeftClick": [
            {"_Type_": "BuildingBlocks_TriggerURL", "name": "Go"},
            {"_Type_": "BuildingBlocks_SomethingElse", "name": "Other"},
            {"_Type_": "BuildingBlocks_TriggerURL", "name": ""}
        ]}})
    };
    assert_eq!(crate::ui_ir::left_click_triggers(&raw(true, false)), ["Go"]);
    assert!(crate::ui_ir::left_click_triggers(&raw(false, false)).is_empty());
    assert!(crate::ui_ir::left_click_triggers(&raw(true, true)).is_empty());
    assert!(crate::ui_ir::left_click_triggers(&serde_json::json!({})).is_empty());
}
