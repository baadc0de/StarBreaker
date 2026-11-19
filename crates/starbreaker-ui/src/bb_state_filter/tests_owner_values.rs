//! State-filter tests for a binding whose owner's user variables are known (WP-I1 fix pass): the exporter passes the
//! owner entity's complete `UserVariablesComponentParams` set, so a bare-named variable the owner does not declare
//! reads at its type's default, and `BooleanFromLocalizationIsEmpty` evaluates the localized variable's value. Door
//! and light panels rely on both: the room-name header (`!IsEmpty(ui_LocationName)`) and the settings gear
//! (`ui_showSettingsButton`, declared by no door pad) are hidden at rest. Synthetic ids and names.

use serde_json::json;

use super::instantiated_false_widgets_with_param_inputs_inherited_bindings_and_defaults as false_widgets;
use crate::canvas::Value;
use crate::defaults::DefaultValueRegistry;

fn header_canvas() -> serde_json::Value {
    json!({
        "_Type_": "BuildingBlocks_Canvas",
        "operations": [
            {"_Pointer_": "ptr:5", "_Type_": "BuildingBlocks_BindingsLocalizedVariable", "binding": "test_room_name"},
            {"_Pointer_": "ptr:6", "_Type_": "BindingsOperation_BooleanFromLocalizationIsEmpty", "input": "_PointsTo_:ptr:5"},
            {"_Pointer_": "ptr:7", "_Type_": "BuildingBlocks_BindingsBooleanInvert", "input": "_PointsTo_:ptr:6"},
            {"_Type_": "BuildingBlocks_BindingsBooleanField", "widget": "_PointsTo_:ptr:4", "field": "IsActive", "input": "_PointsTo_:ptr:7"},
            {"_Pointer_": "ptr:9", "_Type_": "BuildingBlocks_BindingsBooleanVariable", "binding": "test_show_gear"},
            {"_Type_": "BuildingBlocks_BindingsBooleanField", "widget": "_PointsTo_:ptr:8", "field": "IsActive", "input": "_PointsTo_:ptr:9"},
            {"_Pointer_": "ptr:10", "_Type_": "BuildingBlocks_BindingsBooleanEvaluateOr", "inputs": ["_PointsTo_:ptr:9", "_PointsTo_:ptr:7"]},
            {"_Type_": "BuildingBlocks_BindingsBooleanField", "widget": "_PointsTo_:ptr:1", "field": "IsActive", "input": "_PointsTo_:ptr:10"}
        ]
    })
}

#[test]
fn owner_values_hide_an_empty_room_name_and_an_undeclared_toggle() {
    let mut defaults = DefaultValueRegistry::default();
    defaults.insert_path("test_room_name", Value::Str(String::new()));
    defaults.set_owner_user_variables_known(true);
    let hidden = false_widgets(&header_canvas(), &[], &Default::default(), Some(&defaults));
    for widget in [1, 4, 8] {
        assert!(hidden.contains(&widget), "widget ptr:{widget} hidden at rest: {hidden:?}");
    }
}

#[test]
fn a_named_room_shows_the_header() {
    let mut defaults = DefaultValueRegistry::default();
    defaults.insert_path("test_room_name", Value::Str("Room".into()));
    defaults.set_owner_user_variables_known(true);
    let hidden = false_widgets(&header_canvas(), &[], &Default::default(), Some(&defaults));
    assert!(!hidden.contains(&4) && !hidden.contains(&1), "a named room shows its header: {hidden:?}");
    assert!(hidden.contains(&8), "the undeclared gear stays hidden: {hidden:?}");
}

#[test]
fn without_owner_values_an_unset_variable_keeps_the_conservative_show() {
    let hidden = false_widgets(&header_canvas(), &[], &Default::default(), Some(&DefaultValueRegistry::default()));
    assert!(!hidden.contains(&1) && !hidden.contains(&8), "runtime-gated UI stays shown without owner values: {hidden:?}");
}
