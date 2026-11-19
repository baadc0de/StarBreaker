//! Lifts and the geometry-less interactive entities of an interior container
//! (WP-P5), from its `.soc` entities and their classes:
//!
//! - [`InteriorEntity`]: an entity the player or a lift interacts with that has
//!   no mesh of its own — a lift's path nodes (`SCItemElevatorPathNodeParams`),
//!   transit gateways and limbo nodes, and interaction-only entities (a call
//!   button whose geometry is helpers only). Their interaction points carry the
//!   helper frames, which no mesh supplies.
//! - [`Lift`]: one per entity whose class has `SCItemElevatorParams` (a
//!   carriage): its parameters, rest node, stops in chain order with their
//!   landing doors, and the entities that ride it.
//! - [`order_stops`]: the chain rule. Path nodes link the next node by
//!   `EntityLinks`; the stop order starts at the node no other node links to.
//!   A branched, cyclic or disconnected node graph is an error and that lift is
//!   not exported.
//! - [`resolve_container`]: both lists for one container, plus the door timing
//!   and interaction points of its placements.

use std::collections::{HashMap, HashSet};

use super::interior_interactions::{ElevatorParams, InteractionResolver};
use crate::types::{Accessibility, InteractionLink, InteractionPoint, InteriorMesh, InteriorPlacement, ParentAttachment, TraversalLink};

/// A geometry-less interactive entity of an interior container.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct InteriorEntity {
    pub entity_id: String,
    pub entity_name: Option<String>,
    pub entity_class_guid: Option<String>,
    pub entity_class: String,
    /// `path_node`, `transit_gateway`, `transit_limbo` or `interaction`.
    pub kind: &'static str,
    /// Container-local, the frame of the container's placements.
    pub transform: [[f32; 4]; 4],
    pub links: Vec<String>,
    pub interaction_links: Vec<InteractionLink>,
    pub parent_attachment: Option<ParentAttachment>,
    pub interaction_points: Vec<InteractionPoint>,
    pub traversal_links: Vec<TraversalLink>,
}

/// One stop (or pass-through node) of a lift.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct LiftStop {
    /// Position in the chain: the carriage's slot index (`From Elevator call <index>`, `Arrived at <index>`).
    pub index: usize,
    pub node_entity_id: String,
    pub node_entity_name: Option<String>,
    /// `SCItemElevatorPathNodeParams.Type`: `stop`, `waypoint`, `teleport`.
    pub kind: String,
    /// Container-local metres.
    pub position: [f32; 3],
    /// The node's `EntityLinks` targets that are doors (`SCItemDoorParams`).
    pub landing_door_ids: Vec<String>,
    pub teleport_time_scale: Option<f32>,
    pub gateway_timing_in_transit: Option<f32>,
    pub gateway_interaction: Option<String>,
    pub accessibility: Option<Accessibility>,
}

/// A lift carriage and its path.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct Lift {
    pub entity_id: String,
    pub entity_name: Option<String>,
    pub entity_class_guid: Option<String>,
    pub entity_class: String,
    pub params: ElevatorParams,
    pub grid_geometry: Option<String>,
    /// The path node the carriage links: where it rests.
    pub rest_node_id: String,
    pub stops: Vec<LiftStop>,
    /// Entities whose parent chain (`ParentEntityCryGUID`) reaches the carriage.
    pub riders: Vec<String>,
}

impl InteriorEntity {
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

impl Lift {
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// Why a node graph is not a chain.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ChainError {
    /// A node links more than one next node, or more than one node links it.
    Branched(String),
    /// No node is unlinked (a cycle), or several are.
    NoSingleHead(usize),
    /// The chain from the head does not reach every node.
    Disconnected { reached: usize, nodes: usize },
}

/// Chain order of the path nodes connected to `rest`. `next` maps each path
/// node id to the path-node ids its `EntityLinks` target (non-nodes removed).
pub(crate) fn order_stops(rest: &str, next: &HashMap<String, Vec<String>>) -> Result<Vec<String>, ChainError> {
    // The connected component of `rest`, links followed both ways.
    let mut component: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue = vec![rest.to_string()];
    while let Some(id) = queue.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        component.push(id.clone());
        queue.extend(next.get(&id).into_iter().flatten().cloned());
        queue.extend(next.iter().filter(|(_, to)| to.contains(&id)).map(|(from, _)| from.clone()));
    }
    component.sort();
    let mut incoming: HashMap<&str, usize> = HashMap::new();
    for id in &component {
        let out = next.get(id).map(Vec::as_slice).unwrap_or_default();
        if out.len() > 1 {
            return Err(ChainError::Branched(id.clone()));
        }
        for to in out {
            *incoming.entry(to.as_str()).or_default() += 1;
        }
    }
    if let Some((id, _)) = incoming.iter().find(|(_, n)| **n > 1) {
        return Err(ChainError::Branched(id.to_string()));
    }
    let heads: Vec<&String> = component.iter().filter(|id| !incoming.contains_key(id.as_str())).collect();
    let [head] = heads.as_slice() else {
        return Err(ChainError::NoSingleHead(heads.len()));
    };
    let mut order = vec![(*head).clone()];
    while let Some(to) = next.get(order.last().unwrap()).and_then(|o| o.first()) {
        if order.contains(to) {
            break;
        }
        order.push(to.clone());
    }
    if order.len() != component.len() {
        return Err(ChainError::Disconnected { reached: order.len(), nodes: component.len() });
    }
    Ok(order)
}

fn translation(transform: &[[f32; 4]; 4]) -> [f32; 3] {
    [transform[3][0], transform[3][1], transform[3][2]]
}

/// The entities and lifts of one container, and its placements' door timing
/// and interaction points. `meshes` are the container's `.soc` entities;
/// `placed`: the entity ids that became placements (they carry a mesh).
pub(crate) fn resolve_container(
    resolver: &mut InteractionResolver,
    p4k: Option<&starbreaker_p4k::MappedP4k>,
    meshes: &[InteriorMesh],
    placements: &mut [InteriorPlacement],
    placed: &HashSet<String>,
) -> (Vec<InteriorEntity>, Vec<Lift>) {
    for placement in placements.iter_mut() {
        let guid = placement.identity.entity_class_guid.clone();
        let overrides = meshes
            .iter()
            .find(|m| m.entity.entity_id.is_some() && m.entity.entity_id == placement.identity.entity_id)
            .map(|m| m.entity.overrides.clone())
            .unwrap_or_default();
        placement.identity.door = resolver.door(guid.as_deref(), &overrides);
        placement.identity.interaction_points =
            resolver.class(guid.as_deref()).map(|f| f.interaction_points.clone()).unwrap_or_default();
    }
    let by_id: HashMap<&str, &InteriorMesh> =
        meshes.iter().filter_map(|m| Some((m.entity.entity_id.as_deref()?, m))).collect();

    let mut entities = Vec::new();
    for mesh in meshes {
        let Some(id) = mesh.entity.entity_id.as_deref() else { continue };
        if placed.contains(id) {
            continue;
        }
        let Some(facts) = resolver.class(mesh.entity_class_guid.as_deref()).cloned() else { continue };
        let kind = if facts.path_node.is_some() {
            "path_node"
        } else if let Some(role) = facts.transit_role {
            role
        } else if !facts.interaction_points.is_empty() && !mesh.entity.interaction_links.is_empty() {
            "interaction"
        } else {
            continue;
        };
        let mut points = facts.interaction_points.clone();
        let geometry = facts.geometry_path.as_deref().filter(|g| !g.is_empty());
        let nmc = geometry.zip(p4k).and_then(|(g, p4k)| super::load_nmc_for_cgf(p4k, g));
        for point in &mut points {
            point.helper_transform = super::resolve_nmc_helper_transform(nmc.as_ref(), Some(&point.helper_name))
                .map(|m| m.to_cols_array_2d());
        }
        entities.push(InteriorEntity {
            entity_id: id.to_string(),
            entity_name: mesh.entity.entity_name.clone(),
            entity_class_guid: mesh.entity_class_guid.clone(),
            entity_class: facts.record_name.clone(),
            kind,
            transform: mesh.transform,
            links: mesh.entity.links.clone(),
            interaction_links: mesh.entity.interaction_links.clone(),
            parent_attachment: mesh.entity.parent.clone(),
            interaction_points: points,
            traversal_links: resolver.traversal_names(mesh.entity_class_guid.as_deref(), &mesh.entity.overrides.traversal_links),
        });
    }

    let mut lifts = Vec::new();
    for mesh in meshes {
        let Some(id) = mesh.entity.entity_id.as_deref() else { continue };
        let Some(facts) = resolver.class(mesh.entity_class_guid.as_deref()).cloned() else { continue };
        let Some(params) = facts.elevator.clone() else { continue };
        match lift(resolver, &by_id, mesh, id, &facts.record_name, params, facts.grid_geometry.clone()) {
            Ok(lift) => lifts.push(lift),
            Err(reason) => log::warn!("[interior] lift {} ({id}) not exported: {reason}", facts.record_name),
        }
    }
    (entities, lifts)
}

fn lift(
    resolver: &mut InteractionResolver,
    by_id: &HashMap<&str, &InteriorMesh>,
    carriage: &InteriorMesh,
    id: &str,
    record_name: &str,
    params: ElevatorParams,
    grid_geometry: Option<String>,
) -> Result<Lift, String> {
    let is_node = |resolver: &mut InteractionResolver, target: &str| {
        by_id
            .get(target)
            .is_some_and(|m| resolver.class(m.entity_class_guid.as_deref()).is_some_and(|f| f.path_node.is_some()))
    };
    let rest = carriage
        .entity
        .links
        .iter()
        .find(|t| is_node(resolver, t))
        .cloned()
        .ok_or("the carriage links no path node")?;
    let mut next: HashMap<String, Vec<String>> = HashMap::new();
    for (node_id, mesh) in by_id.iter() {
        if !is_node(resolver, node_id) {
            continue;
        }
        let to: Vec<String> = mesh.entity.links.iter().filter(|t| is_node(resolver, t)).cloned().collect();
        next.insert(node_id.to_string(), to);
    }
    let order = order_stops(&rest, &next).map_err(|e| format!("path nodes are not a chain: {e:?}"))?;
    let mut stops = Vec::new();
    for (index, node_id) in order.iter().enumerate() {
        let node = by_id[node_id.as_str()];
        let class = resolver.class(node.entity_class_guid.as_deref()).and_then(|f| f.path_node.clone()).ok_or("a path node lost its class")?;
        let landing_door_ids = node
            .entity
            .links
            .iter()
            .filter(|t| by_id.get(t.as_str()).is_some_and(|m| resolver.class(m.entity_class_guid.as_deref()).is_some_and(|f| f.door.is_some())))
            .cloned()
            .collect();
        stops.push(LiftStop {
            index,
            node_entity_id: node_id.clone(),
            node_entity_name: node.entity.entity_name.clone(),
            kind: class.kind.clone(),
            position: translation(&node.transform),
            landing_door_ids,
            teleport_time_scale: class.teleport_time_scale,
            gateway_timing_in_transit: class.gateway_timing_in_transit,
            gateway_interaction: node.entity.overrides.gateway_interaction.clone().or(class.gateway_interaction.clone()),
            accessibility: node.entity.overrides.accessibility.clone(),
        });
    }
    let mut riders: Vec<String> = Vec::new();
    for (rider, _) in by_id.iter() {
        let mut at = *rider;
        let mut hops = 0;
        while let Some(parent) = by_id.get(at).and_then(|m| m.entity.parent.as_ref()) {
            if parent.parent_entity_id == id {
                riders.push(rider.to_string());
                break;
            }
            at = parent.parent_entity_id.as_str();
            hops += 1;
            if hops > by_id.len() {
                break; // a cyclic parent chain reaches nothing
            }
        }
    }
    riders.sort();
    Ok(Lift {
        entity_id: id.to_string(),
        entity_name: carriage.entity.entity_name.clone(),
        entity_class_guid: carriage.entity_class_guid.clone(),
        entity_class: record_name.to_string(),
        params,
        grid_geometry,
        rest_node_id: rest,
        stops,
        riders,
    })
}

#[cfg(test)]
#[path = "lifts_tests.rs"]
mod tests;
