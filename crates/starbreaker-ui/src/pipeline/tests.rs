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
