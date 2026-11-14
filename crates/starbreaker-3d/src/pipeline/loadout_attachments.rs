//! The attachment rule of a resolved loadout tree, shared by every consumer
//! that walks it.
//!
//! [`walk_loadout_attachments`] visits each node of a `ResolvedNode` tree with
//! the scene attachment it resolves to ([`LoadoutAttachment`]): the parent
//! entity and helper/bone it hangs under, the `no_rotation` rule, and its item
//! port path from the root entity. A node that creates no scene nodes (no
//! geometry, no NMC) passes its own attachment on to its children, which then
//! hang under its parent. `collect_child_payload_specs` (scene children) and
//! the inventory-container walk use it, so both resolve the same parents.

use crate::types::ResolvedNode;

/// One loadout node and the attachment it resolves to.
pub(crate) struct LoadoutAttachment<'n> {
    pub(crate) node: &'n ResolvedNode,
    /// The scene entity whose geometry holds `parent_node_name`.
    pub(crate) parent_entity_name: String,
    /// The helper/bone (or item port) on that entity the node attaches to.
    pub(crate) parent_node_name: String,
    pub(crate) no_rotation: bool,
    /// Item-port names from the root entity to this node; `None` when a node on
    /// the way is not a loadout item.
    pub(crate) port_path: Option<Vec<String>>,
    /// `port_path` of the scene entity named by `parent_entity_name` (`[]` for
    /// the root entity).
    pub(crate) parent_port_path: Option<Vec<String>>,
}

/// Where a walk starts: the parent entity and the port path that leads to it.
#[derive(Clone, Copy)]
pub(crate) struct WalkOrigin<'a> {
    pub(crate) parent_entity_name: &'a str,
    pub(crate) parent_port_path: Option<&'a [String]>,
}

impl<'a> WalkOrigin<'a> {
    /// A walk over the root entity's own loadout children.
    pub(crate) fn root(parent_entity_name: &'a str) -> Self {
        Self { parent_entity_name, parent_port_path: Some(&[]) }
    }
}

fn extend_path(prefix: Option<&[String]>, port: Option<&String>) -> Option<Vec<String>> {
    let mut path = prefix?.to_vec();
    path.push(port?.clone());
    Some(path)
}

/// Visit every node of `children` (depth first, parents before children) with
/// its resolved attachment. `override_attachment` is the attachment a
/// node-less parent passes on (`collect_child_payload_specs`' contract).
pub(crate) fn walk_loadout_attachments<'n>(
    children: &'n [ResolvedNode],
    origin: WalkOrigin<'_>,
    override_attachment: Option<(&str, bool)>,
    node_path_prefix: Option<&[String]>,
    visit: &mut dyn FnMut(LoadoutAttachment<'n>),
) {
    for child in children {
        let (attach_name, no_rotation) = match override_attachment {
            Some((name, parent_no_rot)) => (name.to_string(), child.no_rotation || parent_no_rot),
            None => (child.attachment_name.clone(), child.no_rotation),
        };
        let port_path = extend_path(node_path_prefix, child.item_port_name.as_ref());
        visit(LoadoutAttachment {
            node: child,
            parent_entity_name: origin.parent_entity_name.to_string(),
            parent_node_name: attach_name,
            no_rotation,
            port_path: port_path.clone(),
            parent_port_path: origin.parent_port_path.map(<[String]>::to_vec),
        });
        let child_creates_nodes = child.has_geometry || child.nmc.is_some();
        if child_creates_nodes {
            walk_loadout_attachments(
                &child.children,
                WalkOrigin { parent_entity_name: &child.entity_name, parent_port_path: port_path.as_deref() },
                None,
                port_path.as_deref(),
                visit,
            );
        } else {
            walk_loadout_attachments(
                &child.children,
                origin,
                Some((&child.attachment_name, child.no_rotation)),
                port_path.as_deref(),
                visit,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> starbreaker_datacore::types::Record {
        starbreaker_datacore::types::Record {
            name_offset: starbreaker_datacore::types::StringId2(-1),
            file_name_offset: starbreaker_datacore::types::StringId(0),
            tag_offset: starbreaker_datacore::types::StringId2(-1),
            struct_index: 0,
            id: starbreaker_common::CigGuid::EMPTY,
            instance_index: 0,
            struct_size: 0,
        }
    }

    fn node(name: &str, port: Option<&str>, attach: &str, geometry: bool, children: Vec<ResolvedNode>) -> ResolvedNode {
        ResolvedNode {
            entity_name: name.into(),
            attachment_name: attach.into(),
            item_port_name: port.map(str::to_string),
            no_rotation: false,
            offset_position: [0.0; 3],
            offset_rotation: [0.0; 3],
            detach_direction: [0.0; 3],
            port_flags: String::new(),
            nmc: None,
            bones: Vec::new(),
            has_geometry: geometry,
            record: record(),
            geometry_path: None,
            material_path: None,
            allows_child_object_containers: true,
            children,
        }
    }

    fn walk(children: &[ResolvedNode]) -> Vec<(String, String, String, Option<Vec<String>>, Option<Vec<String>>)> {
        let mut out = Vec::new();
        walk_loadout_attachments(children, WalkOrigin::root("root"), None, Some(&[]), &mut |a| {
            out.push((
                a.node.entity_name.clone(),
                a.parent_entity_name,
                a.parent_node_name,
                a.port_path,
                a.parent_port_path,
            ))
        });
        out
    }

    fn path(parts: &[&str]) -> Option<Vec<String>> {
        Some(parts.iter().map(|p| p.to_string()).collect())
    }

    #[test]
    fn sibling_items_of_one_class_get_distinct_port_paths() {
        let plate = |port: &str| {
            node("Plate", Some(port), &format!("helper_{port}"), true, vec![node(
                "Grid", Some("Hardpoint_Cargo"), "hardpoint_cargo_grid", false, Vec::new(),
            )])
        };
        let visits = walk(&[plate("port_left"), plate("port_right")]);
        assert_eq!(visits.len(), 4);
        assert_eq!(visits[0].3, path(&["port_left"]));
        assert_eq!(visits[0].4, path(&[]), "the root entity's port path is empty");
        assert_eq!(visits[1].0, "Grid");
        assert_eq!(visits[1].1, "Plate");
        assert_eq!(visits[1].2, "hardpoint_cargo_grid");
        assert_eq!(visits[1].3, path(&["port_left", "Hardpoint_Cargo"]));
        assert_eq!(visits[1].4, path(&["port_left"]));
        assert_eq!(visits[3].3, path(&["port_right", "Hardpoint_Cargo"]));
        assert_eq!(visits[3].4, path(&["port_right"]));
    }

    #[test]
    fn a_nodeless_parent_passes_its_attachment_and_anchor_on() {
        let proxy = node("proxy", Some("port_proxy"), "hardpoint_proxy", false, vec![node(
            "item", Some("port_item"), "hardpoint_item", true, Vec::new(),
        )]);
        let visits = walk(&[proxy]);
        assert_eq!(visits[1].1, "root");
        assert_eq!(visits[1].2, "hardpoint_proxy");
        assert_eq!(visits[1].3, path(&["port_proxy", "port_item"]));
        assert_eq!(visits[1].4, path(&[]), "it hangs under the root entity's helper");
    }

    #[test]
    fn a_node_without_an_item_port_has_no_port_path() {
        let part = node("xml_part", None, "part", true, vec![node(
            "wheel", Some("port_wheel"), "wheel", true, Vec::new(),
        )]);
        let visits = walk(&[part]);
        assert_eq!(visits[0].3, None);
        assert_eq!(visits[1].3, None);
        assert_eq!(visits[1].4, None);
    }
}
