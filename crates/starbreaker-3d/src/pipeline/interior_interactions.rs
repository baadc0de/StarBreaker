//! What an interior entity's class says about how it is operated (WP-P5),
//! resolved from DataCore once per class and cached:
//!
//! - [`ClassFacts`]: door timing and state (`SCItemDoorParams`), interaction points and
//!   shared interactions (`SEntityInteractableParams`; the sendable ones with their
//!   linking tags), vehicle item-port interaction links (`SInteractionLinkParams`),
//!   a light group's interactions (`SLightGroupComponentParams`, see
//!   `interaction_graph`), whether it has a state machine, the elevator carriage
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
use super::interaction_graph::LightGroupFacts;
use crate::types::{
    DoorTiming, EntityOverrides, InteractionPoint, ItemPortLink, PointInteraction, SendableInteraction, TraversalLink,
};

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
    /// `SInteractionLinkParams.vehicleItemPortInteractionLinks`, resolved.
    pub item_port_links: Vec<ItemPortLink>,
    /// Shared interactions with `Sendable`, with their linking tags.
    pub sendable_interactions: Vec<SendableInteraction>,
    /// `SLightGroupComponentParams`: the class is a light group.
    pub light_group: Option<LightGroupFacts>,
    /// The class has an interaction state machine (`SInteractionStateMachineParams`).
    pub state_machine: bool,
    /// The class is a walk-up trigger (`SCItemProximitySensorParams`; exported as a proximity sensor).
    pub proximity_sensor: bool,
}

pub(super) fn text<'v, 'a>(value: &'v Value<'a>) -> Option<&'a str> {
    match value {
        Value::String(s) | Value::Locale(s) | Value::Enum(s) => Some(*s),
        _ => None,
    }
}

pub(super) fn boolean(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

/// The `Name` of a (followed) interaction pointer.
pub(super) fn interaction_name(value: Option<&Value>) -> Option<String> {
    value.and_then(|v| field(v, "Name")).and_then(text).filter(|s| !s.is_empty()).map(str::to_string)
}

pub(super) fn query<'a>(db: &'a Database, record: &Record, path: &str) -> Option<Value<'a>> {
    let compiled = db.compile_path::<Value>(record.struct_id(), path).ok()?;
    db.query_single::<Value>(&compiled, record).ok().flatten()
}

fn tag_text(value: Option<&Value>) -> Option<String> {
    value.and_then(text).filter(|s| !s.is_empty()).map(str::to_string)
}

pub(crate) fn door_timing(component: &Value) -> DoorTiming {
    let animation = field(component, "AnimationParams");
    DoorTiming {
        animation_speed: field(component, "DoorAnimationSpeed").and_then(number),
        use_auto_close_delay: field(component, "UseAutoCloseDelay").and_then(boolean),
        auto_close_delay: field(component, "AutoCloseDelay").and_then(number),
        is_opened: field(component, "IsOpened").and_then(boolean),
        is_locked: field(component, "IsLocked").and_then(boolean),
        animation_params: match animation {
            Some(Value::Object { type_name, .. }) if !type_name.is_empty() => Some(type_name.to_string()),
            _ => None,
        },
        transition_tag: tag_text(animation.and_then(|a| field(a, "TransitionTag"))),
        open_tag: tag_text(animation.and_then(|a| field(a, "OpenTag"))),
        close_tag: tag_text(animation.and_then(|a| field(a, "CloseTag"))),
    }
}

/// The door with a `.soc` instance's overrides applied: each authored value replaces the class's; an instance
/// `AnimationParams` of another type replaces the class's whole struct (its tags with it).
pub(crate) fn door_with_overrides(mut door: DoorTiming, overrides: &EntityOverrides) -> DoorTiming {
    door.animation_speed = overrides.door_animation_speed.or(door.animation_speed);
    door.use_auto_close_delay = overrides.use_auto_close_delay.or(door.use_auto_close_delay);
    door.auto_close_delay = overrides.auto_close_delay.or(door.auto_close_delay);
    door.is_opened = overrides.door_is_opened.or(door.is_opened);
    door.is_locked = overrides.door_is_locked.or(door.is_locked);
    if let Some(animation) = &overrides.door_animation {
        if animation.type_name.is_some() && animation.type_name != door.animation_params {
            door.animation_params = animation.type_name.clone();
            door.transition_tag = None;
            door.open_tag = None;
            door.close_tag = None;
        }
        door.transition_tag = animation.transition_tag.clone().or(door.transition_tag);
        door.open_tag = animation.open_tag.clone().or(door.open_tag);
        door.close_tag = animation.close_tag.clone().or(door.close_tag);
    }
    door
}

/// `vehicleItemPortInteractionLinks` of an `SInteractionLinkParams` value, in authored order; an entry without a
/// source interaction, port or target interaction is skipped (logged).
pub(crate) fn item_port_links(component: &Value) -> Vec<ItemPortLink> {
    let Some(Value::Array(links)) = field(component, "vehicleItemPortInteractionLinks") else {
        return Vec::new();
    };
    links
        .iter()
        .filter_map(|link| {
            let source = interaction_name(field(link, "sourceInteraction"));
            let port = field(link, "itemPort").and_then(text).filter(|s| !s.is_empty());
            let target = field(link, "targetInteraction").and_then(text).filter(|s| !s.is_empty());
            match (source, port, target) {
                (Some(source_slot), Some(port), Some(target)) => {
                    Some(ItemPortLink { source_slot, target_port: port.to_string(), target_slot: target.to_string() })
                }
                (source, port, target) => {
                    log::warn!("item-port interaction link with an unresolved part ({source:?}, {port:?}, {target:?}) skipped");
                    None
                }
            }
        })
        .collect()
}

/// The door of an entity class (`SCItemDoorParams`), without instance overrides (a loadout child).
pub(crate) fn class_door(db: &Database, record: &Record) -> Option<DoorTiming> {
    query(db, record, "Components[SCItemDoorParams]").map(|c| door_timing(&c))
}

/// The vehicle item-port interaction links of an entity class.
pub(crate) fn class_item_port_links(db: &Database, record: &Record) -> Vec<ItemPortLink> {
    query(db, record, "Components[SInteractionLinkParams]").map(|c| item_port_links(&c)).unwrap_or_default()
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
                    let shared = field(i, "Interaction")?;
                    let display = |value| text(value).filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("@LOC_EMPTY"));
                    Some(PointInteraction {
                        name: interaction_name(Some(shared))?,
                        // The point can override the label; LOC_EMPTY means it
                        // uses the label from its shared interaction.
                        display_name_key: field(i, "DisplayName").and_then(display)
                            .or_else(|| field(shared, "DisplayName").and_then(display)).map(str::to_string),
                        display_name: None,
                        primary: field(i, "IsPrimary").and_then(boolean).unwrap_or(false),
                    })
                })
                .collect();
            let constraint = field(point, "AngleConstraint");
            (!interactions.is_empty()).then(|| InteractionPoint {
                index,
                helper_name,
                radius: field(point, "Radius").and_then(number),
                angle_range: constraint.and_then(|c| field(c, "angleRange")).and_then(number),
                view_angle_limit: constraint.and_then(|c| field(c, "viewAngleLimit")).and_then(number),
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
    facts.door = class_door(db, record);
    facts.item_port_links = class_item_port_links(db, record);
    let mut light_group = query(db, record, "Components[SLightGroupComponentParams]").is_some();
    if let Some(interactable) = query(db, record, "Components[SEntityInteractableParams].Interactable") {
        facts.interaction_points = interaction_points(&interactable);
        facts.shared_interactions = shared_interactions(&interactable);
        // Linking tags and the state machine are read with their pointer identities from the record JSON (a large
        // state machine exceeds the materialized query's budget, so its presence is read there too).
        if let Some(json) = super::interaction_graph::record_json(db, record) {
            let graph = super::interaction_graph::RecordGraph::new(&json);
            let tag = |t: &serde_json::Value| super::interaction_graph::tag_name(db, t);
            facts.sendable_interactions = super::interaction_graph::sendable_interactions(&graph, &tag);
            facts.state_machine = graph.component("SInteractionStateMachineParams").is_some();
            facts.proximity_sensor = graph.component("SCItemProximitySensorParams").is_some();
            light_group |= graph.component("SLightGroupComponentParams").is_some();
            if light_group {
                facts.light_group =
                    Some(LightGroupFacts { interactions: super::interaction_graph::light_group_interactions(&graph, &tag) });
            }
        }
    }
    if light_group && facts.light_group.is_none() {
        facts.light_group = Some(LightGroupFacts { interactions: Vec::new() });
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

    /// Door timing and state of an entity: its class's `SCItemDoorParams` with the instance overrides.
    pub(crate) fn door(&mut self, guid: Option<&str>, overrides: &EntityOverrides) -> Option<DoorTiming> {
        let door = door_with_overrides(self.class(guid)?.door.clone()?, overrides);
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
#[path = "interior_interactions_tests.rs"]
mod tests;
