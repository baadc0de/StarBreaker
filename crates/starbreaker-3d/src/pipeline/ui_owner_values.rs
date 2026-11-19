//! The values a UI screen's OWNER entity pushes into its canvas (scviewer WP-I1 I1-5): its user variables and its
//! interaction state machine.
//!
//! - [`owner_ui_values`]: the class record's `UserVariablesComponentParams` (every array, by authored order) and the
//!   `SInteractionStateMachineParams` state types bound by state (`bindingsMethod = States`), read from the record's
//!   JSON ([`owner_ui_values_from_components`] is the pure part).
//! - [`apply_user_variable_overrides`]: a `.soc` placement's `EntityComponentUserVariables` instance values, matched
//!   to the class arrays by `__index` (a C2 door pad turns `ui_HideLockControls` … on).
//! - [`attach_owner_ui_values`]: both onto a [`UiBinding`].
//!
//! Nothing here consults an entity or variable name: the canvas binds the variables by name, and the exporter passes
//! what the data authors.

use starbreaker_datacore::database::Database;
use starbreaker_datacore::types::Record;

use crate::types::{UiBinding, UiStateType, UiUserValue, UiUserVariable, UserVariableOverride};

/// The `UserVariablesComponentParams` arrays, with the value kind of each.
const USER_VARIABLE_ARRAYS: [&str; 5] = ["bools", "ints", "floats", "strings", "locStrings"];

fn user_value(array: &str, value: &serde_json::Value) -> Option<UiUserValue> {
    Some(match array {
        "bools" => UiUserValue::Bool(value.as_bool().or_else(|| value.as_i64().map(|v| v != 0))?),
        "ints" => UiUserValue::Int(value.as_i64().or_else(|| value.as_f64().map(|v| v as i64))?),
        "floats" => UiUserValue::Float(value.as_f64()?),
        "strings" => UiUserValue::String(value.as_str()?.to_string()),
        "locStrings" => UiUserValue::Loc(value.as_str()?.to_string()),
        _ => return None,
    })
}

/// An instance override's authored text, read as the array's value kind.
fn user_value_from_text(array: &str, text: &str) -> Option<UiUserValue> {
    Some(match array {
        "bools" => UiUserValue::Bool(match text.trim() {
            "1" => true,
            "0" => false,
            other => other.parse().ok()?,
        }),
        "ints" => UiUserValue::Int(text.trim().parse().ok()?),
        "floats" => UiUserValue::Float(text.trim().parse().ok()?),
        "strings" => UiUserValue::String(text.to_string()),
        "locStrings" => UiUserValue::Loc(text.to_string()),
        _ => return None,
    })
}

/// User variables (each array in [`USER_VARIABLE_ARRAYS`] order, entries in authored order) and the state types bound
/// by state, from an entity record's `Components` JSON array.
pub(crate) fn owner_ui_values_from_components(components: &[serde_json::Value]) -> (Vec<UiUserVariable>, Vec<UiStateType>) {
    let mut variables = Vec::new();
    let mut state_types = Vec::new();
    for component in components {
        match component.get("_Type_").and_then(|v| v.as_str()) {
            Some("UserVariablesComponentParams") => {
                for array in USER_VARIABLE_ARRAYS {
                    for entry in component.get(array).and_then(|v| v.as_array()).into_iter().flatten() {
                        let (Some(name), Some(value)) = (
                            entry.get("name").and_then(|v| v.as_str()).filter(|n| !n.is_empty()),
                            entry.get("defaultValue").and_then(|v| user_value(array, v)),
                        ) else {
                            continue;
                        };
                        variables.push(UiUserVariable { name: name.to_string(), value });
                    }
                }
            }
            Some("SInteractionStateMachineParams") => {
                for state_type in component.get("StateTypes").and_then(|v| v.as_array()).into_iter().flatten() {
                    if state_type.get("bindingsMethod").and_then(|v| v.as_str()) != Some("States") {
                        continue;
                    }
                    let Some(name) = state_type.get("StateTypeName").and_then(|v| v.as_str()).filter(|n| !n.is_empty()) else {
                        continue;
                    };
                    let states: Vec<&serde_json::Value> =
                        state_type.get("States").and_then(|v| v.as_array()).into_iter().flatten().collect();
                    let state_names: Vec<String> = states
                        .iter()
                        .filter_map(|s| s.get("StateName").and_then(|v| v.as_str()).filter(|n| !n.is_empty()))
                        .map(str::to_string)
                        .collect();
                    if state_names.is_empty() {
                        continue;
                    }
                    // `DefaultState` points at one of the states (`_PointsTo_:ptr:N`); without one, the first state.
                    let default_ptr = state_type
                        .get("DefaultState")
                        .and_then(|v| v.as_str())
                        .and_then(|p| p.strip_prefix("_PointsTo_:"));
                    let default_state = default_ptr
                        .and_then(|ptr| {
                            states.iter().find(|s| s.get("_Pointer_").and_then(|v| v.as_str()) == Some(ptr))
                        })
                        .and_then(|s| s.get("StateName").and_then(|v| v.as_str()))
                        .map(str::to_string)
                        .unwrap_or_else(|| state_names[0].clone());
                    state_types.push(UiStateType { name: name.to_string(), states: state_names, default_state });
                }
            }
            _ => {}
        }
    }
    (variables, state_types)
}

/// [`owner_ui_values_from_components`] of the entity `record`.
pub(crate) fn owner_ui_values(db: &Database, record: &Record) -> (Vec<UiUserVariable>, Vec<UiStateType>) {
    let Ok(bytes) = starbreaker_datacore::export::to_json_compact(db, record) else {
        return (Vec::new(), Vec::new());
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return (Vec::new(), Vec::new());
    };
    let components = json
        .get("_RecordValue_")
        .and_then(|v| v.get("Components"))
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    owner_ui_values_from_components(components)
}

/// Apply a placement's instance overrides: override `__index` N of array A replaces the class's N-th variable of A.
/// An override whose index the class does not have, or whose text does not parse as the array's kind, is skipped.
pub(crate) fn apply_user_variable_overrides(
    variables: &mut [UiUserVariable],
    class_arrays: &[(String, usize)],
    overrides: &[UserVariableOverride],
) {
    for o in overrides {
        // The variable's position in `variables`: the variables before its array, plus its index in the array.
        let mut start = 0;
        let mut found = None;
        for (array, len) in class_arrays {
            if *array == o.array {
                found = (o.index < *len).then_some(start + o.index);
                break;
            }
            start += len;
        }
        let (Some(position), Some(value)) = (found, user_value_from_text(&o.array, &o.value)) else {
            log::debug!("user-variable override {}[{}] = {:?} does not apply", o.array, o.index, o.value);
            continue;
        };
        if let Some(variable) = variables.get_mut(position) {
            variable.value = value;
        }
    }
}

/// The class's array lengths, in [`USER_VARIABLE_ARRAYS`] order (what [`apply_user_variable_overrides`] indexes).
pub(crate) fn user_variable_array_lengths(variables: &[UiUserVariable]) -> Vec<(String, usize)> {
    USER_VARIABLE_ARRAYS
        .iter()
        .map(|array| {
            let len = variables
                .iter()
                .filter(|v| {
                    matches!(
                        (*array, &v.value),
                        ("bools", UiUserValue::Bool(_))
                            | ("ints", UiUserValue::Int(_))
                            | ("floats", UiUserValue::Float(_))
                            | ("strings", UiUserValue::String(_))
                            | ("locStrings", UiUserValue::Loc(_))
                    )
                })
                .count();
            (array.to_string(), len)
        })
        .collect()
}

/// Whether `binding` is the UI of a panel the player operates, from data only (the SC_UE_ModelViewer consumer's rule,
/// contract "Consumer"): a `physical` binding of a resolved owner that is not a `Display` item, or is one with
/// `KioskInteractionEntityComponentParams` (door, light and transit panels, kiosks). Its render carries the owner's
/// values and, for an interior placement, the face's texture aspect.
pub(crate) fn is_operated_panel(binding: &UiBinding) -> bool {
    binding.binding_kind == "physical"
        && binding.source_owner_resolved
        && (binding.source_item_type.as_deref() != Some("Display") || binding.source_kiosk_interaction)
}

/// Set `binding`'s owner values from `record`, with `overrides` (a placement's instance values) applied.
pub(crate) fn attach_owner_ui_values(
    db: &Database,
    record: &Record,
    overrides: &[UserVariableOverride],
    binding: &mut UiBinding,
) {
    let (mut variables, state_types) = owner_ui_values(db, record);
    let lengths = user_variable_array_lengths(&variables);
    apply_user_variable_overrides(&mut variables, &lengths, overrides);
    binding.user_variables = variables;
    binding.state_types = state_types;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Synthetic record components (structure of `UserVariablesComponentParams` / `SInteractionStateMachineParams`;
    /// no game values).
    fn components() -> Vec<serde_json::Value> {
        vec![
            json!({"_Type_": "UserVariablesComponentParams",
                   "floats": [{"name": "f0", "defaultValue": 0.5}],
                   "ints": [{"name": "i0", "defaultValue": 1}],
                   "bools": [{"name": "b0", "defaultValue": false}, {"name": "b1", "defaultValue": false},
                             {"name": "b2", "defaultValue": true}],
                   "strings": [{"name": "s0", "defaultValue": "x"}],
                   "locStrings": [{"name": "l0", "defaultValue": "@test_empty"}]}),
            json!({"_Type_": "SInteractionStateMachineParams", "StateTypes": [
                {"StateTypeName": "TypeA", "bindingsMethod": "States", "DefaultState": null,
                 "States": [{"_Pointer_": "ptr:1", "StateName": "A0"}, {"_Pointer_": "ptr:2", "StateName": "A1"}]},
                {"StateTypeName": "TypeB", "bindingsMethod": "States", "DefaultState": "_PointsTo_:ptr:4",
                 "States": [{"_Pointer_": "ptr:3", "StateName": "B0"}, {"_Pointer_": "ptr:4", "StateName": "B1"}]},
                {"StateTypeName": "TypeC", "bindingsMethod": "None",
                 "States": [{"_Pointer_": "ptr:5", "StateName": "C0"}]}
            ]}),
        ]
    }

    #[test]
    fn owner_values_read_every_user_variable_array_and_the_state_bound_types() {
        let (variables, types) = owner_ui_values_from_components(&components());
        let names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["b0", "b1", "b2", "i0", "f0", "s0", "l0"]);
        assert_eq!(variables[2].value, UiUserValue::Bool(true));
        assert_eq!(variables[6].value, UiUserValue::Loc("@test_empty".into()));
        assert_eq!(types.len(), 2, "a type not bound by state is skipped");
        assert_eq!(types[0].default_state, "A0", "no DefaultState: the first state");
        assert_eq!(types[1].default_state, "B1", "DefaultState names its state");
        assert_eq!(types[1].states, ["B0", "B1"]);
    }

    /// I1-5 root cause 4: a placement's `.soc` instance values replace the class defaults by array and `__index` (the
    /// C2 door pads set bools 0, 2, 4 and 5).
    #[test]
    fn instance_overrides_replace_class_values_by_array_and_index() {
        let (mut variables, _) = owner_ui_values_from_components(&components());
        let lengths = user_variable_array_lengths(&variables);
        let o = |array: &str, index: usize, value: &str| UserVariableOverride {
            array: array.into(),
            index,
            value: value.into(),
        };
        apply_user_variable_overrides(
            &mut variables,
            &lengths,
            &[o("bools", 0, "1"), o("bools", 2, "0"), o("ints", 0, "7"), o("bools", 9, "1"), o("floats", 0, "oops")],
        );
        assert_eq!(variables[0].value, UiUserValue::Bool(true));
        assert_eq!(variables[1].value, UiUserValue::Bool(false), "untouched");
        assert_eq!(variables[2].value, UiUserValue::Bool(false));
        assert_eq!(variables[3].value, UiUserValue::Int(7));
        assert_eq!(variables[4].value, UiUserValue::Float(0.5), "an unparsable override is skipped");
    }

    /// Owner values and texture aspects are for operated panels only: a resolved owner that is not a display item, or a
    /// display the player operates; never a display only shown (a wall HUD, an MFD) or an unresolved owner.
    #[test]
    fn owner_values_are_for_operated_panels() {
        let b = |kind: &str, item: Option<&str>, resolved: bool, kiosk: bool| UiBinding {
            binding_kind: kind.into(),
            source_item_type: item.map(str::to_string),
            source_owner_resolved: resolved,
            source_kiosk_interaction: kiosk,
            ..Default::default()
        };
        assert!(is_operated_panel(&b("physical", Some("ControlPanel"), true, false)), "a light panel without a kiosk");
        assert!(is_operated_panel(&b("physical", None, true, true)), "a transit panel");
        assert!(is_operated_panel(&b("physical", Some("Display"), true, true)), "a kiosk");
        assert!(!is_operated_panel(&b("physical", Some("Display"), true, false)), "a wall HUD");
        assert!(!is_operated_panel(&b("mfd", Some("ControlPanel"), true, true)));
        assert!(!is_operated_panel(&b("physical", None, false, false)), "an unresolved owner");
    }
}
