//! Lift chain rule (synthetic graphs) and the C2 Hercules lift resolved from
//! the game's own data (`top_deck_c2.socpak` + DataCore, LIVE 4.10.2; skipped
//! without a Data.p4k).
use super::*;

fn graph(edges: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
    edges.iter().map(|(from, to)| (from.to_string(), to.iter().map(|s| s.to_string()).collect())).collect()
}

#[test]
fn the_chain_starts_at_the_node_nothing_links() {
    // The C2: ground -> hold -> top; the carriage rests at the hold.
    let next = graph(&[("top", &[]), ("hold", &["top"]), ("ground", &["hold"])]);
    assert_eq!(order_stops("hold", &next), Ok(vec!["ground".to_string(), "hold".into(), "top".into()]));
    assert_eq!(order_stops("top", &next).unwrap()[0], "ground");
}

#[test]
fn a_branched_graph_is_rejected() {
    let fork = graph(&[("a", &["b", "c"]), ("b", &[]), ("c", &[])]);
    assert_eq!(order_stops("a", &fork), Err(ChainError::Branched("a".into())));
    let join = graph(&[("a", &["c"]), ("b", &["c"]), ("c", &[])]);
    assert_eq!(order_stops("c", &join), Err(ChainError::Branched("c".into())));
}

#[test]
fn a_cyclic_graph_is_rejected() {
    let cycle = graph(&[("a", &["b"]), ("b", &["c"]), ("c", &["a"])]);
    assert_eq!(order_stops("a", &cycle), Err(ChainError::NoSingleHead(0)));
}

#[test]
fn nodes_of_another_lift_are_not_this_lifts_stops() {
    let two = graph(&[("a", &["b"]), ("b", &[]), ("x", &["y"]), ("y", &[])]);
    assert_eq!(order_stops("b", &two), Ok(vec!["a".to_string(), "b".into()]));
}

const PLATFORM: &str = "42d29922-9f0a-b30b-33a9-05929f61d2bb";
const FLOOR_000: &str = "45276da2-b394-114c-bd1d-65441c12c7a0";
const FLOOR_001: &str = "4bed6723-a0f2-97a7-4123-48d75b9a98ab";
const FLOOR_003: &str = "4418aa71-e2b5-cff1-eb52-4cc6e9958890";
const HOLD_DOOR: &str = "4376aaa1-530b-5d53-88fb-a81eea7ca6b2";
const TOP_DOOR: &str = "4edb736f-b03a-ee7b-e220-1df68f498194";
const INNER_PANEL: &str = "4e867e5a-7436-4974-761c-883613ff8998";
const INNER_GATE: &str = "4b931226-afcb-d0de-0a46-b7760215b5b4";
const GROUND_BUTTONS: [&str; 2] = ["49bd738e-4318-f2b7-487a-8fee884c3a93", "472581fe-5b39-a869-4a7c-7d22444152bd"];

/// Real-P4K regression (WP-P5): the C2's lift, stops, riders, ground buttons and floor panel as the exporter
/// resolves them from `top_deck_c2.socpak` and DataCore.
#[test]
fn c2_lift_resolves_from_game_data() {
    let Ok(found) = starbreaker_common::discover::find_p4k() else {
        eprintln!("skipping: no Data.p4k installed");
        return;
    };
    let p4k = starbreaker_p4k::MappedP4k::open(found.path).unwrap();
    let dcb = p4k.read_file("Data\\Game2.dcb").or_else(|_| p4k.read_file("Data\\Game.dcb")).unwrap();
    let db = starbreaker_datacore::database::Database::from_bytes(&dcb).unwrap();
    let identity = glam::Mat4::IDENTITY.to_cols_array_2d();
    let payload = crate::socpak::load_interior_from_socpak(
        &p4k,
        "Data\\ObjectContainers\\Ships\\CRUS\\Starlifter\\top_deck_c2.socpak",
        identity,
        identity,
        &[],
    )
    .expect("top_deck_c2.socpak");
    let interiors = super::super::build_interiors_from_payloads(
        &db,
        &p4k,
        &[payload],
        false,
        0,
        &mut crate::socpak::light_component::LightResolver::new(&db),
    );
    let container = &interiors.containers[0];

    assert_eq!(container.lifts.len(), 1);
    let lift = &container.lifts[0];
    assert_eq!(lift.entity_id, PLATFORM);
    assert_eq!(lift.entity_class, "CRUS_Hercules_Lift_Platform");
    assert_eq!(lift.rest_node_id, FLOOR_001);
    let stops: Vec<&str> = lift.stops.iter().map(|s| s.node_entity_id.as_str()).collect();
    assert_eq!(stops, [FLOOR_003, FLOOR_001, FLOOR_000], "ground, hold, top");
    let z: Vec<f32> = lift.stops.iter().map(|s| s.position[2]).collect();
    assert!((z[0] + 14.65).abs() < 1e-4 && (z[1] + 7.875).abs() < 1e-4 && (z[2] + 0.5).abs() < 1e-4, "{z:?}");
    assert!(lift.stops.iter().all(|s| s.kind == "stop" && s.teleport_time_scale == Some(1.0)));
    assert_eq!(lift.stops[0].landing_door_ids, Vec::<String>::new());
    assert_eq!(lift.stops[1].landing_door_ids, [HOLD_DOOR]);
    assert_eq!(lift.stops[2].landing_door_ids, [TOP_DOOR]);
    assert_eq!(lift.stops[0].accessibility.as_ref().map(|a| a.accessible_timing_in_transit), Some(Some(0.5)));
    assert_eq!(lift.stops[0].gateway_timing_in_transit, Some(0.5));
    let mut riders = vec![INNER_GATE.to_string(), INNER_PANEL.to_string()];
    riders.sort();
    assert_eq!(lift.riders, riders);
    let p = &lift.params;
    assert_eq!((p.max_speed, p.easing_distance, p.open_wait_time, p.queue_wait_time, p.idling_period), (Some(3.5), Some(5.1), Some(4.0), Some(5.0), Some(120.0)));
    assert_eq!(p.slow_down_at_bends, Some(false));
    assert_eq!(p.open_inner_door_interaction.as_deref(), Some("OpenInnerDoor"));
    assert_eq!(p.close_all_doors_interaction.as_deref(), Some("CloseAllDoors"));
    assert!(lift.grid_geometry.as_deref().is_some_and(|g| g.ends_with("LocalGrid_Lift_Platform.cgf")), "{:?}", lift.grid_geometry);

    // Geometry-less entities: the three stops and both ground buttons (helpers only), with their links.
    let kinds: HashMap<&str, &str> = container.entities.iter().map(|e| (e.entity_id.as_str(), e.kind)).collect();
    for node in [FLOOR_000, FLOOR_001, FLOOR_003] {
        assert_eq!(kinds.get(node), Some(&"path_node"), "{node}");
    }
    for button in GROUND_BUTTONS {
        assert_eq!(kinds.get(button), Some(&"interaction"), "{button}");
        let entity = container.entities.iter().find(|e| e.entity_id == button).unwrap();
        let slots: Vec<(&str, &str)> =
            entity.interaction_links.iter().map(|l| (l.source_slot.as_str(), l.target_slot.as_str())).collect();
        assert_eq!(slots, [("Descend", "DescentRequest"), ("Descend", "Open")]);
        let point = &entity.interaction_points[0];
        assert_eq!(point.interactions[0].name, "Descend");
        assert_eq!(point.interactions[0].display_name.as_deref(), Some("Call Elevator"));
        assert!(point.helper_transform.is_some(), "the helper frame comes from the helper-only geometry: {}", point.helper_name);
    }
    let ground = container.entities.iter().find(|e| e.entity_id == FLOOR_003).unwrap();
    assert_eq!(ground.traversal_links[0].local_interaction.as_deref(), Some("GatewayOpen"));
    assert_eq!(ground.traversal_links[0].linked_interaction_name, "OpenSelf");

    // Placements: the floor panel's three choices, door timing, and one carriage (no item-port duplicate).
    let placement = |id: &str| container.placements.iter().find(|p| p.identity.entity_id.as_deref() == Some(id)).unwrap();
    let panel = placement(INNER_PANEL);
    let choices: Vec<(&str, Option<&str>, bool)> = panel.identity.interaction_points[0]
        .interactions
        .iter()
        .map(|i| (i.name.as_str(), i.display_name.as_deref(), i.primary))
        .collect();
    assert_eq!(
        choices,
        [("RequestFloor1", Some("Top Deck"), true), ("RequestFloor0", Some("Lower Deck"), false), ("RequestSurface", Some("Exit Ship"), false)]
    );
    assert_eq!(panel.identity.interaction_links.len(), 4);
    assert_eq!(panel.identity.parent.as_ref().map(|p| p.parent_entity_id.as_str()), Some(PLATFORM));
    let hold_door = placement(HOLD_DOOR).identity.door.clone().expect("door timing");
    assert_eq!((hold_door.animation_speed, hold_door.use_auto_close_delay), (Some(1.0), Some(false)));
    // The belly hatch instance authors its point 0 with no interactions: that point is not exported.
    let hatch = placement("405af200-ec86-04b3-b5d2-4eb715112589");
    assert!(hatch.identity.interaction_points.iter().all(|p| p.index != 0), "{:?}", hatch.identity.interaction_points);
    assert!(placement(PLATFORM).identity.door.is_none());
    let carriages = container.placements.iter().filter(|p| p.identity.entity_class_guid.is_none() && {
        let mesh = &interiors.unique_cgfs[p.mesh_index];
        mesh.cgf_path.ends_with("CRUS_Starlifter_Elevator_Platform.cga")
    });
    assert_eq!(carriages.count(), 0, "the root item port of the carriage is the carriage, not a second one");
}
