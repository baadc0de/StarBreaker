//! Item-port loadouts given by an XML file (scviewer C2 fixes): an entity class whose default loadout is
//! `SItemPortLoadoutXMLParams` names a `Scripts/Loadouts/…` file in the P4k, which DataCore cannot read. The loadout
//! walk records its `loadoutPath` ([`LoadoutNode::loadout_xml_path`]); this module reads the file
//! (`character::loadout::parse_loadout`, the engine's `<Loadout><Items><Item portName itemName>` format) and resolves
//! its items against the node's item ports exactly as a manual loadout (`resolve_manual_loadout_indexed`).
//!
//! - [`expand_xml_loadouts`]: fills the children of every node (recursively) that has a loadout file and no
//!   children.
//! - [`expanded_children`]: a loadout tree's root children with their XML loadouts expanded (borrowed unchanged when
//!   the tree names no file).
//! - [`manual_entries`]: the file's items as manual loadout entries (pure).

use std::borrow::Cow;

use starbreaker_datacore::database::Database;
use starbreaker_datacore::loadout::{EntityIndex, LoadoutNode, LoadoutTree, ManualLoadoutEntry};
use starbreaker_p4k::MappedP4k;

use crate::character::loadout::LoadoutEntry;

/// Nested loadouts stack at most this deep (as DataCore's walk).
const MAX_DEPTH: usize = 8;

/// The items of a loadout file as manual entries: an item without an `itemName` attaches nothing; one with nested
/// items gives them as its loadout, else its class's own loadout applies.
pub(crate) fn manual_entries(entries: &[LoadoutEntry]) -> Vec<ManualLoadoutEntry> {
    entries
        .iter()
        .filter(|e| !e.item.is_empty())
        .map(|e| ManualLoadoutEntry {
            item_port_name: e.port.clone(),
            entity_class_name: e.item.clone(),
            entity_class_guid: None,
            children: (!e.children.is_empty()).then(|| manual_entries(&e.children)),
        })
        .collect()
}

/// Whether any node of `nodes` (recursively) names a loadout file it has not expanded.
pub(crate) fn names_xml_loadout(nodes: &[LoadoutNode]) -> bool {
    nodes.iter().any(|n| (n.loadout_xml_path.is_some() && n.children.is_empty()) || names_xml_loadout(&n.children))
}

/// The entries of loadout file `path` (`Scripts/Loadouts/…`); `None` when it does not read or parse (logged).
fn read_loadout_file(p4k: &MappedP4k, path: &str) -> Option<Vec<LoadoutEntry>> {
    let p4k_path = super::datacore_path_to_p4k(path);
    let bytes = p4k
        .entry_case_insensitive(&p4k_path)
        .and_then(|entry| p4k.read(entry).ok())
        .or_else(|| p4k.read_file(&p4k_path).ok());
    let Some(bytes) = bytes else {
        log::warn!("loadout file {p4k_path}: not in the archive");
        return None;
    };
    crate::character::loadout::parse_loadout(&bytes)
        .map_err(|e| log::warn!("loadout file {p4k_path}: {e}"))
        .ok()
}

/// Fill the children of every node of `nodes` that names a loadout file and has none, then walk on.
pub(crate) fn expand_xml_loadouts(idx: &EntityIndex, p4k: &MappedP4k, nodes: &mut [LoadoutNode], depth: usize) {
    if depth >= MAX_DEPTH {
        return;
    }
    for node in nodes.iter_mut() {
        if node.children.is_empty()
            && let Some(path) = node.loadout_xml_path.clone()
            && let Some(entries) = read_loadout_file(p4k, &path)
        {
            let entries = manual_entries(&entries);
            node.children = starbreaker_datacore::loadout::resolve_manual_loadout_indexed(idx, &node.record, &entries);
            log::debug!("{}: {} children from loadout file {path}", node.entity_name, node.children.len());
        }
        expand_xml_loadouts(idx, p4k, &mut node.children, depth + 1);
    }
}

/// `tree`'s root children with every loadout file expanded; borrowed unchanged when none is named.
pub(crate) fn expanded_children<'t>(db: &Database, p4k: &MappedP4k, tree: &'t LoadoutTree) -> Cow<'t, [LoadoutNode]> {
    if !names_xml_loadout(&tree.root.children) {
        return Cow::Borrowed(&tree.root.children);
    }
    let idx = EntityIndex::new(db);
    let mut children = tree.root.children.clone();
    expand_xml_loadouts(&idx, p4k, &mut children, 0);
    Cow::Owned(children)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(port: &str, item: &str, children: Vec<LoadoutEntry>) -> LoadoutEntry {
        LoadoutEntry { port: port.into(), item: item.into(), children }
    }

    /// Synthetic loadout file items (made-up names): an empty item attaches nothing; nested items become the entry's
    /// own loadout, none leaves the class's.
    #[test]
    fn file_items_become_manual_entries() {
        let entries = manual_entries(&[
            entry("port_a", "Item_A", vec![entry("port_a1", "Item_A1", vec![])]),
            entry("port_b", "", vec![]),
            entry("port_c", "Item_C", vec![]),
        ]);
        assert_eq!(entries.len(), 2);
        assert_eq!((entries[0].item_port_name.as_str(), entries[0].entity_class_name.as_str()), ("port_a", "Item_A"));
        assert_eq!(entries[0].children.as_ref().map(Vec::len), Some(1));
        assert!(entries[1].children.is_none(), "no nested items: the class's own loadout applies");
    }

    /// LIVE 4.10.2: the C2's front bottom hatch takes its two door panels from its loadout file, at its own item
    /// ports; the rear ramp keeps its two (its class loadout), and no parent lists a port twice (skipped without a
    /// Data.p4k).
    #[test]
    fn c2_front_door_panels_come_from_the_hatch_loadout_file() {
        let Ok(found) = starbreaker_common::discover::find_p4k() else {
            eprintln!("skipping: no Data.p4k installed");
            return;
        };
        let p4k = MappedP4k::open(found.path).unwrap();
        let dcb = p4k.read_file("Data\\Game2.dcb").or_else(|_| p4k.read_file("Data\\Game.dcb")).unwrap();
        let db = Database::from_bytes(&dcb).unwrap();
        let idx = EntityIndex::new(&db);
        let tree = starbreaker_datacore::loadout::resolve_loadout_indexed(&idx, idx.find_record("CRUS_Starlifter_C2").unwrap());
        let children = expanded_children(&db, &p4k, &tree);
        let at = |port: &str| children.iter().find(|c| c.item_port_name == port).unwrap_or_else(|| panic!("{port}"));
        let panels = |node: &LoadoutNode| -> Vec<(String, String)> {
            node.children.iter().map(|c| (c.item_port_name.clone(), c.entity_name.clone())).collect()
        };
        let pair = |port: &str, item: &str| (port.to_string(), item.to_string());

        let bottom = at("hardpoint_front_door_bottom");
        assert_eq!(
            bottom.loadout_xml_path.as_deref(),
            Some("Scripts/Loadouts/Objects/Doors/DoorExteriorLoadoutCRUSStarlifterDoorBottom.xml")
        );
        assert_eq!(
            panels(bottom),
            [
                pair("IP_Interior", "ControlPanel_Screen_DoorControl_Starlifter_Interior_Button"),
                pair("IP_Exterior", "ControlPanel_Screen_DoorControl_Starlifter_Exterior_Button"),
            ]
        );
        assert_eq!(
            panels(at("hardpoint_rear_ramp")),
            [
                pair("IP_Interior", "ControlPanel_Screen_DoorControl_Starlifter_Interior_Button"),
                pair("IP_Exterior", "ControlPanel_Screen_DoorControl_Starlifter_Interior_Button"),
            ],
            "unchanged"
        );
        assert!(at("hardpoint_front_ramp").children.is_empty(), "its class loadout names no items");

        fn no_port_twice(nodes: &[LoadoutNode], path: &str) {
            let mut seen = std::collections::HashSet::new();
            for node in nodes {
                assert!(seen.insert(node.item_port_name.to_ascii_lowercase()), "{path}/{} listed twice", node.item_port_name);
                no_port_twice(&node.children, &format!("{path}/{}", node.item_port_name));
            }
        }
        no_port_twice(&children, "CRUS_Starlifter_C2");
    }
}
