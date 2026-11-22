//! Hovered-node tests: the registry's hovered node (the widget under the player's cursor, which the engine tags with
//! its internal hover tag) receives that tag during state-tag resolution, so style entries gated on it apply.

use serde_json::json;

use crate::defaults::DefaultValueRegistry;

/// Synthetic ids (not game tags).
const HOVER: &str = "11111111-2222-3333-4444-555555555555";

fn two_buttons() -> crate::bb_scene::BbScene {
    crate::bb_scene::parse_bb_canvas(&json!({
        "_RecordValue_": {
            "_Type_": "BuildingBlocks_Canvas",
            "size": {"x": 100.0, "y": 100.0},
            "scene": [
                {"_Pointer_": "ptr:1", "_Type_": "BuildingBlocks_DisplayWidget", "name": "Root"},
                {"_Pointer_": "ptr:2", "_Type_": "BuildingBlocks_DisplayWidget", "name": "ButtonA", "parent": "_PointsTo_:ptr:1"},
                {"_Pointer_": "ptr:3", "_Type_": "BuildingBlocks_DisplayWidget", "name": "ButtonB", "parent": "_PointsTo_:ptr:1"}
            ]
        }
    }))
    .expect("synthetic canvas parses")
}

fn tags_of<'a>(scene: &'a crate::bb_scene::BbScene, name: &str) -> &'a [String] {
    &scene.nodes.values().find(|n| n.name == name).expect("node exists").style_tag_uuids
}

#[test]
fn the_hovered_node_gets_the_hover_tag_and_no_other_node_does() {
    let mut scene = two_buttons();
    let mut defaults = DefaultValueRegistry::default();
    defaults.set_hovered("ButtonB", HOVER);
    super::resolve_state_tags_into_scene(&mut scene, &defaults);
    assert!(tags_of(&scene, "ButtonB").iter().any(|t| t == HOVER), "ButtonB is hovered");
    assert!(!tags_of(&scene, "ButtonA").iter().any(|t| t == HOVER), "ButtonA is not");
    assert!(!tags_of(&scene, "Root").iter().any(|t| t == HOVER), "nor its parent");
}

#[test]
fn without_a_hovered_node_no_hover_tag_is_added() {
    let mut scene = two_buttons();
    super::resolve_state_tags_into_scene(&mut scene, &DefaultValueRegistry::default());
    assert!(scene.nodes.values().all(|n| !n.style_tag_uuids.iter().any(|t| t == HOVER)));
}

#[test]
fn the_hover_tag_is_added_once() {
    let mut scene = two_buttons();
    let mut defaults = DefaultValueRegistry::default();
    defaults.set_hovered("ButtonA", HOVER);
    super::resolve_state_tags_into_scene(&mut scene, &defaults);
    super::resolve_state_tags_into_scene(&mut scene, &defaults); // a parent level resolves the merged scene again
    assert_eq!(tags_of(&scene, "ButtonA").iter().filter(|t| *t == HOVER).count(), 1);
}
