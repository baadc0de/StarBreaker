//! Tests for widget-standard template expansion (`engine_04`): host icon forwarding into the button templates' icon
//! instance, host fallback clearing, the large button and its host-bound label, the modular-kit sheets, scrollbars and
//! the expansion id bands.
#![allow(unused_imports, dead_code)]

use super::*;

fn tag_database_fixture() -> serde_json::Value {
    serde_json::json!({
        "tags": [
            {"tagName": "canvas-proxy-root", "_RecordId_": "21788313-6aff-45b4-a3ad-fdc62b1cf849"},
            {"tagName": "general-button-secondary", "_RecordId_": "cf236683-1f17-41ee-a0d2-0e23c5f1c552"},
            {"tagName": "icon", "_RecordId_": "8b9ef179-0000-0000-0000-000000000000"}
        ]
    })
}

/// Minimal stand-in for `buttongeneralsecondarycomponentstandard`:
/// ComponentRoot (proxy-root tagged) hosting a `WidgetIcon` instance whose
/// authored preset is the template default (`ArrowHollowRight`).
fn button_template_fixture() -> serde_json::Value {
    serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.ButtonGeneralSecondaryComponentStandard",
        "_RecordId_": "00000000-0000-0000-0000-00000000b001",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 64.0, "y": 64.0, "z": 0.0},
            "scene": [
                {
                    "_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget",
                    "name": "ComponentRoot",
                    "styleTags": [{"_RecordId_": "21788313-6aff-45b4-a3ad-fdc62b1cf849"}]
                },
                {
                    "_Pointer_": "ptr:8", "_Type_": "BuildingBlocks_WidgetIcon",
                    "name": "IconWidgetInstance", "parent": "_PointsTo_:ptr:1",
                    "iconProperties": {
                        "_Type_": "BuildingBlocks_ComponentIconProperties",
                        "show": true, "iconPreset": "ArrowHollowRight", "customIcon": ""
                    }
                }
            ],
            "operations": [
                {
                    "_Pointer_": "ptr:30",
                    "_Type_": "BuildingBlocks_BindingsNumberComponentParameter",
                    "name": "Icon AnchorToParentX", "parameter": "ParamInput0",
                    "defaultValue": 0.0
                },
                {
                    "_Pointer_": "ptr:31",
                    "_Type_": "BuildingBlocks_BindingsNumberComponentParameter",
                    "name": "Icon AnchorToParentY", "parameter": "ParamInput1",
                    "defaultValue": 0.0
                }
            ]
        }
    })
}

/// Host canvas: a `ComponentGeneralButtonSecondary` authored with the
/// medical close button's icon contract (show=true, preset `GeneralX`).
fn button_host_scene() -> BbScene {
    let canvas = serde_json::json!({
        "_RecordName_": "test_button_host",
        "_RecordId_": "00000000-0000-0000-0000-00000000c001",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 1920.0, "y": 1080.0, "z": 0.0},
            "scene": [
                {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "root"},
                {
                    "_Pointer_": "ptr:2",
                    "_Type_": "BuildingBlocks_ComponentGeneralButtonSecondary",
                    "name": "ExitBed", "parent": "_PointsTo_:ptr:1",
                    "fillStyle": "Ghost",
                    "iconProperties": {
                        "_Type_": "BuildingBlocks_ComponentIconProperties",
                        "show": true, "iconPreset": "GeneralX", "customIcon": "",
                        "anchorToParentX": 0.5, "anchorToParentY": 0.5
                    },
                    "labelProperties": {
                        "_Type_": "BuildingBlocks_ComponentLabelProperties",
                        "show": false, "label": "@LOC_PLACEHOLDER"
                    }
                }
            ],
            "operations": []
        }
    });
    parse_bb_canvas(&canvas).expect("host canvas should parse")
}

fn expansion_fetcher(path: &str) -> Result<serde_json::Value, String> {
    if path == tag_database_path() {
        Ok(tag_database_fixture())
    } else if path == button_secondary_component_standard_path() {
        Ok(button_template_fixture())
    } else {
        Err(format!("test fetcher: no record for '{path}'"))
    }
}

/// The engine routes a button's authored `iconProperties` through the
/// template's IconPreset/CustomIcon parameters into its icon instance:
/// the merged `IconWidgetInstance` must carry the HOST's glyph identity,
/// not the template's authored default.
#[test]
fn button_expansion_forwards_host_icon_identity_to_icon_instance() {
    let mut scene = button_host_scene();
    expand_widget_standards(&mut scene, &expansion_fetcher);

    let icon_instance = scene
        .nodes
        .values()
        .find(|n| matches!(n.ty, BbNodeType::WidgetIcon) && n.id >= EXPANSION_ID_BASE)
        .expect("expansion should merge the template's WidgetIcon instance");
    assert_eq!(
        icon_instance
            .raw
            .get("iconProperties")
            .and_then(|p| p.get("iconPreset"))
            .and_then(|v| v.as_str()),
        Some("GeneralX"),
        "icon instance must inherit the host button's authored iconPreset"
    );
    assert_eq!(
        host_icon_path(icon_instance),
        crate::icon_preset::svg_path_for_preset("GeneralX"),
        "forwarded preset must resolve to the host glyph's vector asset"
    );
    // Assert the layer the RENDERER reads: forwarding only `raw` while the
    // parse-time `BbIcon` kept the template default passed the raw-level
    // assertion but drew the wrong glyph (ledger 103).
    let baked = icon_instance
        .icon
        .as_ref()
        .expect("merged icon instance must carry a parse-time BbIcon");
    assert_eq!(
        baked.image_record,
        crate::icon_preset::svg_path_for_preset("GeneralX"),
        "the baked BbIcon must resolve the forwarded preset, not the template default"
    );
}

/// The expanded instance renders the glyph; the host's parse-time icon
/// fallback must be cleared or the glyph draws twice.
#[test]
fn button_expansion_clears_host_icon_fallback() {
    let mut scene = button_host_scene();
    expand_widget_standards(&mut scene, &expansion_fetcher);

    let host = scene
        .nodes
        .values()
        .find(|n| matches!(n.ty, BbNodeType::ComponentGeneralButtonSecondary))
        .expect("host should survive expansion");
    assert!(
        host.icon.as_ref().is_none_or(|icon| icon.image_record.is_none()),
        "button host parse-time icon fallback must be cleared after expansion"
    );
}

/// The templates derive the icon instance's ANCHOR from the host's
/// authored `iconProperties.anchorToParentX/Y` through the "Icon
/// AnchorToParent*" component parameters; an unwired parameter defaults
/// 0.0 and the template op graph (f(param) − 1 + switch) yields −1,
/// throwing the icon out of the button (the medbed close ✕ rendered left
/// of its square). The expansion must inject the authored numbers.
#[test]
fn button_expansion_injects_host_anchor_to_parent_params() {
    let mut scene = button_host_scene();
    expand_widget_standards(&mut scene, &expansion_fetcher);

    let synth: Vec<_> = scene
        .operations
        .iter()
        .filter(|op| op.get("_Type_").and_then(|v| v.as_str()) == Some("_SynthNumberParam_"))
        .collect();
    assert_eq!(synth.len(), 2, "both icon anchor params injected: {synth:?}");
    for op in synth {
        assert_eq!(
            op.get("resolvedNumber").and_then(|v| v.as_f64()),
            Some(0.5),
            "the host's authored anchorToParent 0.5 must reach the parameter"
        );
    }
}

/// Host canvas: a PRIMARY `ComponentGeneralButton` authored with the
/// transit call-console's contract (fillStyle `Filled`, double-caret icon
/// above an `@ui_interactor_call_elevator` label).
fn primary_button_host_scene() -> BbScene {
    let canvas = serde_json::json!({
        "_RecordName_": "test_primary_button_host",
        "_RecordId_": "00000000-0000-0000-0000-00000000c002",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 512.0, "y": 740.0, "z": 0.0},
            "scene": [
                {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "root"},
                {
                    "_Pointer_": "ptr:2",
                    "_Type_": "BuildingBlocks_ComponentGeneralButton",
                    "name": "Button_Call", "parent": "_PointsTo_:ptr:1",
                    "fillStyle": "Filled",
                    "iconPosition": "Top",
                    "iconProperties": {
                        "_Type_": "BuildingBlocks_ComponentIconProperties",
                        "show": true, "iconPreset": "ArrowCaratDoubleUp", "customIcon": ""
                    },
                    "labelProperties": {
                        "_Type_": "BuildingBlocks_ComponentLabelProperties",
                        "show": true, "label": "@ui_interactor_call_elevator"
                    }
                }
            ],
            "operations": []
        }
    });
    parse_bb_canvas(&canvas).expect("host canvas should parse")
}

fn primary_expansion_fetcher(path: &str) -> Result<serde_json::Value, String> {
    if path == tag_database_path() {
        Ok(tag_database_fixture())
    } else if path == button_general_component_standard_path() {
        Ok(button_template_fixture())
    } else {
        Err(format!("test fetcher: no record for '{path}'"))
    }
}

/// A PRIMARY `ComponentGeneralButton` (e.g. a transit console's CALL
/// ELEVATOR button) expands through its component standard exactly like
/// the secondary variant: the merged icon instance carries the HOST's
/// authored glyph identity.
#[test]
fn primary_button_expansion_forwards_host_icon_identity() {
    let mut scene = primary_button_host_scene();
    expand_widget_standards(&mut scene, &primary_expansion_fetcher);

    let icon_instance = scene
        .nodes
        .values()
        .find(|n| matches!(n.ty, BbNodeType::WidgetIcon) && n.id >= EXPANSION_ID_BASE)
        .expect("expansion should merge the primary standard's WidgetIcon instance");
    assert_eq!(
        icon_instance
            .raw
            .get("iconProperties")
            .and_then(|p| p.get("iconPreset"))
            .and_then(|v| v.as_str()),
        Some("ArrowCaratDoubleUp"),
        "icon instance must inherit the primary host's authored iconPreset"
    );
    // Assert the layer the RENDERER reads (ledger 103): the parse-time
    // `BbIcon` bakes the preset→SVG resolution, so a raw-only forward
    // passes the assertion above while still drawing the template glyph.
    let baked = icon_instance
        .icon
        .as_ref()
        .expect("merged icon instance must carry a parse-time BbIcon");
    assert_eq!(
        baked.image_record,
        crate::icon_preset::svg_path_for_preset("ArrowCaratDoubleUp"),
        "the baked BbIcon must resolve the forwarded preset, not the template default"
    );
}

/// Minimal stand-in for `buttongenerallargecomponentstandard` (structure
/// only, no game values): proxy-tagged `ComponentRoot` hosting the label
/// text-field instance and the background shape, with the template's
/// `Label` (localized) and `ShowLabel` (boolean) component parameters bound
/// to the text instance's `ParamInput0` / `IsActive`, as the real template
/// authors them.
fn large_button_template_fixture() -> serde_json::Value {
    serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.ButtonGeneralLargeComponentStandard",
        "_RecordId_": "00000000-0000-0000-0000-00000000b003",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 1920.0, "y": 1080.0, "z": 0.0},
            "scene": [
                {
                    "_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget",
                    "name": "ComponentRoot",
                    "styleTags": [{"_RecordId_": "21788313-6aff-45b4-a3ad-fdc62b1cf849"}]
                },
                {
                    "_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_WidgetTextField",
                    "name": "TextFieldWidgetInstance", "parent": "_PointsTo_:ptr:1",
                    "labelProperties": {
                        "_Type_": "BuildingBlocks_ComponentLabelProperties",
                        "show": true, "label": "@LOC_PLACEHOLDER", "caseModifier": "None"
                    }
                },
                {
                    "_Type_": "BuildingBlocks_WidgetCustomShape",
                    "name": "BackgroundInstance", "parent": "_PointsTo_:ptr:1"
                }
            ],
            "operations": [
                {"_Pointer_": "ptr:3", "_Type_": "BuildingBlocks_BindingsLocalizedComponentParameter",
                 "name": "Label", "parameter": "ParamInput0", "defaultValue": "@LOC_PLACEHOLDER"},
                {"_Type_": "BuildingBlocks_BindingsLocalizedField",
                 "widget": "_PointsTo_:ptr:2", "field": "ParamInput0", "input": "_PointsTo_:ptr:3"},
                {"_Pointer_": "ptr:5", "_Type_": "BuildingBlocks_BindingsBooleanComponentParameter",
                 "name": "ShowLabel", "parameter": "ParamInput0", "defaultValue": true},
                {"_Type_": "BuildingBlocks_BindingsBooleanField",
                 "widget": "_PointsTo_:ptr:2", "field": "IsActive", "input": "_PointsTo_:ptr:5"}
            ]
        }
    })
}

fn large_tag_database_fixture() -> serde_json::Value {
    serde_json::json!({
        "tags": [
            {"tagName": "canvas-proxy-root", "_RecordId_": "21788313-6aff-45b4-a3ad-fdc62b1cf849"},
            {"tagName": "general-button-large", "_RecordId_": "00000000-0000-0000-0000-0000000a1a7e"}
        ]
    })
}

/// Host canvas: a `ComponentGeneralButtonLarge` whose label is BOUND by
/// the host canvas (a door pad's `DoorButton`: `LocalizedField →
/// ParamInput0` from a boolean state), with an authored design-time label.
/// Synthetic keys only.
fn large_button_host_scene() -> BbScene {
    let canvas = serde_json::json!({
        "_RecordName_": "test_large_button_host",
        "_RecordId_": "00000000-0000-0000-0000-00000000c003",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 1920.0, "y": 1080.0, "z": 0.0},
            "scene": [
                {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "root"},
                {
                    "_Pointer_": "ptr:7",
                    "_Type_": "BuildingBlocks_ComponentGeneralButtonLarge",
                    "name": "MainButton", "parent": "_PointsTo_:ptr:1",
                    "iconProperties": {
                        "_Type_": "BuildingBlocks_ComponentIconProperties",
                        "show": false, "iconPreset": "ArrowHollowRight", "customIcon": ""
                    },
                    "labelProperties": {
                        "_Type_": "BuildingBlocks_ComponentLabelProperties",
                        "show": true, "label": "@test_label_authored", "caseModifier": "Upper", "style": "Title4"
                    }
                }
            ],
            "operations": [
                {"_Pointer_": "ptr:10", "_Type_": "BuildingBlocks_BindingsBooleanVariable",
                 "path": [], "binding": "test.state.flag"},
                {"_Pointer_": "ptr:16", "_Type_": "BuildingBlocks_BindingsLocalizedFromBoolean",
                 "isTrue": "@test_label_on", "isFalse": "@test_label_off", "input": "_PointsTo_:ptr:10"},
                {"_Type_": "BuildingBlocks_BindingsLocalizedField",
                 "widget": "_PointsTo_:ptr:7", "field": "ParamInput0", "input": "_PointsTo_:ptr:16"}
            ]
        }
    });
    parse_bb_canvas(&canvas).expect("host canvas should parse")
}

fn large_expansion_fetcher(path: &str) -> Result<serde_json::Value, String> {
    if path == tag_database_path() {
        Ok(large_tag_database_fixture())
    } else if path == button_large_component_standard_path() {
        Ok(large_button_template_fixture())
    } else {
        Err(format!("test fetcher: no record for '{path}'"))
    }
}

/// I1-5 root cause 1: the engine instantiates `ButtonGeneralLargeComponentStandard`
/// under every large button (its frame, background and centred label). The
/// expansion merges the template's instances under the host, in the SECOND
/// expansion band (a host type added after the frozen set must not renumber
/// their instance ids), and tags the host `general-button-large`.
#[test]
fn large_button_expands_from_its_component_standard_in_the_second_band() {
    let mut scene = large_button_host_scene();
    expand_widget_standards(&mut scene, &large_expansion_fetcher);

    let host_id = scene
        .nodes
        .values()
        .find(|n| n.name == "MainButton")
        .map(|n| n.id)
        .expect("host survives expansion");
    let text = scene
        .nodes
        .values()
        .find(|n| n.name == "TextFieldWidgetInstance")
        .expect("the template's label instance is merged");
    assert!(text.id >= EXPANSION_ID_BASE_SECOND, "second band, got {:#x}", text.id);
    let root = scene.nodes.values().find(|n| n.name == "ComponentRoot").expect("template root merged");
    assert_eq!(root.parent, Some(host_id), "the template root hangs under the host");
    assert!(
        scene.nodes.values().any(|n| n.name == "BackgroundInstance"),
        "the template's background shape is merged"
    );
    let host = &scene.nodes[&host_id];
    assert!(
        host.style_tag_uuids
            .iter()
            .any(|t| t.eq_ignore_ascii_case("00000000-0000-0000-0000-0000000a1a7e")),
        "the host carries its implicit general-button-large tag: {:?}",
        host.style_tag_uuids
    );
}

/// The host canvas binds the large button's label parameter (`LocalizedField
/// → host.ParamInput0`, the door's OPEN / CLOSE / LOCKED switch): that
/// binding, not the host's authored design-time label, is what the
/// template's `Label` parameter carries into its text instance.
#[test]
fn large_button_label_follows_the_hosts_bound_label_parameter() {
    let mut scene = large_button_host_scene();
    expand_widget_standards(&mut scene, &large_expansion_fetcher);
    let text = scene
        .nodes
        .values()
        .find(|n| n.name == "TextFieldWidgetInstance")
        .expect("label instance merged")
        .clone();
    let mut defaults = crate::defaults::DefaultValueRegistry::new();
    defaults.insert_localization("test_label_authored", "authored".to_string());
    defaults.insert_localization("test_label_on", "on".to_string());
    defaults.insert_localization("test_label_off", "off".to_string());
    let resolver = crate::bb_bindings::BindingResolver::from_operations(&scene.operations);
    defaults.insert_path("test.state.flag", crate::canvas::Value::Bool(true));
    assert_eq!(resolver.resolve_text(text.id, &text.raw, &defaults), "ON");
    defaults.insert_path("test.state.flag", crate::canvas::Value::Bool(false));
    assert_eq!(resolver.resolve_text(text.id, &text.raw, &defaults), "OFF");
    // ... and the host, whose canvas binds that label, draws no text of its own.
    let host = scene.nodes.values().find(|n| n.name == "MainButton").expect("host");
    assert_eq!(host.raw.get(LABEL_INSTANCED_KEY).and_then(|v| v.as_bool()), Some(true));
    // The host's label style (the template's `LabelStyle` parameter) reaches the instance.
    assert_eq!(
        text.raw.get("labelProperties").and_then(|lp| lp.get("style")).and_then(|v| v.as_str()),
        Some("Title4")
    );
}

/// The brand's modular-kit LARGE-button sheet (`sk_<brand>_buttonlargestyles`) is
/// part of the standard-module tier like the primary / secondary sheets: its
/// entries (the large button's fill and label colours) apply to the scene.
#[test]
fn modular_kit_sheets_include_the_large_button_sheet() {
    let mut scene = large_button_host_scene();
    let sheet = serde_json::json!({
        "_RecordValue_": {"entries": [{
            "_Type_": "BuildingBlocks_StyleEntry", "name": "test",
            "conditionsList": [{"conditions": [
                {"_Type_": "BuildingBlocks_StyleSelectorConditionType", "type": "Base"}
            ]}],
            "modifiers": [{"_Type_": "BuildingBlocks_FieldModifierBoolean", "field": "IsActive", "value": false}]
        }]}
    });
    let fetch = |path: &str| -> Result<serde_json::Value, String> {
        if path.ends_with("sk_test/sk_test_buttonlargestyles.json") {
            Ok(sheet.clone())
        } else {
            Err(format!("no record for '{path}'"))
        }
    };
    apply_modular_kit_sheets(&mut scene, "s_test", &fetch, None);
    let root = scene.nodes.values().find(|n| n.name == "root").expect("root");
    assert!(!root.is_active, "the large-button sheet's entry applies");
}

/// A canvas whose `brandStyles` carry no entry for the ship's manufacturer
/// (the door pad's `I_Door_Standard` lists orig / rsi / … on a CRUS ship)
/// still takes the manufacturer's own brand style record for its modular-kit
/// sheets: the first existing `s_<mfr>_<family>` / sibling / `s_<mfr>`.
#[test]
fn modular_style_falls_back_to_the_manufacturers_brand_record() {
    let root = serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.test_canvas",
        "_RecordValue_": {"style": null, "brandStyles": [
            {"brandIdentifier": "file://./x/styles/other.json", "entries": []}
        ]}
    });
    let with_record = |path: &str| -> Result<serde_json::Value, String> {
        if path == "s_test_hud" { Ok(serde_json::json!({})) } else { Err("none".into()) }
    };
    assert_eq!(
        crate::bb_resolve::modular_style_identifier_with_records(&root, Some("test"), Some("test"), &with_record).as_deref(),
        Some("s_test_hud")
    );
    let none = |_: &str| -> Result<serde_json::Value, String> { Err("none".into()) };
    assert_eq!(crate::bb_resolve::modular_style_identifier_with_records(&root, Some("test"), Some("test"), &none), None);
    // No known manufacturer (a socpak environment, rendered under the pipeline's default brand): no brand record.
    assert_eq!(crate::bb_resolve::modular_style_identifier_with_records(&root, Some("test"), None, &with_record), None);
    assert_eq!(crate::bb_resolve::modular_style_identifier(&root, Some("test")), None, "the canvas's own entries");
}

/// Minimal stand-in for `scrollbarhorizontalcomponentstandard`: an
/// inactive proxy-tagged `ComponentRoot` hosting the thumb, with the
/// authored `_SizeRatio` → `SizeX` and `_Show` → `scrollbar-show` →
/// `SecondaryStateTag` op chains and the `RootShow` embedded style.
fn scrollbar_template_fixture() -> serde_json::Value {
    serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.ScrollBarHorizontalComponentStandard",
        "_RecordId_": "00000000-0000-0000-0000-00000000sb01",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 1920.0, "y": 1080.0, "z": 0.0},
            "embeddedStyles": [
                {
                    "_Type_": "BuildingBlocks_StyleEntry",
                    "name": "RootShow",
                    "conditionsList": [
                        {"conditions": [
                            {"_Type_": "BuildingBlocks_StyleSelectorConditionTag",
                             "tag": {"_RecordId_": "e02a2290-9d6c-46f9-8235-4e828d804cee"}}
                        ]}
                    ],
                    "modifiers": [
                        {"_Type_": "BuildingBlocks_FieldModifierBoolean",
                         "field": "IsActive", "value": true}
                    ]
                }
            ],
            "scene": [
                {
                    "_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget",
                    "name": "ComponentRoot", "isActive": false,
                    "styleTags": [{"_RecordId_": "21788313-6aff-45b4-a3ad-fdc62b1cf849"}]
                },
                {
                    "_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget",
                    "name": "BarElementInstance", "parent": "_PointsTo_:ptr:1",
                    "styleTags": [{"_RecordId_": "9ca447ec-f238-4751-9f22-440db0c25cb2"}]
                }
            ],
            "operations": [
                {"_Type_": "BuildingBlocks_BindingsNumberField",
                 "widget": "_PointsTo_:ptr:2", "field": "SizeX", "input": "_PointsTo_:ptr:3"},
                {"_Pointer_": "ptr:3",
                 "_Type_": "BuildingBlocks_BindingsNumberComponentParameter",
                 "name": "_SizeRatio", "parameter": "ParamInput1", "defaultValue": 0.0},
                {"_Pointer_": "ptr:5",
                 "_Type_": "BuildingBlocks_BindingsBooleanComponentParameter",
                 "name": "_Show", "parameter": "ParamInput1", "defaultValue": false},
                {"_Pointer_": "ptr:6", "_Type_": "BuildingBlocks_BindingsTagFromBoolean",
                 "isTrue": {"_RecordId_": "e02a2290-9d6c-46f9-8235-4e828d804cee"},
                 "isFalse": null, "input": "_PointsTo_:ptr:5"},
                {"_Type_": "BuildingBlocks_BindingsStringField",
                 "widget": "_PointsTo_:ptr:1", "field": "SecondaryStateTag",
                 "input": "_PointsTo_:ptr:6"}
            ]
        }
    })
}

/// Host canvas mirroring the power screen's wiring: a clipping
/// `Scrollview` and a `ComponentScrollBar` targeting it.
fn scrollbar_host_scene() -> BbScene {
    let canvas = serde_json::json!({
        "_RecordName_": "test_scrollbar_host",
        "_RecordId_": "00000000-0000-0000-0000-00000000c002",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 1920.0, "y": 1080.0, "z": 0.0},
            "scene": [
                {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "root"},
                {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget",
                 "name": "Scrollview", "parent": "_PointsTo_:ptr:1",
                 "overflow": {"_Type_": "BuildingBlocks_Overflow", "overflow": "Clip"}},
                {"_Pointer_": "ptr:4", "_Type_": "BuildingBlocks_ComponentScrollBar",
                 "name": "ListScroll", "parent": "_PointsTo_:ptr:1",
                 "direction": "Horizontal", "target": "_PointsTo_:ptr:2"}
            ],
            "operations": []
        }
    });
    parse_bb_canvas(&canvas).expect("host canvas should parse")
}

fn scrollbar_fetcher(path: &str) -> Result<serde_json::Value, String> {
    if path == tag_database_path() {
        Ok(tag_database_fixture())
    } else if path.contains("scrollbarhorizontalcomponentstandard") {
        Ok(scrollbar_template_fixture())
    } else {
        Err(format!("test fetcher: no record for '{path}'"))
    }
}

/// Expansion must pair the template's thumb with the host's `target`
/// scroll view and surface the template's embedded `RootShow` entry.
#[test]
fn scrollbar_expansion_pairs_thumb_with_target_view() {
    let mut scene = scrollbar_host_scene();
    let entries = expand_widget_standards(&mut scene, &scrollbar_fetcher);

    let thumb = scene
        .nodes
        .values()
        .find(|n| n.raw.get("_ScrollThumbPair_").is_some())
        .expect("expansion should annotate the template thumb");
    assert_eq!(
        thumb.raw.get("_ScrollThumbAxis_").and_then(|v| v.as_str()),
        Some("x"),
        "a SizeX-bound thumb scrolls along x"
    );
    let pair = thumb.raw.get("_ScrollThumbPair_").cloned();
    let view = scene
        .nodes
        .values()
        .find(|n| n.name == "Scrollview")
        .expect("scroll view should survive expansion");
    assert_eq!(
        view.raw.get("_ScrollViewPair_").cloned(),
        pair,
        "the host's target view must carry the thumb's pair key"
    );
    assert!(
        entries
            .iter()
            .any(|e| e.get("name").and_then(|v| v.as_str()) == Some("RootShow")),
        "the scrollbar template's embedded RootShow entry must be surfaced"
    );
}

/// The full at-rest visibility chain: `_Show=true` synth param →
/// `scrollbar-show` SecondaryStateTag → embedded `RootShow` style →
/// `ComponentRoot.IsActive`.
#[test]
fn scrollbar_show_chain_activates_component_root() {
    let mut scene = scrollbar_host_scene();
    let entries = expand_widget_standards(&mut scene, &scrollbar_fetcher);
    crate::bb_bindings::resolve_state_tags_into_scene(&mut scene, &Default::default());
    let empty_raw = serde_json::json!({});
    let brand = bb_brand_style::BrandStyle {
        identifier: "widget-standard-embedded".to_string(),
        entries: entries.as_slice(),
        raw: &empty_raw,
    };
    crate::bb_brand_apply::apply_brand_modifiers(&mut scene, &brand, None);

    let root = scene
        .nodes
        .values()
        .find(|n| n.name == "ComponentRoot")
        .expect("expansion should merge the template root");
    assert!(
        root.is_active,
        "_Show=true must activate the scrollbar root through RootShow"
    );
}

/// A host TYPE added AFTER today's set (WidgetIcon / general button /
/// scrollbar) must NOT steal the primary expansion band from an existing
/// host's already-frozen instance IDs. Characterizes the medical close-X
/// theft (handoff 130-132 / runbook 395-402): before this fix a new host
/// type sorting at a lower node id expanded first and consumed
/// 0xF000_0000, shifting every downstream expansion ID. Latent-stability
/// only — no production host type reaches the second band today.
#[test]
fn new_host_type_expansion_does_not_shift_existing_low_band_ids() {
    // One-node child scene standing in for a merged template instance.
    let child = |name: &str| {
        parse_bb_canvas(&serde_json::json!({
            "_RecordValue_": {
                "_Type_": "BuildingBlocks_Canvas",
                "size": {"x": 1.0, "y": 1.0},
                "scene": [
                    {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": name}
                ]
            }
        }))
        .expect("child canvas should parse")
    };
    let parent = || {
        parse_bb_canvas(&serde_json::json!({
            "_RecordValue_": {
                "_Type_": "BuildingBlocks_Canvas",
                "size": {"x": 100.0, "y": 100.0},
                "scene": [
                    {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "root"}
                ]
            }
        }))
        .expect("parent canvas should parse")
    };
    let merged_id = |scene: &BbScene, name: &str| -> BbNodeId {
        scene.nodes.values().find(|n| n.name == name).expect("merged node present").id
    };

    // Baseline: an existing host type expands into the primary band.
    let mut base = parent();
    merge_child_scene(&mut base, child("existing"), "", None, Some(EXPANSION_ID_BASE));
    let existing_solo = merged_id(&base, "existing");
    assert_eq!(
        existing_solo, EXPANSION_ID_BASE,
        "an existing host type's instance takes the primary band base"
    );

    // Theft scenario: a NEW host type merges FIRST (sorts earlier), then the
    // existing type. The new type must land in the reserved SECOND band and
    // leave the existing type's primary-band id untouched.
    let mut with_new = parent();
    merge_child_scene(&mut with_new, child("new_type"), "", None, Some(EXPANSION_ID_BASE_SECOND));
    merge_child_scene(&mut with_new, child("existing"), "", None, Some(EXPANSION_ID_BASE));
    assert!(
        merged_id(&with_new, "new_type") >= EXPANSION_ID_BASE_SECOND,
        "the new host type lands in the reserved second band (0xF800_0000)"
    );
    assert_eq!(
        merged_id(&with_new, "existing"), existing_solo,
        "existing host type's instance id is invariant to the new type (no theft)"
    );
}

/// Review fix: an ordinary canvas merge keeps each expanded instance in its own band. A large button (second band)
/// expanded inside an embedded canvas stays in the second band when that canvas merges into its parent, so it cannot
/// take primary-band ids from the earlier host types' frozen instances.
#[test]
fn canvas_merge_keeps_second_band_instances_in_the_second_band() {
    let one = |name: &str| {
        parse_bb_canvas(&serde_json::json!({"_RecordValue_": {"_Type_": "BuildingBlocks_Canvas", "size": {"x": 1.0, "y": 1.0},
            "scene": [{"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": name}]}}))
        .expect("canvas parses")
    };
    // The embedded canvas: one primary-band and one second-band expansion instance.
    let mut embedded = one("embedded_root");
    merge_child_scene(&mut embedded, one("primary_instance"), "", None, Some(EXPANSION_ID_BASE));
    merge_child_scene(&mut embedded, one("second_instance"), "", None, Some(EXPANSION_ID_BASE_SECOND));
    let mut parent = one("parent_root");
    merge_child_scene(&mut parent, embedded, "", None, None);
    let id = |name: &str| parent.nodes.values().find(|n| n.name == name).expect("merged").id;
    assert!(id("second_instance") >= EXPANSION_ID_BASE_SECOND, "second band kept: {:#x}", id("second_instance"));
    let primary = id("primary_instance");
    assert!((EXPANSION_ID_BASE..EXPANSION_ID_BASE_SECOND).contains(&primary), "primary band kept: {primary:#x}");
}
