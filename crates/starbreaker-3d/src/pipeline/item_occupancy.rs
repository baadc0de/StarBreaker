//! Item footprint of an exported root entity: the inventory occupancy, cargo
//! units, mass and item identity a consumer needs to place it in a container
//! (`scene.json` `root_entity` fields).
//!
//! [`root_item_fields`] reads them from the entity class record:
//! `SAttachableComponentParams.AttachDef` (`SItemDefinition`: occupancy
//! dimensions / volume / local bounds, `Type`, `SubType`, `Size`, localised
//! `Name`), `SEntityPhysicsControllerParams.PhysType.Mass`,
//! `StaticEntityClassData[CargoGridOccupantProperties]` (per-face stacking
//! rules) and, when the item is itself a container, its inventory container.
//! [`item_fields_json`] is the pure part, over already-queried values.

use std::collections::HashMap;

use starbreaker_datacore::database::Database;
use starbreaker_datacore::query::value::Value;
use starbreaker_datacore::types::Record;

use super::interior_entities::{field, vec3};
use super::inventory_containers::{ContainerResolver, value_json};

fn query<'a>(db: &'a Database, record: &Record, path: &str) -> Option<Value<'a>> {
    let compiled = db.compile_path::<Value>(record.struct_id(), path).ok()?;
    db.query_single::<Value>(&compiled, record).ok().flatten()
}

fn text<'v>(value: Option<&'v Value>) -> Option<&'v str> {
    match value? {
        Value::String(s) | Value::Enum(s) | Value::Locale(s) => Some(s),
        _ => None,
    }
}

/// The single numeric field of a cargo-unit object (`SStandardCargoUnit`
/// `standardCargoUnits`, `SMicroCargoUnit` `microSCU`, …), as authored.
fn unit_amount(volume: &Value) -> Option<serde_json::Value> {
    let Value::Object { fields, .. } = volume else { return None };
    match fields.as_slice() {
        [(_, amount)] => Some(value_json(amount)).filter(serde_json::Value::is_number),
        _ => None,
    }
}

/// `root_entity` fields of an item from its `AttachDef`, physics `PhysType`
/// and cargo-grid occupant properties. Empty unless `attach_def` authors
/// inventory occupancy.
pub(crate) fn item_fields_json(
    attach_def: Option<&Value>,
    phys_type: Option<&Value>,
    occupant: Option<&Value>,
    display_name: Option<&str>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    let Some(attach_def) = attach_def else { return out };
    let dimensions = field(attach_def, "inventoryOccupancyDimensions");
    let volume = field(attach_def, "inventoryOccupancyVolume");
    if dimensions.is_none() && volume.is_none() {
        return out;
    }
    let vec = |name| field(attach_def, name).and_then(vec3);
    out.insert(
        "inventory_occupancy".into(),
        serde_json::json!({
            "dimensions": dimensions.and_then(vec3),
            "volume": volume.and_then(unit_amount),
            "volume_unit": volume.and_then(|v| match v {
                Value::Object { type_name, .. } => Some(*type_name),
                _ => None,
            }),
            "local_bounds_min": vec("inventoryOccupancyLocalBoundsMin"),
            "local_bounds_max": vec("inventoryOccupancyLocalBoundsMax"),
        }),
    );
    out.insert(
        "standard_cargo_units".into(),
        serde_json::json!(volume.and_then(|v| field(v, "standardCargoUnits")).map(value_json)),
    );
    out.insert("mass".into(), serde_json::json!(phys_type.and_then(|p| field(p, "Mass")).map(value_json)));
    out.insert("item_type".into(), serde_json::json!(text(field(attach_def, "Type"))));
    out.insert("item_sub_type".into(), serde_json::json!(text(field(attach_def, "SubType"))));
    out.insert("item_size".into(), serde_json::json!(field(attach_def, "Size").map(value_json)));
    let name_key = field(attach_def, "Localization").and_then(|l| text(field(l, "Name")));
    out.insert("display_name_key".into(), serde_json::json!(name_key));
    out.insert("display_name".into(), serde_json::json!(display_name));
    out.insert("cargo_grid_occupant".into(), serde_json::json!(occupant.map(value_json)));
    out
}

/// The item footprint fields of `record` for `root_entity` (see the module
/// doc); `localization` is the parsed `global.ini` (lower-case keys).
pub(crate) fn root_item_fields(
    db: &Database,
    record: &Record,
    localization: &HashMap<String, String>,
) -> serde_json::Map<String, serde_json::Value> {
    let attach_def = query(db, record, "Components[SAttachableComponentParams].AttachDef");
    let phys_type = query(db, record, "Components[SEntityPhysicsControllerParams].PhysType");
    let occupant = query(db, record, "StaticEntityClassData[CargoGridOccupantProperties]");
    let display_name = attach_def
        .as_ref()
        .and_then(|a| field(a, "Localization"))
        .and_then(|l| field(l, "Name"))
        .and_then(super::palette::localization_key_from_value)
        .and_then(|key| localization.get(&key).cloned());
    let mut out = item_fields_json(attach_def.as_ref(), phys_type.as_ref(), occupant.as_ref(), display_name.as_deref());
    if let Some(container) = ContainerResolver::new(db).container(record) {
        out.insert("inventory_container".into(), container);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(type_name: &'static str, fields: Vec<(&'static str, Value<'static>)>) -> Value<'static> {
        Value::Object { type_name, fields, record_id: None }
    }

    fn vec3_value(x: f32, y: f32, z: f32) -> Value<'static> {
        object("Vec3", vec![("x", Value::Float(x)), ("y", Value::Float(y)), ("z", Value::Float(z))])
    }

    /// Synthetic `SItemDefinition`: arbitrary values, not game data.
    fn attach_def(volume: Value<'static>) -> Value<'static> {
        object(
            "SItemDefinition",
            vec![
                ("Type", Value::Enum("SyntheticType")),
                ("SubType", Value::Enum("SyntheticSubType")),
                ("Size", Value::Int32(3)),
                ("Localization", object("SCItemLocalization", vec![("Name", Value::Locale("@synthetic_name"))])),
                ("inventoryOccupancyDimensions", vec3_value(1.0, 2.0, 0.5)),
                ("inventoryOccupancyLocalBoundsMin", vec3_value(-0.5, -1.0, 0.0)),
                ("inventoryOccupancyLocalBoundsMax", vec3_value(0.5, 1.0, 0.5)),
                ("inventoryOccupancyVolume", volume),
            ],
        )
    }

    #[test]
    fn standard_cargo_unit_item_exports_its_footprint() {
        let def = attach_def(object("SStandardCargoUnit", vec![("standardCargoUnits", Value::Float(3.0))]));
        let phys = object("SyntheticRigidParams", vec![("Mass", Value::Float(42.0))]);
        let occupant = object("CargoGridOccupantProperties", vec![("Top", object("CargoGridOccupantFace", vec![]))]);
        let json = serde_json::Value::Object(item_fields_json(Some(&def), Some(&phys), Some(&occupant), Some("Synthetic")));
        assert_eq!(json["inventory_occupancy"]["dimensions"], serde_json::json!([1.0, 2.0, 0.5]));
        assert_eq!(json["inventory_occupancy"]["volume"], serde_json::json!(3.0));
        assert_eq!(json["inventory_occupancy"]["volume_unit"], "SStandardCargoUnit");
        assert_eq!(json["inventory_occupancy"]["local_bounds_min"], serde_json::json!([-0.5, -1.0, 0.0]));
        assert_eq!(json["standard_cargo_units"], serde_json::json!(3.0));
        assert_eq!(json["mass"], serde_json::json!(42.0));
        assert_eq!(json["item_type"], "SyntheticType");
        assert_eq!(json["item_sub_type"], "SyntheticSubType");
        assert_eq!(json["item_size"], 3);
        assert_eq!(json["display_name_key"], "@synthetic_name");
        assert_eq!(json["display_name"], "Synthetic");
        assert_eq!(json["cargo_grid_occupant"]["__type"], "CargoGridOccupantProperties");
    }

    #[test]
    fn other_volume_units_are_kept_as_authored_without_scu() {
        let def = attach_def(object("SyntheticMicroUnit", vec![("syntheticMicro", Value::Int32(250))]));
        let json = serde_json::Value::Object(item_fields_json(Some(&def), None, None, None));
        assert_eq!(json["inventory_occupancy"]["volume"], 250);
        assert_eq!(json["inventory_occupancy"]["volume_unit"], "SyntheticMicroUnit");
        assert_eq!(json["standard_cargo_units"], serde_json::Value::Null);
        assert_eq!(json["mass"], serde_json::Value::Null);
        assert_eq!(json["display_name"], serde_json::Value::Null);
    }

    #[test]
    fn an_item_without_occupancy_adds_nothing() {
        let def = object("SItemDefinition", vec![("Type", Value::Enum("SyntheticType"))]);
        assert!(item_fields_json(Some(&def), None, None, None).is_empty());
        assert!(item_fields_json(None, None, None, None).is_empty());
    }
}
