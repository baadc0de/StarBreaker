//! Inventory containers (cargo grids, item banks, lockers) resolved from
//! DataCore for the decomposed scene manifest.
//!
//! An entity is a container when its class has
//! `SCItemInventoryContainerComponentParams`; its `containerParams` names an
//! `InventoryContainer` record, written by [`container_json`] (normalised
//! fields plus the record verbatim). Containers are recognised by that
//! component only, never by class or port name.
//!
//! - [`ContainerResolver`]: per-class cache of the container a record carries.
//! - [`collect_vehicle_containers`] / [`VehicleInventoryContainer`]: every
//!   loadout item of a vehicle that is a container, geometry-less items
//!   included, with its port path and attachment (`scene.json`
//!   `inventory_containers[]`).
//! - [`InteriorInventoryContainer`]: a `.soc` entity that is a container
//!   (`interiors[].inventory_containers[]`).
//! - [`value_json`]: a DataCore `Value` as JSON, with `__type` on objects.

use std::collections::{HashMap, HashSet};

use starbreaker_common::CigGuid;
use starbreaker_datacore::database::Database;
use starbreaker_datacore::query::value::Value;
use starbreaker_datacore::types::Record;

use super::interior_entities::{class_record, field, vec3};
use super::loadout_attachments::{WalkOrigin, creates_scene_nodes, walk_loadout_attachments};
use crate::types::{InteriorMesh, ResolvedNode};

const CONTAINER_COMPONENT_PATH: &str = "Components[SCItemInventoryContainerComponentParams]";

/// A DataCore value as JSON. Objects carry their struct type in `__type`
/// (the `dcb query` convention); a materialised reference adds `__record_id`.
pub(crate) fn value_json(value: &Value) -> serde_json::Value {
    use serde_json::json;
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(v) => json!(v),
        Value::Int8(v) => json!(v),
        Value::Int16(v) => json!(v),
        Value::Int32(v) => json!(v),
        Value::Int64(v) => json!(v),
        Value::UInt8(v) => json!(v),
        Value::UInt16(v) => json!(v),
        Value::UInt32(v) => json!(v),
        Value::UInt64(v) => json!(v),
        Value::Float(v) => json!(v),
        Value::Double(v) => json!(v),
        Value::String(v) | Value::Enum(v) | Value::Locale(v) => json!(v),
        Value::Guid(v) => json!(v.to_string()),
        Value::Array(items) => serde_json::Value::Array(items.iter().map(value_json).collect()),
        Value::Object { type_name, fields, record_id } => {
            let mut map = serde_json::Map::new();
            map.insert("__type".into(), json!(type_name));
            if let Some(id) = record_id {
                map.insert("__record_id".into(), json!(id.to_string()));
            }
            for (name, field) in fields {
                map.insert((*name).to_string(), value_json(field));
            }
            serde_json::Value::Object(map)
        }
    }
}

fn object_type<'a>(value: &Value<'a>) -> Option<&'a str> {
    match value {
        Value::Object { type_name, .. } => Some(type_name),
        _ => None,
    }
}

fn scalar(value: &Value) -> Option<f64> {
    Some(match value {
        Value::Int8(v) => *v as f64,
        Value::Int16(v) => *v as f64,
        Value::Int32(v) => *v as f64,
        Value::Int64(v) => *v as f64,
        Value::UInt8(v) => *v as f64,
        Value::UInt16(v) => *v as f64,
        Value::UInt32(v) => *v as f64,
        Value::UInt64(v) => *v as f64,
        Value::Float(v) => *v as f64,
        Value::Double(v) => *v,
        _ => return None,
    })
}

/// A grid cell size object in metres, for the unit fields whose meaning is
/// their name (`centimeters`, `meters`); `None` for any other shape.
fn grid_cell_size_m(value: &Value) -> Option<f64> {
    let Value::Object { fields, .. } = value else { return None };
    match fields.as_slice() {
        [("centimeters", v)] => scalar(v).map(|cm| cm / 100.0),
        [("meters", v)] => scalar(v),
        _ => None,
    }
}

/// The `container` object of a resolved `InventoryContainer` (`params`):
/// normalised fields (null when the record or its `inventoryType` does not
/// author them; zeros stay zeros) plus `authored`, the record verbatim.
pub(crate) fn container_json(record: Option<(&str, &str)>, params: &Value) -> serde_json::Value {
    let inventory_type = field(params, "inventoryType");
    let type_field = |name| inventory_type.and_then(|t| field(t, name));
    let vec = |value: Option<&Value>| value.and_then(vec3);
    let cell = type_field("gridCellSize");
    serde_json::json!({
        "record": record.map(|(name, _)| name),
        "record_guid": record.map(|(_, guid)| guid),
        "interior_dimensions": vec(field(params, "interiorDimensions")),
        "inventory_type": inventory_type.and_then(object_type),
        "grid_cell_size": cell.and_then(grid_cell_size_m),
        "grid_cell_size_authored": cell.map(value_json),
        "grid_pos_offset": vec(type_field("gridPosOffset")),
        "min_permitted_item_size": vec(type_field("minPermittedItemSize")),
        "max_permitted_item_size": vec(type_field("maxPermittedItemSize")),
        "is_external_container": type_field("isExternalContainer").and_then(|v| match v {
            Value::Bool(b) => Some(*b),
            _ => None,
        }),
        "capacity": type_field("capacity").map(value_json),
        "override_storable_item_types": type_field("overrideStorableItemTypes").map(value_json),
        "excluded_item_sub_types": field(params, "excludedItemSubTypes").map(value_json),
        "authored": value_json(params),
    })
}

fn short_record_name<'a>(db: &'a Database, record: &Record) -> &'a str {
    let name = db.resolve_string2(record.name_offset);
    name.rsplit('.').next().unwrap_or(name)
}

/// Resolves the inventory container of entity class records, caching per class.
pub(crate) struct ContainerResolver<'a> {
    db: &'a Database<'a>,
    by_class: HashMap<CigGuid, Option<serde_json::Value>>,
}

impl<'a> ContainerResolver<'a> {
    pub(crate) fn new(db: &'a Database<'a>) -> Self {
        Self { db, by_class: HashMap::new() }
    }

    /// The `container` object of `record`'s inventory-container component.
    pub(crate) fn container(&mut self, record: &Record) -> Option<serde_json::Value> {
        let db = self.db;
        self.by_class
            .entry(record.id)
            .or_insert_with(|| {
                // The component is the query leaf: a reference *inside* a
                // materialised value keeps its record id (a leaf reference
                // would not).
                let compiled = db.compile_path::<Value>(record.struct_id(), CONTAINER_COMPONENT_PATH).ok()?;
                let component = db.query_single::<Value>(&compiled, record).ok().flatten()?;
                let params = field(&component, "containerParams")?;
                if matches!(params, Value::Null) {
                    return None;
                }
                let source = match params {
                    Value::Object { record_id: Some(id), .. } => {
                        db.record_by_id(id).map(|r| (short_record_name(db, r).to_string(), id.to_string()))
                    }
                    _ => None,
                };
                Some(container_json(source.as_ref().map(|(n, g)| (n.as_str(), g.as_str())), params))
            })
            .clone()
    }

    /// The container `.soc` entity `mesh` is, when its class has the component.
    pub(crate) fn interior(&mut self, mesh: &InteriorMesh) -> Option<InteriorInventoryContainer> {
        let record = class_record(self.db, mesh)?;
        let container = self.container(record)?;
        Some(InteriorInventoryContainer {
            entity_id: mesh.entity.entity_id.clone(),
            entity_class: short_record_name(self.db, record).to_string(),
            entity_class_guid: record.id.to_string(),
            transform: mesh.transform,
            links: mesh.entity.links.clone(),
            container,
        })
    }
}

/// A vehicle loadout item that is an inventory container.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VehicleInventoryContainer {
    pub entity_name: String,
    pub entity_class_guid: String,
    pub port_path: Option<Vec<String>>,
    /// Port path of the scene entity whose geometry holds `parent_node_name`
    /// (`[]`: the root entity; else a `children[]` entry's `port_path`).
    pub parent_port_path: Option<Vec<String>>,
    pub parent_entity_name: String,
    pub parent_node_name: String,
    pub no_rotation: bool,
    pub offset_position: [f32; 3],
    pub offset_rotation: [f32; 3],
    pub port_flags: String,
    pub container: serde_json::Value,
}

impl VehicleInventoryContainer {
    /// The `inventory_containers[]` entry, with the exporter-resolved local
    /// transform (the convention of `children[].local_transform_sc`).
    pub(crate) fn to_json(&self, local_transform_sc: [[f32; 4]; 4], resolved_no_rotation: bool) -> serde_json::Value {
        serde_json::json!({
            "entity_name": self.entity_name,
            "entity_class_guid": self.entity_class_guid,
            "port_path": self.port_path,
            "parent_port_path": self.parent_port_path,
            "parent_entity_name": self.parent_entity_name,
            "parent_node_name": self.parent_node_name,
            "source_transform_basis": "gltf_y_up",
            "local_transform_sc": local_transform_sc,
            "resolved_no_rotation": resolved_no_rotation,
            "no_rotation": self.no_rotation,
            "offset_position": self.offset_position,
            "offset_rotation": self.offset_rotation,
            "port_flags": self.port_flags,
            "container": self.container,
        })
    }
}

/// Every node of the root entity's loadout `children` whose class is an
/// inventory container, in loadout order, with its scene attachment.
/// `emitted` are the `port_path`s of the scene children actually exported: a
/// container's parent is the nearest of those (or the root), so it never names
/// a child whose payload failed to load.
pub(crate) fn collect_vehicle_containers(
    db: &Database,
    children: &[ResolvedNode],
    root_entity_name: &str,
    emitted: &HashSet<Vec<String>>,
) -> Vec<VehicleInventoryContainer> {
    let mut resolver = ContainerResolver::new(db);
    let mut out = Vec::new();
    let is_instance = |node: &ResolvedNode, port_path: Option<&[String]>| match port_path {
        Some(path) => emitted.contains(path),
        // Off-loadout nodes carry no port path to match: the pre-load rule.
        None => creates_scene_nodes(node, None),
    };
    let origin = WalkOrigin::root(root_entity_name);
    walk_loadout_attachments(children, origin, None, Some(&[]), &is_instance, &mut |attachment| {
        let node = attachment.node;
        let Some(container) = resolver.container(&node.record) else { return };
        out.push(VehicleInventoryContainer {
            entity_name: node.entity_name.clone(),
            entity_class_guid: node.record.id.to_string(),
            port_path: attachment.port_path,
            parent_port_path: attachment.parent_port_path,
            parent_entity_name: attachment.parent_entity_name,
            parent_node_name: attachment.parent_node_name,
            no_rotation: attachment.no_rotation,
            offset_position: node.offset_position,
            offset_rotation: node.offset_rotation,
            port_flags: node.port_flags.clone(),
            container,
        });
    });
    out
}

/// A `.soc` entity of an interior container that is an inventory container.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InteriorInventoryContainer {
    pub entity_id: Option<String>,
    pub entity_class: String,
    pub entity_class_guid: String,
    /// Container-local, the frame of the container's placements.
    pub transform: [[f32; 4]; 4],
    /// Ids the entity's `EntityLinks` target.
    pub links: Vec<String>,
    pub container: serde_json::Value,
}

impl InteriorInventoryContainer {
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "entity_id": self.entity_id,
            "entity_class": self.entity_class,
            "entity_class_guid": self.entity_class_guid,
            "transform": self.transform,
            "links": self.links,
            "container": self.container,
        })
    }
}

#[cfg(test)]
#[path = "inventory_containers_tests.rs"]
mod tests;
