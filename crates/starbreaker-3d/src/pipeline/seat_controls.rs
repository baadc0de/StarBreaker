//! Authored vehicle XML seat controls. Role and dashboard links come from
//! controller priority groups and tags, independent of loadout port names.
use starbreaker_cryxml::{CryXml, CryXmlNode};
use starbreaker_datacore::{Database, types::Record};
use starbreaker_p4k::MappedP4k;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub(crate) struct SeatRoute {
    pub can_drive: Option<bool>,
    pub dashboard_port: Option<String>,
}

#[derive(Default)]
struct PortControl {
    name: String,
    item_type: String,
    controllable_tags: Vec<String>,
    flight: Option<Option<PriorityGroup>>,
    dashboard: Option<PriorityGroup>,
}

struct PriorityGroup {
    tags: Vec<(String, bool)>,
    controls_by_default: bool,
}

fn attribute<'a>(xml: &'a CryXml, node: &'a CryXmlNode, name: &str) -> Option<&'a str> {
    xml.node_attributes(node)
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
}

fn child<'a>(xml: &'a CryXml, node: &'a CryXmlNode, name: &str) -> Option<&'a CryXmlNode> {
    xml.node_children(node)
        .find(|entry| xml.node_tag(entry) == name)
}

fn controls(priority: &str) -> bool {
    priority.eq_ignore_ascii_case("exclusive_control")
        || priority.parse::<i32>().ok().is_some_and(|value| value > 0)
}

fn group(xml: &CryXml, definition: &CryXmlNode, kind: &str) -> Option<PriorityGroup> {
    let groups = child(xml, definition, "PriorityGroups")?;
    let node = xml.node_children(groups).find(|entry| {
        xml.node_tag(entry) == "PriorityGroup"
            && attribute(xml, entry, "itemType").is_some_and(|item| item.eq_ignore_ascii_case(kind))
    })?;
    let tags = xml
        .node_children(node)
        .filter(|entry| xml.node_tag(entry) == "tags")
        .filter_map(|entry| {
            let tag = attribute(xml, entry, "tag")?;
            let priority = child(xml, entry, "Priority")
                .and_then(|priority| attribute(xml, priority, "value"))?;
            Some((tag.to_ascii_lowercase(), controls(priority)))
        })
        .collect();
    // An authored group without a default override has its normal control
    // priority. Observe-only and no-control are explicit overrides.
    let controls_by_default = attribute(xml, node, "defaultPriority").is_none_or(controls);
    Some(PriorityGroup {
        tags,
        controls_by_default,
    })
}

fn collect(xml: &CryXml, node: &CryXmlNode, ports: &mut BTreeMap<String, PortControl>) {
    if xml.node_tag(node) == "Part" {
        if let Some((name, item)) = attribute(xml, node, "name").zip(child(xml, node, "ItemPort")) {
            let kind = child(xml, item, "Types")
                .into_iter()
                .flat_map(|types| xml.node_children(types))
                .filter_map(|kind| attribute(xml, kind, "type"))
                .find(|kind| {
                    ["Seat", "SeatDashboard", "FlightController"]
                        .iter()
                        .any(|candidate| kind.eq_ignore_ascii_case(candidate))
                });
            if kind.is_some() || ports.contains_key(&name.to_ascii_lowercase()) {
                let port = ports.entry(name.to_ascii_lowercase()).or_default();
                port.name = name.to_owned();
                if let Some(kind) = kind {
                    port.item_type = kind.to_ascii_lowercase();
                }
                if let Some(controller) = child(xml, item, "ControllerDef") {
                    if let Some(tags) = attribute(xml, controller, "controllableTags") {
                        port.controllable_tags = tags
                            .split(|c: char| c == ',' || c.is_ascii_whitespace())
                            .filter(|s| !s.is_empty())
                            .map(str::to_ascii_lowercase)
                            .collect();
                    }
                    if let Some(user) = child(xml, controller, "UserDef") {
                        port.flight = Some(group(xml, user, "FlightController"));
                    }
                    if let Some(usable) = child(xml, controller, "UsableDef") {
                        port.dashboard = group(xml, usable, "SeatDashboard");
                    }
                }
            }
        }
    }
    for child in xml.node_children(node) {
        collect(xml, child, ports);
    }
}

fn route(ports: &BTreeMap<String, PortControl>, seat_port: &str) -> SeatRoute {
    let Some(port) = ports
        .get(&seat_port.to_ascii_lowercase())
        .filter(|p| p.item_type == "seat")
    else {
        return SeatRoute::default();
    };
    let grants = |group: &PriorityGroup, candidate: &PortControl| {
        group
            .tags
            .iter()
            .find(|(tag, _)| candidate.controllable_tags.contains(tag))
            .map_or(group.controls_by_default, |(_, controls)| *controls)
    };
    let can_drive = port.flight.as_ref().map(|flight| {
        flight.as_ref().is_some_and(|group| {
            let mut candidates = ports
                .values()
                .filter(|candidate| candidate.item_type == "flightcontroller")
                .peekable();
            if candidates.peek().is_some() {
                candidates.any(|candidate| grants(group, candidate))
            } else {
                group.controls_by_default
            }
        })
    });
    let dashboard_port = port.dashboard.as_ref().and_then(|group| {
        let mut matching = ports
            .values()
            .filter(|candidate| candidate.item_type == "seatdashboard" && grants(group, candidate));
        let found = matching.next()?;
        matching.next().is_none().then(|| found.name.clone())
    });
    SeatRoute {
        can_drive,
        dashboard_port,
    }
}

pub(crate) fn load(
    db: &Database,
    p4k: &MappedP4k,
    vehicle: &Record,
) -> BTreeMap<String, SeatRoute> {
    let mut ports = BTreeMap::new();
    super::vehicle_xml::visit_vehicle_xml(db, p4k, vehicle, |xml, _| {
        collect(xml, xml.root(), &mut ports)
    });
    ports
        .keys()
        .map(|port| (port.clone(), route(&ports, port)))
        .collect()
}

#[cfg(test)]
#[path = "seat_controls_tests.rs"]
mod tests;
