//! Placed audio of an object container's `.soc`: trigger spots and area ambiences (route A of WP-A1).
//!
//! An entity whose `PropertiesDataCore` holds an `AudioTriggerSpotParams` component with a play or stop trigger is a
//! **spot**: position, its component's authored overrides (the class supplies the rest, resolved at export), the
//! trigger names, its `EntityLinks` and its `AudioPropagationComponent staticApcRoom`. A spot's links to entities that
//! carry no trigger of their own are the **extra positions** of a multi-position spot (one light hum at many lights).
//! An entity with an `AudioAreaAmbienceComponentParams` component is an **area**: the same fields, the spots it links
//! (that it governs), and its extent — the `AreaShape` segments whose `<Entities>` list the area (parsed by
//! [`super::room_volume::extract_room_volumes`] with the areas' GUIDs as the registry).
//!
//! Components are recognised by their DataCore type (`__type`), never by entity class or name. Positions are in the
//! same container-local space as the container's lights.

use std::collections::{BTreeMap, HashMap, HashSet};

use starbreaker_cryxml::{CryXml, CryXmlNode};

use super::room_volume::{RoomVolume, extract_room_volumes};

const SPOT_TYPE: &str = "AudioTriggerSpotParams";
const AREA_TYPE: &str = "AudioAreaAmbienceComponentParams";

/// A placed audio entity (spot or area) as authored.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedAudio {
    pub entity_id: String,
    pub name: String,
    pub class_guid: Option<String>,
    pub parent_entity_id: Option<String>,
    /// Container-local position.
    pub position: [f32; 3],
    /// The component's own attributes (overrides of the class defaults).
    pub params: BTreeMap<String, String>,
    pub play_trigger: Option<String>,
    pub stop_trigger: Option<String>,
    /// `EntityLinks` targets (lower case).
    pub links: Vec<String>,
    /// `AudioPropagationComponent staticApcRoom` (lower case).
    pub room: Option<String>,
    /// Spots: positions of linked entities without a trigger (multi-position).
    pub extra_positions: Vec<[f32; 3]>,
    /// Areas: their extent.
    pub shapes: Vec<RoomVolume>,
}

impl PlacedAudio {
    /// Apply the container's non-item-port delta (as lights and room volumes receive it).
    pub(crate) fn transformed(mut self, delta: [[f32; 4]; 4]) -> Self {
        let m = glam::Mat4::from_cols_array_2d(&delta);
        let move_point = |p: [f32; 3]| m.transform_point3(glam::Vec3::from_array(p)).to_array();
        self.position = move_point(self.position);
        self.extra_positions = self.extra_positions.into_iter().map(move_point).collect();
        self.shapes = self.shapes.into_iter().map(|s| s.transformed(delta)).collect();
        self
    }
}

/// The placed audio of a container.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InteriorAudio {
    pub spots: Vec<PlacedAudio>,
    pub areas: Vec<PlacedAudio>,
    /// Spot components without a trigger (positions of multi-position spots, or unused).
    pub untriggered: usize,
    pub(crate) room_connectors: Vec<crate::decomposed_propagation::RoomConnector>,
}

impl InteriorAudio {
    pub(crate) fn extend(&mut self, other: InteriorAudio, delta: [[f32; 4]; 4]) {
        self.spots.extend(other.spots.into_iter().map(|s| s.transformed(delta)));
        self.areas.extend(other.areas.into_iter().map(|a| a.transformed(delta)));
        self.untriggered += other.untriggered;
    }
}

/// The spots and areas of one `.soc` entity chunk: `(spots, areas, untriggered spot components)`.
pub(crate) fn extract_audio_entities(xml: &CryXml) -> (Vec<PlacedAudio>, Vec<PlacedAudio>, usize) {
    let root = xml.root();
    let entities: Vec<&CryXmlNode> = match xml
        .node_children(root)
        .find(|c| matches!(xml.node_tag(c), "Entities" | "SCOC_Entities"))
    {
        Some(container) => xml.node_children(container).filter(|e| xml.node_tag(e) == "Entity").collect(),
        None if matches!(xml.node_tag(root), "Entities" | "SCOC_Entities") => {
            xml.node_children(root).filter(|e| xml.node_tag(e) == "Entity").collect()
        }
        None => Vec::new(),
    };
    let mut positions: HashMap<String, [f32; 3]> = HashMap::new();
    let mut triggered: HashSet<String> = HashSet::new();
    let (mut spots, mut areas) = (Vec::new(), Vec::new());
    let mut untriggered = 0usize;
    for entity in &entities {
        let attrs: HashMap<&str, &str> = xml.node_attributes(entity).collect();
        let Some(id) = attrs.get("EntityCryGUID").map(|g| g.to_ascii_lowercase()) else { continue };
        let position = parse_vec3(attrs.get("Pos").copied());
        positions.insert(id.clone(), position);
        let Some((kind, component)) = audio_component(xml, entity) else { continue };
        let mut placed = PlacedAudio {
            entity_id: id.clone(),
            name: attrs.get("Name").copied().unwrap_or_default().to_owned(),
            class_guid: attrs.get("EntityClassGUID").map(|g| g.to_ascii_lowercase()),
            parent_entity_id: attrs
                .get("ParentEntityCryGUID")
                .filter(|g| g.chars().any(|c| c != '0' && c != '-'))
                .map(|g| g.to_ascii_lowercase()),
            position,
            params: xml
                .node_attributes(component)
                .filter(|(k, _)| !k.starts_with("__"))
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
            play_trigger: None,
            stop_trigger: None,
            links: xml
                .node_children(entity)
                .filter(|c| xml.node_tag(c) == "EntityLinks")
                .flat_map(|l| xml.node_children(l))
                .filter_map(|l| xml.node_attributes(l).find(|(k, _)| *k == "TargetCryGUID").map(|(_, v)| v.to_ascii_lowercase()))
                .collect(),
            room: xml
                .node_children(entity)
                .find(|c| xml.node_tag(c) == "AudioPropagationComponent")
                .and_then(|c| xml.node_attributes(c).find(|(k, _)| *k == "staticApcRoom").map(|(_, v)| v.to_ascii_lowercase())),
            extra_positions: Vec::new(),
            shapes: Vec::new(),
        };
        for child in xml.node_children(component) {
            let trigger = xml.node_attributes(child).find(|(k, _)| *k == "audioTrigger").map(|(_, v)| v.trim().to_owned());
            let Some(trigger) = trigger.filter(|t| !t.is_empty()) else { continue };
            let tag = xml.node_tag(child).to_ascii_lowercase();
            if tag.contains("stop") {
                placed.stop_trigger = Some(trigger);
            } else if tag.contains("play") {
                placed.play_trigger = Some(trigger);
            }
        }
        if kind == SPOT_TYPE {
            if placed.play_trigger.is_some() || placed.stop_trigger.is_some() {
                triggered.insert(id);
                spots.push(placed);
            } else {
                untriggered += 1;
            }
        } else {
            triggered.insert(id);
            areas.push(placed);
        }
    }
    for spot in &mut spots {
        spot.extra_positions = spot
            .links
            .iter()
            .filter(|l| !triggered.contains(*l))
            .filter_map(|l| positions.get(l).copied())
            .collect();
    }
    if !areas.is_empty() {
        let ids: HashSet<String> = areas.iter().map(|a| a.entity_id.clone()).collect();
        for shape in extract_room_volumes(xml, &ids) {
            if let Some(area) = areas.iter_mut().find(|a| a.entity_id == shape.room_guid) {
                area.shapes.push(shape);
            }
        }
    }
    (spots, areas, untriggered)
}

/// The entity's audio component (`AudioTriggerSpotParams` / `AudioAreaAmbienceComponentParams`) under
/// `PropertiesDataCore`.
fn audio_component<'a>(xml: &'a CryXml, entity: &'a CryXmlNode) -> Option<(&'static str, &'a CryXmlNode)> {
    xml.node_children(entity)
        .filter(|c| xml.node_tag(c) == "PropertiesDataCore")
        .flat_map(|p| xml.node_children(p))
        .find_map(|c| match xml.node_attributes(c).find(|(k, _)| *k == "__type").map(|(_, v)| v) {
            Some(SPOT_TYPE) => Some((SPOT_TYPE, c)),
            Some(AREA_TYPE) => Some((AREA_TYPE, c)),
            _ => None,
        })
}

fn parse_vec3(text: Option<&str>) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (slot, v) in out.iter_mut().zip(text.unwrap_or("").split(',')) {
        *slot = v.trim().parse().unwrap_or(0.0);
    }
    out
}

#[cfg(test)]
#[path = "audio_entities_tests.rs"]
mod tests;
