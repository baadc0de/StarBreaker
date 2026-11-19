//! What an interior entity's class says about how it is operated (WP-P5),
//! resolved from DataCore once per class and cached:
//!
//! - [`ClassFacts`]: door timing (`SCItemDoorParams`), interaction points and
//!   shared interactions (`SEntityInteractableParams`), the elevator carriage
//!   (`SCItemElevatorParams`, its physics grid) and path-node
//!   (`SCItemElevatorPathNodeParams`) components, the transit gateway / limbo
//!   node roles, and the class geometry.
//! - [`InteractionResolver`]: the cache; resolves `DisplayName` keys through
//!   `global.ini` (loaded on first use) and an entity's instance overrides
//!   ([`InteractionResolver::door`], [`InteractionResolver::traversal_names`]).
//!
//! Every fact is recognised by its DataCore component, never by a class or
//! entity name.

use std::collections::HashMap;
use std::str::FromStr;

use starbreaker_common::CigGuid;
use starbreaker_datacore::database::Database;
use starbreaker_datacore::query::value::Value;
use starbreaker_datacore::types::Record;

use super::interior_entities::{field, number};
use crate::types::{DoorTiming, EntityOverrides, InteractionPoint, PointInteraction, TraversalLink};

/// `SCItemElevatorParams` of a carriage class.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct ElevatorParams {
    pub max_speed: Option<f32>,
    pub easing_distance: Option<f32>,
    pub slow_down_at_bends: Option<bool>,
    pub open_wait_time: Option<f32>,
    pub queue_wait_time: Option<f32>,
    pub idling_period: Option<f32>,
    pub open_inner_door_interaction: Option<String>,
    pub open_inner_door_finished_interaction: Option<String>,
    pub close_all_doors_interaction: Option<String>,
    pub close_inner_door_finished_interaction: Option<String>,
}

/// `SCItemElevatorPathNodeParams` of a path-node class.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PathNodeClass {
    /// `Type`, lower case: `stop`, `waypoint`, `teleport`.
    pub kind: String,
    pub teleport_time_scale: Option<f32>,
    pub gateway_timing_in_transit: Option<f32>,
    pub gateway_interaction: Option<String>,
    /// The interaction names the node's own slots carry (`doorOpenInteraction` …).
    pub interactions: Vec<(String, String)>,
}

/// The facts of one entity class.
#[derive(Debug, Clone, Default)]
pub(crate) struct ClassFacts {
    pub record_name: String,
    pub door: Option<DoorTiming>,
    pub interaction_points: Vec<InteractionPoint>,
    /// `SharedInteractions[].Name`, by index (traversal links name them by index).
    pub shared_interactions: Vec<String>,
    pub elevator: Option<ElevatorParams>,
    pub path_node: Option<PathNodeClass>,
    /// `SEntityPhysicsControllerParams.PhysType.grid.gridGeometry.path`.
    pub grid_geometry: Option<String>,
    /// `transit_gateway` (`TransitGatewayParams`) or `transit_limbo` (`TransitLimboNodeParams`).
    pub transit_role: Option<&'static str>,
    /// `SGeometryResourceParams…Geometry.path`.
    pub geometry_path: Option<String>,
}

fn text<'a>(value: &'a Value<'a>) -> Option<&'a str> {
    match value {
        Value::String(s) | Value::Locale(s) | Value::Enum(s) => Some(s),
        _ => None,
    }
}

fn boolean(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

/// The `Name` of a (followed) interaction pointer.
fn interaction_name(value: Option<&Value>) -> Option<String> {
    value.and_then(|v| field(v, "Name")).and_then(text).filter(|s| !s.is_empty()).map(str::to_string)
}

fn query<'a>(db: &'a Database, record: &Record, path: &str) -> Option<Value<'a>> {
    let compiled = db.compile_path::<Value>(record.struct_id(), path).ok()?;
    db.query_single::<Value>(&compiled, record).ok().flatten()
}

pub(crate) fn door_timing(component: &Value) -> DoorTiming {
    DoorTiming {
        animation_speed: field(component, "DoorAnimationSpeed").and_then(number),
        use_auto_close_delay: field(component, "UseAutoCloseDelay").and_then(boolean),
        auto_close_delay: field(component, "AutoCloseDelay").and_then(number),
    }
}

pub(crate) fn elevator_params(component: &Value) -> ElevatorParams {
    ElevatorParams {
        max_speed: field(component, "MaxSpeed").and_then(number),
        easing_distance: field(component, "EasingDistance").and_then(number),
        slow_down_at_bends: field(component, "SlowDownAtBends").and_then(boolean),
        open_wait_time: field(component, "OpenWaitTime").and_then(number),
        queue_wait_time: field(component, "QueueWaitTime").and_then(number),
        idling_period: field(component, "IdlingPeriod").and_then(number),
        open_inner_door_interaction: interaction_name(field(component, "OpenInnerDoorInteraction")),
        open_inner_door_finished_interaction: interaction_name(field(component, "OpenInnerDoorFinishedInteraction")),
        close_all_doors_interaction: interaction_name(field(component, "CloseAllDoorsInteraction")),
        close_inner_door_finished_interaction: interaction_name(field(component, "CloseInnerDoorFinishedInteraction")),
    }
}

pub(crate) fn path_node_class(component: &Value) -> PathNodeClass {
    let gateway = field(component, "gatewayParams").filter(|g| !matches!(g, Value::Null));
    let interactions = ["doorOpenInteraction", "doorCloseInteraction", "ascentRequestInteraction", "descentRequestInteraction", "cancelStopInteraction"]
        .iter()
        .filter_map(|slot| interaction_name(field(component, slot)).map(|name| (slot.to_string(), name)))
        .collect();
    PathNodeClass {
        kind: field(component, "Type").and_then(text).unwrap_or("Stop").to_ascii_lowercase(),
        teleport_time_scale: field(component, "TeleportTimeScale").and_then(number),
        gateway_timing_in_transit: gateway.and_then(|g| field(g, "timingInTransit")).and_then(number),
        gateway_interaction: gateway.and_then(|g| interaction_name(field(g, "interaction"))),
        interactions,
    }
}

/// Interaction points of an `SInteractableParams` value; display names stay keys here.
pub(crate) fn interaction_points(interactable: &Value) -> Vec<InteractionPoint> {
    let Some(Value::Array(points)) = field(interactable, "InteractionPoints") else {
        return Vec::new();
    };
    points
        .iter()
        .enumerate()
        .filter_map(|(index, point)| {
            let helper_name = field(point, "HelperName").and_then(text).unwrap_or_default().to_string();
            let Some(Value::Array(list)) = field(point, "Interactions") else { return None };
            let interactions: Vec<PointInteraction> = list
                .iter()
                .filter_map(|i| {
                    Some(PointInteraction {
                        name: interaction_name(field(i, "Interaction"))?,
                        display_name_key: field(i, "DisplayName").and_then(text).filter(|s| !s.is_empty()).map(str::to_string),
                        display_name: None,
                        primary: field(i, "IsPrimary").and_then(boolean).unwrap_or(false),
                    })
                })
                .collect();
            (!interactions.is_empty()).then(|| InteractionPoint {
                index,
                helper_name,
                radius: field(point, "Radius").and_then(number),
                helper_transform: None,
                interactions,
            })
        })
        .collect()
}

pub(crate) fn shared_interactions(interactable: &Value) -> Vec<String> {
    let Some(Value::Array(list)) = field(interactable, "SharedInteractions") else {
        return Vec::new();
    };
    list.iter().map(|i| interaction_name(Some(i)).unwrap_or_default()).collect()
}

fn class_facts(db: &Database, record: &Record) -> ClassFacts {
    let name = db.resolve_string2(record.name_offset);
    let mut facts = ClassFacts { record_name: name.rsplit('.').next().unwrap_or(name).to_string(), ..Default::default() };
    facts.door = query(db, record, "Components[SCItemDoorParams]").map(|c| door_timing(&c));
    if let Some(interactable) = query(db, record, "Components[SEntityInteractableParams].Interactable") {
        facts.interaction_points = interaction_points(&interactable);
        facts.shared_interactions = shared_interactions(&interactable);
    }
    facts.elevator = query(db, record, "Components[SCItemElevatorParams]").map(|c| elevator_params(&c));
    facts.path_node = query(db, record, "Components[SCItemElevatorPathNodeParams]").map(|c| path_node_class(&c));
    // PhysType is polymorphic (rigid, …): its `grid` is read from the materialised value.
    facts.grid_geometry = query(db, record, "Components[SEntityPhysicsControllerParams].PhysType")
        .as_ref()
        .and_then(|phys| field(phys, "grid"))
        .and_then(|grid| field(grid, "gridGeometry"))
        .and_then(|geometry| field(geometry, "path"))
        .and_then(text)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    facts.transit_role = if query(db, record, "Components[TransitGatewayParams]").is_some() {
        Some("transit_gateway")
    } else if query(db, record, "Components[TransitLimboNodeParams]").is_some() {
        Some("transit_limbo")
    } else {
        None
    };
    facts.geometry_path = query(db, record, "Components[SGeometryResourceParams].Geometry.Geometry.Geometry.path")
        .as_ref()
        .and_then(text)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    facts
}

/// Class facts by class GUID, and the English localization for display names.
pub(crate) struct InteractionResolver<'a> {
    db: &'a Database<'a>,
    p4k: Option<&'a starbreaker_p4k::MappedP4k>,
    by_class: HashMap<String, Option<ClassFacts>>,
    localization: Option<HashMap<String, String>>,
}

impl<'a> InteractionResolver<'a> {
    pub(crate) fn new(db: &'a Database<'a>, p4k: Option<&'a starbreaker_p4k::MappedP4k>) -> Self {
        Self { db, p4k, by_class: HashMap::new(), localization: None }
    }

    /// The facts of class `guid` (`None`: no such record).
    pub(crate) fn class(&mut self, guid: Option<&str>) -> Option<&ClassFacts> {
        let guid = guid?;
        let db = self.db;
        let key = guid.to_ascii_lowercase();
        if !self.by_class.contains_key(&key) {
            let facts = CigGuid::from_str(guid).ok().and_then(|g| db.record_by_id(&g)).map(|r| class_facts(db, r));
            let facts = facts.map(|mut f| {
                self.localize(&mut f.interaction_points);
                f
            });
            self.by_class.insert(key.clone(), facts);
        }
        self.by_class.get(&key).and_then(Option::as_ref)
    }

    fn localize(&mut self, points: &mut [InteractionPoint]) {
        if points.iter().all(|p| p.interactions.iter().all(|i| i.display_name_key.is_none())) {
            return;
        }
        let Some(p4k) = self.p4k else { return };
        let map = self.localization.get_or_insert_with(|| super::load_localization_map(p4k));
        for interaction in points.iter_mut().flat_map(|p| p.interactions.iter_mut()) {
            let key = interaction.display_name_key.as_deref().map(|k| Value::String(k));
            interaction.display_name = key
                .as_ref()
                .and_then(super::palette::localization_key_from_value)
                .and_then(|k| map.get(&k).cloned());
        }
    }

    /// Door timing of an entity: its class's `SCItemDoorParams` with the instance overrides.
    pub(crate) fn door(&mut self, guid: Option<&str>, overrides: &EntityOverrides) -> Option<DoorTiming> {
        let mut door = self.class(guid)?.door.clone()?;
        door.animation_speed = overrides.door_animation_speed.or(door.animation_speed);
        door.use_auto_close_delay = overrides.use_auto_close_delay.or(door.use_auto_close_delay);
        door.auto_close_delay = overrides.auto_close_delay.or(door.auto_close_delay);
        if door.animation_speed.is_none_or(|speed| !speed.is_finite() || speed <= 0.0) {
            log::warn!("[interior] door class {guid:?}: DoorAnimationSpeed {:?} is not a usable rate", door.animation_speed);
        }
        Some(door)
    }

    /// The class's interaction points of an entity with its instance overrides applied: a point the instance authors with
    /// no interactions offers none and is left out.
    pub(crate) fn points(&mut self, guid: Option<&str>, overrides: &EntityOverrides, entity: Option<&str>) -> Vec<InteractionPoint> {
        if !overrides.replaced_interaction_points.is_empty() {
            log::warn!(
                "[interior] entity {entity:?}: instance replaces the interactions of points {:?}; the class's are exported",
                overrides.replaced_interaction_points
            );
        }
        let mut points = self.class(guid).map(|f| f.interaction_points.clone()).unwrap_or_default();
        points.retain(|p| !overrides.cleared_interaction_points.contains(&p.index));
        points
    }

    /// Traversal links with their local interaction resolved to the class's shared interaction name.
    pub(crate) fn traversal_names(&mut self, guid: Option<&str>, links: &[TraversalLink]) -> Vec<TraversalLink> {
        let shared = self.class(guid).map(|f| f.shared_interactions.clone()).unwrap_or_default();
        links
            .iter()
            .map(|link| {
                let mut link = link.clone();
                link.local_interaction = shared_index(&link.local_interaction_path)
                    .and_then(|i| shared.get(i).cloned())
                    .filter(|s| !s.is_empty());
                link
            })
            .collect()
    }
}

/// `…SharedInteractions[5]` → 5.
pub(crate) fn shared_index(path: &str) -> Option<usize> {
    let rest = path.rsplit_once("SharedInteractions[")?.1;
    rest.split(']').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(type_name: &'static str, fields: Vec<(&'static str, Value<'static>)>) -> Value<'static> {
        Value::Object { type_name, fields, record_id: None }
    }

    fn shared(name: &'static str) -> Value<'static> {
        object("SSharedInteractionParams", vec![("Name", Value::String(name))])
    }

    #[test]
    fn elevator_params_read_the_six_values_and_four_interactions() {
        // CRUS_Hercules_Lift_Platform (LIVE 4.10.2).
        let component = object(
            "SCItemElevatorParams",
            vec![
                ("MaxSpeed", Value::Float(3.5)),
                ("EasingDistance", Value::Float(5.1)),
                ("IdlingPeriod", Value::Float(120.0)),
                ("OpenWaitTime", Value::Float(4.0)),
                ("QueueWaitTime", Value::Float(5.0)),
                ("SlowDownAtBends", Value::Bool(false)),
                ("OpenInnerDoorInteraction", shared("OpenInnerDoor")),
                ("OpenInnerDoorFinishedInteraction", shared("OpenInnerDoorFinished")),
                ("CloseAllDoorsInteraction", shared("CloseAllDoors")),
                ("CloseInnerDoorFinishedInteraction", shared("CloseInnerDoorFinished")),
            ],
        );
        let p = elevator_params(&component);
        assert_eq!(p.max_speed, Some(3.5));
        assert_eq!(p.easing_distance, Some(5.1));
        assert_eq!(p.open_wait_time, Some(4.0));
        assert_eq!(p.queue_wait_time, Some(5.0));
        assert_eq!(p.idling_period, Some(120.0));
        assert_eq!(p.slow_down_at_bends, Some(false));
        assert_eq!(p.close_all_doors_interaction.as_deref(), Some("CloseAllDoors"));
        assert_eq!(p.close_inner_door_finished_interaction.as_deref(), Some("CloseInnerDoorFinished"));
    }

    #[test]
    fn interaction_points_keep_their_interactions_in_order() {
        let interactable = object(
            "SInteractableParams",
            vec![(
                "InteractionPoints",
                Value::Array(vec![object(
                    "SInteractionPointParams",
                    vec![
                        ("HelperName", Value::String("$proxy_elevator_button")),
                        ("Radius", Value::Float(1.6)),
                        (
                            "Interactions",
                            Value::Array(vec![
                                object("SInteractionParams", vec![("Interaction", shared("RequestFloor1")), ("DisplayName", Value::Locale("@innerthought_lift_top_deck")), ("IsPrimary", Value::Bool(true))]),
                                object("SInteractionParams", vec![("Interaction", shared("RequestFloor0")), ("IsPrimary", Value::Bool(false))]),
                                object("SInteractionParams", vec![("Interaction", Value::Null)]),
                            ]),
                        ),
                    ],
                )]),
            )],
        );
        let points = interaction_points(&interactable);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].helper_name, "$proxy_elevator_button");
        assert_eq!(points[0].radius, Some(1.6));
        let names: Vec<_> = points[0].interactions.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["RequestFloor1", "RequestFloor0"], "an interaction without a target is not offered");
        assert!(points[0].interactions[0].primary);
        assert_eq!(points[0].interactions[0].display_name_key.as_deref(), Some("@innerthought_lift_top_deck"));
    }

    #[test]
    fn shared_index_reads_the_authored_path() {
        assert_eq!(shared_index("InteractableComponent.Interactable.SharedInteractions[5]"), Some(5));
        assert_eq!(shared_index("Interactable.InteractionPoints[1]"), None);
    }
}
