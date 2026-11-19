//! Interaction wiring of `.soc` `<Entity>` nodes (WP-P5): what an entity sends
//! to which other entity, what it rides, and the instance overrides that shape
//! its behaviour.
//!
//! - [`interaction_links`]: `InteractionLinkController/Link` (source slot →
//!   target entity, target slot), in authored order.
//! - [`parent_attachment`]: `ParentEntityCryGUID` + `AttachmentType` /
//!   `AttachmentTarget` / `ParentOffset*` (a lift gate riding its carriage).
//! - [`tile_port_name`]: `TileAttachmentParams@ItemPortName`, the root
//!   container item port the entity occupies.
//! - [`overrides`]: `PropertiesDataCore` values by component `__type`
//!   (`SCItemDoorParams`, `SCItemElevatorPathNodeParams`,
//!   `SEntityTraversingTreeParams`).
//!
//! Nothing here consults an entity class or name: the components are known by
//! their authored `__type`.

use starbreaker_cryxml::{CryXml, CryXmlNode};

use crate::types::{Accessibility, EntityOverrides, InteractionLink, ParentAttachment, TraversalLink};

fn children<'a, 'x>(xml: &'x CryXml<'a>, node: &'x CryXmlNode) -> impl Iterator<Item = &'a CryXmlNode> + 'x {
    xml.node_children(node)
}

fn child<'a>(xml: &CryXml<'a>, node: &CryXmlNode, tag: &str) -> Option<&'a CryXmlNode> {
    xml.node_children(node).find(|c| xml.node_tag(c) == tag)
}

fn attr<'a>(xml: &CryXml<'a>, node: &CryXmlNode, name: &str) -> Option<&'a str> {
    xml.node_attributes(node).find(|(key, _)| *key == name).map(|(_, value)| value)
}

fn csv(text: &str) -> Vec<f64> {
    text.split(',').filter_map(|s| s.trim().parse().ok()).collect()
}

/// `InteractionLinkController/Link`s with a non-null target, in authored order.
pub(crate) fn interaction_links(xml: &CryXml, entity: &CryXmlNode) -> Vec<InteractionLink> {
    let Some(controller) = child(xml, entity, "InteractionLinkController") else {
        return Vec::new();
    };
    children(xml, controller)
        .filter(|link| xml.node_tag(link) == "Link")
        .filter_map(|link| {
            let target = attr(xml, link, "TargetCryGUID").filter(|t| super::is_nonzero_guid(t))?;
            Some(InteractionLink {
                source_slot: attr(xml, link, "SourceSlotName").unwrap_or_default().to_string(),
                target_entity_id: target.to_string(),
                target_slot: attr(xml, link, "TargetSlotName").unwrap_or_default().to_string(),
            })
        })
        .collect()
}

/// The entity's parent attachment, when it names a parent entity.
pub(crate) fn parent_attachment(xml: &CryXml, entity: &CryXmlNode) -> Option<ParentAttachment> {
    let parent = attr(xml, entity, "ParentEntityCryGUID").filter(|g| super::is_nonzero_guid(g))?;
    let position = attr(xml, entity, "ParentOffsetPos").map(csv).unwrap_or_default();
    let rotation = attr(xml, entity, "ParentOffsetRot").map(csv).unwrap_or_default();
    let scale = attr(xml, entity, "ParentOffsetScale").map(csv).unwrap_or_default();
    Some(ParentAttachment {
        parent_entity_id: parent.to_string(),
        attachment_type: attr(xml, entity, "AttachmentType").filter(|s| !s.is_empty()).map(str::to_string),
        attachment_target: attr(xml, entity, "AttachmentTarget").filter(|s| !s.is_empty()).map(str::to_string),
        offset_position: <[f64; 3]>::try_from(position.as_slice()).unwrap_or([0.0; 3]),
        offset_rotation: <[f64; 4]>::try_from(rotation.as_slice()).unwrap_or([1.0, 0.0, 0.0, 0.0]),
        // A uniform scale is authored as one value; per-axis as three.
        offset_scale: match scale.as_slice() {
            [s] => [*s; 3],
            [x, y, z] => [*x, *y, *z],
            _ => [1.0; 3],
        },
    })
}

/// `TileAttachmentParams@ItemPortName`.
pub(crate) fn tile_port_name(xml: &CryXml, entity: &CryXmlNode) -> Option<String> {
    let tile = child(xml, entity, "TileAttachmentParams")?;
    attr(xml, tile, "ItemPortName").filter(|s| !s.is_empty()).map(str::to_string)
}

/// The `PropertiesDataCore` component of `__type` `type_name` (the live
/// values; `specialWeakPointers` repeats component names without values).
fn component<'a>(xml: &CryXml<'a>, entity: &CryXmlNode, type_name: &str) -> Option<&'a CryXmlNode> {
    let properties = child(xml, entity, "PropertiesDataCore")?;
    xml.node_children(properties).find(|c| attr(xml, c, "__type") == Some(type_name))
}

/// Instance overrides of the components this exporter reads.
pub(crate) fn overrides(xml: &CryXml, entity: &CryXmlNode) -> EntityOverrides {
    let mut out = EntityOverrides::default();
    if let Some(door) = component(xml, entity, "SCItemDoorParams") {
        out.door_animation_speed = attr(xml, door, "DoorAnimationSpeed").and_then(|v| v.parse().ok());
        out.use_auto_close_delay = attr(xml, door, "UseAutoCloseDelay").map(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        out.auto_close_delay = attr(xml, door, "AutoCloseDelay").and_then(|v| v.parse().ok());
    }
    if let Some(node) = component(xml, entity, "SCItemElevatorPathNodeParams") {
        out.gateway_interaction = child(xml, node, "gatewayParams")
            .and_then(|g| attr(xml, g, "interaction"))
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        out.accessibility = child(xml, node, "accessibilityParams").and_then(|a| {
            let kind = attr(xml, a, "__type").filter(|t| !t.is_empty() && *t != "<NullPtr>")?;
            Some(Accessibility {
                kind: kind.to_string(),
                accessible_timing_in_transit: attr(xml, a, "accessibleTimingInTransit").and_then(|v| v.parse().ok()),
            })
        });
    }
    if let Some(tree) = component(xml, entity, "SEntityTraversingTreeParams")
        && let Some(nodes) = child(xml, tree, "TraversingNodes")
    {
        for node in children(xml, nodes) {
            traverse(xml, node, &mut Vec::new(), None, &mut out.traversal_links);
        }
    }
    out
}

/// Walks one `SEntityTraversingNodeBase`: its type joins the path, its
/// `executeNodes` that create an interaction link become [`TraversalLink`]s,
/// and its `nextNodes` continue the walk.
fn traverse(xml: &CryXml, node: &CryXmlNode, via: &mut Vec<String>, port: Option<String>, out: &mut Vec<TraversalLink>) {
    let kind = child(xml, node, "entityTraversingType");
    let kind_name = kind.and_then(|k| attr(xml, k, "__type")).unwrap_or_default();
    via.push(kind_name.strip_prefix("SEntityTraversingNodeType").unwrap_or(kind_name).to_string());
    let port = kind.and_then(|k| attr(xml, k, "itemPortName")).filter(|s| !s.is_empty()).map(str::to_string).or(port);
    if let Some(execute) = child(xml, node, "executeNodes") {
        for exec in children(xml, execute) {
            if attr(xml, exec, "__type") != Some("SInteractionLinkExecuteCreateInteractionLinkNode") {
                continue;
            }
            out.push(TraversalLink {
                via: via.clone(),
                item_port_name: port.clone(),
                linked_interaction_name: attr(xml, exec, "linkedInteractionName").unwrap_or_default().to_string(),
                local_interaction_path: attr(xml, exec, "localInteraction").unwrap_or_default().to_string(),
                local_interaction: None,
            });
        }
    }
    if let Some(next) = child(xml, node, "nextNodes") {
        for n in children(xml, next) {
            traverse(xml, n, via, port.clone(), out);
        }
    }
    via.pop();
}
