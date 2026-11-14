//! Unit tests for `inventory_containers.rs`: container objects built from
//! synthetic `InventoryContainer` values (open and closed types), the verbatim
//! value encoding, and the vehicle / interior manifest entries.

use std::str::FromStr;

use super::*;

fn object(type_name: &'static str, fields: Vec<(&'static str, Value<'static>)>) -> Value<'static> {
    Value::Object { type_name, fields, record_id: None }
}

fn vec3_value(x: f32, y: f32, z: f32) -> Value<'static> {
    object("Vec3", vec![("x", Value::Float(x)), ("y", Value::Float(y)), ("z", Value::Float(z))])
}

/// A synthetic open (grid) container: arbitrary values, not game data.
fn open_container() -> Value<'static> {
    object(
        "InventoryContainer",
        vec![
            ("excludedItemSubTypes", Value::Array(vec![Value::Enum("SyntheticSubType")])),
            ("interiorDimensions", vec3_value(3.0, 6.0, 1.5)),
            (
                "inventoryType",
                object(
                    "InventoryOpenContainerType",
                    vec![
                        ("gridCellSize", object("SyntheticGridCellSizeCentimeters", vec![("centimeters", Value::Int32(5))])),
                        ("gridPosOffset", vec3_value(0.0, 0.0, 0.0)),
                        ("isExternalContainer", Value::Bool(true)),
                        ("maxPermittedItemSize", vec3_value(3.0, 6.0, 1.5)),
                        ("minPermittedItemSize", vec3_value(0.5, 0.5, 0.5)),
                        ("overrideStorableItemTypes", Value::Array(Vec::new())),
                        ("syntheticExtra", Value::Float(0.75)),
                    ],
                ),
            ),
        ],
    )
}

#[test]
fn open_container_normalises_grid_fields_and_keeps_the_record() {
    let json = container_json(Some(("Synthetic_Grid", "guid-1")), &open_container());
    assert_eq!(json["record"], "Synthetic_Grid");
    assert_eq!(json["record_guid"], "guid-1");
    assert_eq!(json["interior_dimensions"], serde_json::json!([3.0, 6.0, 1.5]));
    assert_eq!(json["inventory_type"], "InventoryOpenContainerType");
    assert_eq!(json["grid_cell_size"], serde_json::json!(0.05));
    assert_eq!(json["grid_cell_size_authored"]["__type"], "SyntheticGridCellSizeCentimeters");
    assert_eq!(json["grid_cell_size_authored"]["centimeters"], 5);
    assert_eq!(json["grid_pos_offset"], serde_json::json!([0.0, 0.0, 0.0]), "zeros stay zeros");
    assert_eq!(json["min_permitted_item_size"], serde_json::json!([0.5, 0.5, 0.5]));
    assert_eq!(json["max_permitted_item_size"], serde_json::json!([3.0, 6.0, 1.5]));
    assert_eq!(json["is_external_container"], true);
    assert_eq!(json["capacity"], serde_json::Value::Null);
    assert_eq!(json["excluded_item_sub_types"], serde_json::json!(["SyntheticSubType"]));
    assert_eq!(json["override_storable_item_types"], serde_json::json!([]));
    // Fields without a normalised name survive verbatim.
    assert_eq!(json["authored"]["inventoryType"]["syntheticExtra"], serde_json::json!(0.75));
}

#[test]
fn closed_container_keeps_its_capacity_verbatim_and_has_no_grid() {
    let params = object(
        "InventoryContainer",
        vec![
            ("excludedItemSubTypes", Value::Array(Vec::new())),
            ("interiorDimensions", vec3_value(0.25, 0.5, 0.25)),
            (
                "inventoryType",
                object(
                    "InventoryClosedContainerType",
                    vec![("capacity", object("SyntheticCargoUnit", vec![("syntheticUnits", Value::Int64(1234))]))],
                ),
            ),
        ],
    );
    let json = container_json(None, &params);
    assert_eq!(json["record"], serde_json::Value::Null, "an inline container names no record");
    assert_eq!(json["inventory_type"], "InventoryClosedContainerType");
    assert_eq!(json["capacity"]["__type"], "SyntheticCargoUnit");
    assert_eq!(json["capacity"]["syntheticUnits"], 1234);
    for absent in ["grid_cell_size", "grid_pos_offset", "min_permitted_item_size", "max_permitted_item_size", "is_external_container"] {
        assert_eq!(json[absent], serde_json::Value::Null, "{absent}");
    }
}

#[test]
fn an_unknown_cell_size_unit_is_kept_authored_only() {
    let cell = object("SyntheticCellSizeInches", vec![("inches", Value::Int32(2))]);
    assert_eq!(grid_cell_size_m(&cell), None);
    let meters = object("SyntheticCellSizeMeters", vec![("meters", Value::Float(0.5))]);
    assert_eq!(grid_cell_size_m(&meters), Some(0.5));
}

#[test]
fn value_json_tags_objects_and_references() {
    let id = CigGuid::from_str("aaaa0000-0000-0000-0000-000000000001").unwrap();
    let value = Value::Object {
        type_name: "Synthetic",
        fields: vec![("flag", Value::Bool(false)), ("list", Value::Array(vec![Value::UInt8(3)]))],
        record_id: Some(id),
    };
    let json = value_json(&value);
    assert_eq!(json["__type"], "Synthetic");
    assert_eq!(json["__record_id"], "aaaa0000-0000-0000-0000-000000000001");
    assert_eq!(json["flag"], false);
    assert_eq!(json["list"], serde_json::json!([3]));
}

#[test]
fn vehicle_entry_carries_port_paths_attachment_and_transform() {
    let entry = VehicleInventoryContainer {
        entity_name: "Synthetic_Grid".into(),
        entity_class_guid: "class-guid".into(),
        port_path: Some(vec!["port_a".into(), "port_grid".into()]),
        parent_port_path: Some(vec!["port_a".into()]),
        parent_entity_name: "Synthetic_Plate".into(),
        parent_node_name: "helper_grid".into(),
        no_rotation: false,
        offset_position: [0.0; 3],
        offset_rotation: [0.0; 3],
        port_flags: String::new(),
        container: container_json(None, &open_container()),
    };
    let transform = glam::Mat4::from_translation(glam::Vec3::new(1.0, 2.0, 3.0)).to_cols_array_2d();
    let json = entry.to_json(transform, false);
    assert_eq!(json["port_path"], serde_json::json!(["port_a", "port_grid"]));
    assert_eq!(json["parent_port_path"], serde_json::json!(["port_a"]));
    assert_eq!(json["parent_node_name"], "helper_grid");
    assert_eq!(json["source_transform_basis"], "gltf_y_up");
    assert_eq!(json["local_transform_sc"][3], serde_json::json!([1.0, 2.0, 3.0, 1.0]));
    assert_eq!(json["container"]["interior_dimensions"], serde_json::json!([3.0, 6.0, 1.5]));
}

#[test]
fn interior_entry_carries_identity_transform_and_links() {
    let entry = InteriorInventoryContainer {
        entity_id: Some("grid-id".into()),
        entity_class: "Synthetic_Grid".into(),
        entity_class_guid: "class-guid".into(),
        transform: glam::Mat4::from_translation(glam::Vec3::new(4.0, 5.0, 6.0)).to_cols_array_2d(),
        links: vec!["platform-id".into()],
        container: container_json(None, &open_container()),
    };
    let json = entry.to_json();
    assert_eq!(json["entity_id"], "grid-id");
    assert_eq!(json["entity_class"], "Synthetic_Grid");
    assert_eq!(json["links"], serde_json::json!(["platform-id"]));
    assert_eq!(json["transform"][3], serde_json::json!([4.0, 5.0, 6.0, 1.0]));
    assert_eq!(json["container"]["inventory_type"], "InventoryOpenContainerType");
}
