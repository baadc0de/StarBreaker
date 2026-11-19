//! Tests for the widget standards' style plumbing (WP-I1 fix pass 2): a style-set
//! component parameter (`ParamInputN`) routed into the expanded template, the
//! template's own runtime styles (matching brand, else `defaultStyles`) scoped to
//! its instance, and the modular-kit sheets scoped to their own standard.

use super::*;
use crate::bb_resolve::*;
use crate::bb_scene::{BbScene, parse_bb_canvas};

const ICON_WIDGET_INSTANCE_TAG: &str = "91fd1afb-05f3-4736-bfbe-704b9e16fef1";

fn tag_database() -> serde_json::Value {
    serde_json::json!({"tags": [
        {"tagName": "canvas-proxy-root", "_RecordId_": "21788313-6aff-45b4-a3ad-fdc62b1cf849"},
        {"tagName": "general-button-secondary", "_RecordId_": "cf236683-1f17-41ee-a0d2-0e23c5f1c552"},
        {"tagName": "icon-widget-instance", "_RecordId_": ICON_WIDGET_INSTANCE_TAG}
    ]})
}

/// A stand-in for `buttongeneralsecondarycomponentstandard`: the `ShowIcon` boolean
/// parameter on slot `ParamInput1` drives the icon instance's `IsActive`; the
/// `LabelStyle` integer parameter shares the slot number. Its `defaultStyles`
/// pad the icon instance by 7; its brand container for the test manufacturer
/// (`s_test_env`, when present) by 9.
fn secondary_template(with_brand: bool) -> serde_json::Value {
    let brand = if with_brand { "s_test_env" } else { "s_other_env" };
    let brand_padding = if with_brand { 9.0 } else { 11.0 };
    serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.ButtonGeneralSecondaryComponentStandard",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 64.0, "y": 64.0, "z": 0.0},
            "scene": [
                {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "ComponentRoot",
                 "styleTags": [{"_RecordId_": "21788313-6aff-45b4-a3ad-fdc62b1cf849"}]},
                {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_WidgetIcon", "name": "IconWidgetInstance",
                 "parent": "_PointsTo_:ptr:1", "styleTags": [{"_RecordId_": ICON_WIDGET_INSTANCE_TAG}],
                 "iconProperties": {"_Type_": "BuildingBlocks_ComponentIconProperties", "show": true, "iconPreset": "_None", "customIcon": ""}}
            ],
            "operations": [
                {"_Pointer_": "ptr:9", "_Type_": "BuildingBlocks_BindingsBooleanComponentParameter",
                 "name": "ShowIcon", "parameter": "ParamInput1", "defaultValue": false},
                {"_Type_": "BuildingBlocks_BindingsBooleanField", "widget": "_PointsTo_:ptr:2", "field": "IsActive", "input": "_PointsTo_:ptr:9"},
                {"_Pointer_": "ptr:16", "_Type_": "BuildingBlocks_BindingsIntegerComponentParameter",
                 "name": "LabelStyle", "parameter": "ParamInput1", "defaultValue": 0}
            ],
            "defaultStyles": {"sharedStyles": null, "entries": [padding_entry(7.0)]},
            "brandStyles": [{"brandIdentifier": format!("file://./x/styles/{brand}.json"), "entries": [padding_entry(brand_padding)]}]
        }
    })
}

fn padding_entry(value: f64) -> serde_json::Value {
    serde_json::json!({
        "_Type_": "BuildingBlocks_StyleEntry", "name": "RootIcon",
        "conditionsList": [{"conditions": [
            {"_Type_": "BuildingBlocks_StyleSelectorConditionTag", "tag": {"_RecordId_": ICON_WIDGET_INSTANCE_TAG}}
        ]}],
        "modifiers": [{"_Type_": "BuildingBlocks_FieldModifierNumber", "field": "PaddingTop", "value": value}]
    })
}

/// Host canvas: one secondary button authored with its icon hidden (the door
/// shelf's light buttons: `iconProperties.show` false, the brand turns it on),
/// and a stray node carrying the icon-instance tag outside any button.
fn host_scene() -> BbScene {
    parse_bb_canvas(&serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.test_shelf",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 1920.0, "y": 1080.0, "z": 0.0},
            "scene": [
                {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "root"},
                {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_ComponentGeneralButtonSecondary", "name": "LightButton_Low",
                 "parent": "_PointsTo_:ptr:1", "fillStyle": "Filled", "iconPosition": "Top",
                 "iconProperties": {"_Type_": "BuildingBlocks_ComponentIconProperties", "show": false,
                    "iconPreset": "ArrowHollowRight", "customIcon": "UI/Textures/Vector/General/CommonIcons/icon_common_light_low.svg"},
                 "labelProperties": {"_Type_": "BuildingBlocks_ComponentLabelProperties", "show": false, "label": "@LOC_PLACEHOLDER"}},
                {"_Pointer_": "ptr:3", "_Type_": "BuildingBlocks_DisplayWidget", "name": "stray", "parent": "_PointsTo_:ptr:1",
                 "styleTags": [{"_RecordId_": ICON_WIDGET_INSTANCE_TAG}]}
            ],
            "operations": []
        }
    }))
    .expect("host canvas parses")
}

fn fetcher(with_brand: bool) -> impl Fn(&str) -> Result<serde_json::Value, String> {
    move |path: &str| {
        if path == tag_database_path() {
            Ok(tag_database())
        } else if path == button_secondary_component_standard_path() {
            Ok(secondary_template(with_brand))
        } else {
            Err(format!("test fetcher: no record for '{path}'"))
        }
    }
}

fn icon_instance(scene: &BbScene) -> &crate::bb_scene::BbNode {
    scene.nodes.values().find(|n| n.name == "IconWidgetInstance").expect("merged icon instance")
}

/// The CRUS brand block of `IC_DoorButtonShelf` sets the secondary buttons'
/// `ParamInput1` (a boolean) to true: the standard's `ShowIcon` parameter, so the
/// button's own icon shows (the authored `base_Icon` is hidden by the same block).
/// The integer parameter on the same slot is untouched.
#[test]
fn a_styled_boolean_slot_drives_the_templates_boolean_parameter() {
    let mut scene = host_scene();
    expand_widget_standards(&mut scene, &fetcher(false));
    let host_id = scene.nodes.values().find(|n| n.name == "LightButton_Low").map(|n| n.id).expect("host");
    let raw = scene.nodes.get_mut(&host_id).and_then(|n| n.raw.as_object_mut()).expect("host raw");
    raw.insert(crate::bb_brand_apply::STYLED_PARAMS_KEY.into(), serde_json::json!({"Boolean:ParamInput1": true}));

    assert_eq!(route_styled_component_params(&mut scene), vec![host_id]);
    finalize_widget_standard_fields(&mut scene, &crate::defaults::DefaultValueRegistry::default());
    assert!(icon_instance(&scene).is_active, "ShowIcon follows the styled slot");
    let int_param = scene.operations.iter().find(|op| {
        op.get("_Type_").and_then(|v| v.as_str()) == Some("_SynthIntegerParam_")
            && op.get("parameter").and_then(|v| v.as_str()) == Some("ParamInput1")
    });
    assert!(int_param.is_none_or(|op| op.get("resolvedInt") != Some(&serde_json::json!(1))), "integer slot untouched");
}

/// Without a styled slot the host's authored `show: false` stands.
#[test]
fn an_unstyled_slot_keeps_the_authored_parameter() {
    let mut scene = host_scene();
    expand_widget_standards(&mut scene, &fetcher(false));
    assert!(route_styled_component_params(&mut scene).is_empty());
    finalize_widget_standard_fields(&mut scene, &crate::defaults::DefaultValueRegistry::default());
    assert!(!icon_instance(&scene).is_active);
}

/// A standard's runtime look is its brand container for the manufacturer, else its
/// `defaultStyles` (the canvas rule), applied to its own instance only.
#[test]
fn standard_runtime_styles_use_the_default_styles_without_a_matching_brand() {
    let mut scene = host_scene();
    let fetch = fetcher(false);
    expand_widget_standards(&mut scene, &fetch);
    apply_standard_runtime_styles(&mut scene, None, None, Some("test"), crate::bb_brand_style::BrandClass::Env, &fetch, None);
    assert_eq!(icon_instance(&scene).padding.top, 7.0, "defaultStyles entry");
    let stray = scene.nodes.values().find(|n| n.name == "stray").expect("stray");
    assert_eq!(stray.padding.top, 0.0, "nodes outside the standard's instance are not styled");
}

#[test]
fn standard_runtime_styles_prefer_the_manufacturers_brand_container() {
    let mut scene = host_scene();
    let fetch = fetcher(true);
    expand_widget_standards(&mut scene, &fetch);
    apply_standard_runtime_styles(&mut scene, None, None, Some("test"), crate::bb_brand_style::BrandClass::Env, &fetch, None);
    assert_eq!(icon_instance(&scene).padding.top, 9.0, "brand entry");
}

/// Each standard names its own modular-kit sheet (`defaultStyles.sharedStyles`:
/// the secondary standard's `*_buttonsecondarystyles`, the primary's
/// `*_buttonstyles`): the brand's PRIMARY sheet (`sk_crus_env_buttonprimarystyles`
/// `RootIconElementInstance` PivotY 0.5) must not restyle a secondary button.
#[test]
fn the_primary_kit_sheet_leaves_secondary_buttons_alone() {
    let mut scene = host_scene();
    expand_widget_standards(&mut scene, &fetcher(false));
    let sheet = serde_json::json!({"_RecordValue_": {"entries": [padding_entry(13.0)]}});
    let fetch = |path: &str| -> Result<serde_json::Value, String> {
        if path.ends_with("sk_test/sk_test_buttonprimarystyles.json") {
            Ok(sheet.clone())
        } else {
            Err(format!("no record for '{path}'"))
        }
    };
    apply_modular_kit_sheets(&mut scene, "s_test", &fetch, None);
    assert_eq!(icon_instance(&scene).padding.top, 0.0, "primary sheet scoped to primary buttons");
}

/// The standard's base sheet (`defaultStyles.sharedStyles`, its `_default` modular-kit
/// sheet) applies under the template's own entries: the large button's
/// `modularkit_default_buttonlargestyles` centres the label (the hangar call panel,
/// which has no brand, drew it at the top edge).
#[test]
fn standard_runtime_styles_start_from_the_standards_shared_sheet() {
    let mut scene = host_scene();
    let shared_path = "file://./x/styles/modularkitstyles/_default/test_buttonsecondarystyles.json";
    let mut template = secondary_template(false);
    template["_RecordValue_"]["defaultStyles"]["sharedStyles"] = serde_json::json!(shared_path);
    template["_RecordValue_"]["defaultStyles"]["entries"] = serde_json::json!([]);
    let shared = serde_json::json!({"_RecordValue_": {"entries": [padding_entry(5.0)]}});
    let fetch = move |path: &str| -> Result<serde_json::Value, String> {
        if path == tag_database_path() {
            Ok(tag_database())
        } else if path == button_secondary_component_standard_path() {
            Ok(template.clone())
        } else if path == shared_path {
            Ok(shared.clone())
        } else {
            Err(format!("no record for '{path}'"))
        }
    };
    expand_widget_standards(&mut scene, &fetch);
    apply_standard_runtime_styles(&mut scene, None, None, Some("test"), crate::bb_brand_style::BrandClass::Env, &fetch, None);
    assert_eq!(icon_instance(&scene).padding.top, 5.0, "shared sheet entry");
}

/// A child canvas's styles reach a standard instance the child does not expand
/// itself: the door shelf (`IC_DoorButtonShelf`, a child canvas) scales the secondary
/// buttons' glyph custom shape 2x ("Override Icon Shelf Button Size", `Type(CustomShape)`
/// inside a `general-button-secondary`), but the glyph lives in the button's nested
/// icon instance, which the PARENT level expands; the entry used to miss it.
#[test]
fn child_canvas_styles_reach_instances_the_parent_level_expands() {
    let shelf_url = "file://./test_shelf.json";
    let icon_template = serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.IconWidgetStandard",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 64.0, "y": 64.0, "z": 0.0},
            "scene": [{"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_WidgetCustomShape", "name": "IconInstance"}],
            "operations": []
        }
    });
    let shelf = serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.test_shelf",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 1920.0, "y": 1080.0, "z": 0.0},
            "embeddedStyles": [{
                "_Type_": "BuildingBlocks_StyleEntry", "name": "Override Icon Shelf Button Size",
                "conditionsList": [{"conditions": [
                    {"_Type_": "BuildingBlocks_StyleSelectorConditionType", "type": "CustomShape"},
                    {"_Type_": "BuildingBlocks_StyleSelectorConditionAncestor", "conditions": [], "breakConditions": [
                        {"_Type_": "BuildingBlocks_StyleSelectorConditionTag", "tag": {"_RecordId_": "cf236683-1f17-41ee-a0d2-0e23c5f1c552"}}
                    ]}
                ]}],
                "modifiers": [{"_Type_": "BuildingBlocks_FieldModifierNumber", "field": "ScaleX", "value": 2.0}]
            }],
            "scene": host_scene_nodes(),
            "operations": []
        }
    });
    let root = serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.test_door",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 1920.0, "y": 1080.0, "z": 0.0},
            "scene": [
                {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "door_root"},
                {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_WidgetCanvas", "name": "Canvas_Shelf", "parent": "_PointsTo_:ptr:1", "canvas": shelf_url}
            ],
            "operations": []
        }
    });
    let mut template = secondary_template(false);
    template["_RecordValue_"]["defaultStyles"]["entries"] = serde_json::json!([]);
    // The button's icon instance shows (as the brand's `ShowIcon` makes it in the shelf).
    template["_RecordValue_"]["operations"][0]["defaultValue"] = serde_json::json!(true);
    let scene = crate::bb_resolve::resolve_canvas_graph(&root, None, &|path| {
        if path == shelf_url {
            Ok(shelf.clone())
        } else if path == tag_database_path() {
            Ok(tag_database())
        } else if path == button_secondary_component_standard_path() {
            Ok(template.clone())
        } else if path.ends_with("iconwidgetstandard.json") {
            Ok(icon_template.clone())
        } else {
            Err(format!("no record for '{path}'"))
        }
    })
    .expect("resolved scene");
    let glyph = scene.nodes.values().find(|n| n.name == "IconInstance").expect("nested icon instance");
    assert_eq!(glyph.raw.get("scale").and_then(|s| s.get("x")).and_then(|v| v.as_f64()), Some(2.0), "the shelf's entry scales the glyph");
}

fn host_scene_nodes() -> serde_json::Value {
    serde_json::json!([
        {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "ButtonContainer"},
        {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_ComponentGeneralButtonSecondary", "name": "LightButton_Low",
         "parent": "_PointsTo_:ptr:1", "fillStyle": "Filled", "iconPosition": "Top",
         "iconProperties": {"_Type_": "BuildingBlocks_ComponentIconProperties", "show": true,
            "iconPreset": "ArrowHollowRight", "customIcon": "UI/Textures/Vector/General/CommonIcons/icon_common_light_low.svg"},
         "labelProperties": {"_Type_": "BuildingBlocks_ComponentLabelProperties", "show": false, "label": "@LOC_PLACEHOLDER"}}
    ])
}

/// Each synthetic parameter takes only its own modifier family: a Number style on a
/// slot does not round into the Integer parameter sharing it (the secondary
/// standard's Number `Icon Scale` and Integer `IconPosition` are both `ParamInput4`),
/// and an Integer style reaches the Integer parameter. A declared parameter the
/// expansion did not inject gets its synthetic.
#[test]
fn styled_slots_route_by_exact_family_and_create_declared_parameters() {
    let mut scene = host_scene();
    expand_widget_standards(&mut scene, &fetcher(false));
    let host_id = scene.nodes.values().find(|n| n.name == "LightButton_Low").map(|n| n.id).expect("host");
    let raw = scene.nodes.get_mut(&host_id).and_then(|n| n.raw.as_object_mut()).expect("host raw");
    raw.insert(crate::bb_brand_apply::STYLED_PARAMS_KEY.into(), serde_json::json!({"Number:ParamInput1": 3.6}));
    assert!(route_styled_component_params(&mut scene).is_empty(), "no Number parameter on ParamInput1");
    let raw = scene.nodes.get_mut(&host_id).and_then(|n| n.raw.as_object_mut()).expect("host raw");
    raw.insert(crate::bb_brand_apply::STYLED_PARAMS_KEY.into(), serde_json::json!({"Integer:ParamInput1": 2.0}));
    assert_eq!(route_styled_component_params(&mut scene), vec![host_id]);
    let int_param = scene
        .operations
        .iter()
        .find(|op| op.get("_Type_").and_then(|v| v.as_str()) == Some("_SynthIntegerParam_"))
        .expect("a synthetic for the declared LabelStyle parameter");
    assert_eq!(int_param.get("resolvedInt"), Some(&serde_json::json!(2)));
    let label_style = scene
        .operations
        .iter()
        .find(|op| op.get("name").and_then(|v| v.as_str()) == Some("LabelStyle"))
        .expect("the declared parameter");
    assert_eq!(int_param.get("_Pointer_"), label_style.get("_Pointer_"), "the synthetic stands for the declared parameter");
}

/// A host expanded by a child canvas keeps the look that child's styles gave it: the
/// parent level applies the standards' runtime styles only to the instances it
/// expands itself (a parent re-application of the template's padding 7 over the
/// shelf's own padding 20 was review finding 1).
#[test]
fn a_parent_level_does_not_reapply_standard_styles_to_a_childs_instances() {
    let shelf_url = "file://./test_shelf.json";
    let shelf = serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.test_shelf",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 1920.0, "y": 1080.0, "z": 0.0},
            "embeddedStyles": [padding_entry(20.0)],
            "scene": host_scene_nodes(),
            "operations": []
        }
    });
    let root = serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.test_door",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 1920.0, "y": 1080.0, "z": 0.0},
            "scene": [
                {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "door_root"},
                {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_WidgetCanvas", "name": "Canvas_Shelf", "parent": "_PointsTo_:ptr:1", "canvas": shelf_url}
            ],
            "operations": []
        }
    });
    let template = secondary_template(false);
    let scene = crate::bb_resolve::resolve_canvas_graph(&root, None, &|path| {
        if path == shelf_url {
            Ok(shelf.clone())
        } else if path == tag_database_path() {
            Ok(tag_database())
        } else if path == button_secondary_component_standard_path() {
            Ok(template.clone())
        } else {
            Err(format!("no record for '{path}'"))
        }
    })
    .expect("resolved scene");
    assert_eq!(icon_instance(&scene).padding.top, 20.0, "the shelf's own entry stands");
}

/// A styled parameter's state tag is in place before the selectors that test it run:
/// the canvas sets `ShowIcon` (boolean `ParamInput1`) through its styles, and the
/// standard's embedded entry gated on `show-icon` must see it.
#[test]
fn a_styled_parameter_drives_its_state_tag_before_the_cascade() {
    let mut template = secondary_template(false);
    let ops = template["_RecordValue_"]["operations"].as_array_mut().expect("ops");
    ops.push(serde_json::json!({"_Pointer_": "ptr:15", "_Type_": "BuildingBlocks_BindingsTagFromBoolean",
        "isTrue": {"_RecordId_": "614ba513-7b87-4546-98a1-95e29eca75ec"},
        "isFalse": {"_RecordId_": "a8477652-49e1-4d42-9480-63d71b75078b"}, "input": "_PointsTo_:ptr:9"}));
    ops.push(serde_json::json!({"_Type_": "BuildingBlocks_BindingsStringField", "widget": "_PointsTo_:ptr:1",
        "field": "QuarternaryStateTag", "input": "_PointsTo_:ptr:15"}));
    template["_RecordValue_"]["defaultStyles"]["entries"] = serde_json::json!([]);
    template["_RecordValue_"]["embeddedStyles"] = serde_json::json!([{
        "_Type_": "BuildingBlocks_StyleEntry", "name": "ShownIcon",
        "conditionsList": [{"conditions": [
            {"_Type_": "BuildingBlocks_StyleSelectorConditionTag", "tag": {"_RecordId_": ICON_WIDGET_INSTANCE_TAG}},
            {"_Type_": "BuildingBlocks_StyleSelectorConditionAncestor", "conditions": [
                {"_Type_": "BuildingBlocks_StyleSelectorConditionTag", "tag": {"_RecordId_": "614ba513-7b87-4546-98a1-95e29eca75ec"}}
            ], "breakConditions": []}
        ]}],
        "modifiers": [{"_Type_": "BuildingBlocks_FieldModifierNumber", "field": "PaddingTop", "value": 3.0}]
    }]);
    let mut nodes = host_scene_nodes();
    nodes[1]["iconProperties"]["show"] = serde_json::json!(false);
    let root = serde_json::json!({
        "_RecordName_": "BuildingBlocks_Canvas.test_shelf",
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"_Type_": "Vec3", "x": 1920.0, "y": 1080.0, "z": 0.0},
            "defaultStyles": {"sharedStyles": null, "entries": [{
                "_Type_": "BuildingBlocks_StyleEntry", "name": "ShowSecondaryButtonIcon",
                "conditionsList": [{"conditions": [
                    {"_Type_": "BuildingBlocks_StyleSelectorConditionTag", "tag": {"_RecordId_": "cf236683-1f17-41ee-a0d2-0e23c5f1c552"}}
                ]}],
                "modifiers": [{"_Type_": "BuildingBlocks_FieldModifierBoolean", "field": "ParamInput1", "value": true}]
            }]},
            "scene": nodes,
            "operations": []
        }
    });
    let scene = crate::bb_resolve::resolve_canvas_graph(&root, None, &|path| {
        if path == tag_database_path() {
            Ok(tag_database())
        } else if path == button_secondary_component_standard_path() {
            Ok(template.clone())
        } else {
            Err(format!("no record for '{path}'"))
        }
    })
    .expect("resolved scene");
    let icon = icon_instance(&scene);
    assert!(icon.is_active, "the styled ShowIcon shows the icon");
    assert_eq!(icon.padding.top, 3.0, "the show-icon entry saw the styled state");
}
