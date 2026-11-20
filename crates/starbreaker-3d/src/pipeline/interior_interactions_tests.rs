//! Class facts of interior entities (`interior_interactions`): synthetic component values for each rule, and the
//! C2 doors and front hatch links from the game's own data (LIVE 4.10.2; skipped without a Data.p4k).
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
                    ("AngleConstraint", object("SInteractionPointAngleConstraint", vec![
                        ("angleRange", Value::Float(150.0)),
                        ("viewAngleLimit", Value::Float(100.0)),
                    ])),
                    (
                        "Interactions",
                        Value::Array(vec![
                            object("SInteractionParams", vec![("Interaction", shared("RequestFloor1")), ("DisplayName", Value::Locale("@innerthought_lift_top_deck")), ("IsPrimary", Value::Bool(true))]),
                            object("SInteractionParams", vec![
                                ("Interaction", object("SSharedInteractionParams", vec![
                                    ("Name", Value::String("RequestFloor0")),
                                    ("DisplayName", Value::Locale("@fixture_shared_prompt")),
                                ])),
                                ("DisplayName", Value::Locale("@LOC_EMPTY")),
                                ("IsPrimary", Value::Bool(false)),
                            ]),
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
    assert_eq!(points[0].angle_range, Some(150.0));
    assert_eq!(points[0].view_angle_limit, Some(100.0));
    let names: Vec<_> = points[0].interactions.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["RequestFloor1", "RequestFloor0"], "an interaction without a target is not offered");
    assert!(points[0].interactions[0].primary);
    assert_eq!(points[0].interactions[0].display_name_key.as_deref(), Some("@innerthought_lift_top_deck"));
    assert_eq!(points[0].interactions[1].display_name_key.as_deref(), Some("@fixture_shared_prompt"));
}

/// Synthetic `SCItemDoorParams` (structure only): the rest state and the polymorphic animation struct's type and
/// tags are read; what is not authored stays `None`.
#[test]
fn door_reads_its_rest_state_and_animation_params() {
    let door = door_timing(&object(
        "SCItemDoorParams",
        vec![
            ("IsOpened", Value::Bool(true)),
            ("IsLocked", Value::Bool(false)),
            ("AnimationParams", object("SCItemDoorTestParams", vec![("TransitionTag", Value::String("T")), ("OpenTag", Value::String(""))])),
        ],
    ));
    assert_eq!(door.is_opened, Some(true));
    assert_eq!(door.is_locked, Some(false));
    assert_eq!(door.animation_params.as_deref(), Some("SCItemDoorTestParams"));
    assert_eq!(door.transition_tag.as_deref(), Some("T"));
    assert_eq!(door.open_tag, None, "an empty tag is not a tag");
    assert_eq!(door.close_tag, None);
    assert_eq!(door.animation_speed, None, "unauthored stays None");
    let bare = door_timing(&object("SCItemDoorParams", vec![("AnimationParams", Value::Null)]));
    assert_eq!((bare.is_opened, bare.animation_params), (None, None));
}

/// An instance override replaces each authored value; an `AnimationParams` of another type replaces the class's
/// struct with its tags.
#[test]
fn door_overrides_replace_values_and_a_retyped_animation_struct() {
    let class = DoorTiming {
        is_opened: Some(false),
        animation_params: Some("A".into()),
        open_tag: Some("O".into()),
        close_tag: Some("C".into()),
        ..Default::default()
    };
    let same_type = EntityOverrides {
        door_is_opened: Some(true),
        door_animation: Some(crate::types::DoorAnimationOverride { type_name: Some("A".into()), open_tag: Some("O2".into()), ..Default::default() }),
        ..Default::default()
    };
    let door = door_with_overrides(class.clone(), &same_type);
    assert_eq!(door.is_opened, Some(true));
    assert_eq!((door.open_tag.as_deref(), door.close_tag.as_deref()), (Some("O2"), Some("C")));
    let retyped = EntityOverrides {
        door_animation: Some(crate::types::DoorAnimationOverride { type_name: Some("B".into()), transition_tag: Some("T".into()), ..Default::default() }),
        ..Default::default()
    };
    let door = door_with_overrides(class, &retyped);
    assert_eq!(door.animation_params.as_deref(), Some("B"));
    assert_eq!((door.transition_tag.as_deref(), door.open_tag.as_deref(), door.close_tag.as_deref()), (Some("T"), None, None));
    assert_eq!(door.is_opened, Some(false), "not overridden");
}

/// `vehicleItemPortInteractionLinks` resolve their source interaction by name, in authored order; an entry
/// missing a part is skipped.
#[test]
fn item_port_links_resolve_in_authored_order() {
    let link = |source: Value<'static>, port: &'static str, target: &'static str| {
        object(
            "SVehicleItemportInteractionLink",
            vec![("sourceInteraction", source), ("itemPort", Value::String(port)), ("targetInteraction", Value::String(target))],
        )
    };
    let component = object(
        "SInteractionLinkParams",
        vec![(
            "vehicleItemPortInteractionLinks",
            Value::Array(vec![link(shared("Go"), "port_b", "In"), link(shared("Go"), "port_a", "In"), link(Value::Null, "port_c", "In")]),
        )],
    );
    let links = item_port_links(&component);
    let triples: Vec<(&str, &str, &str)> =
        links.iter().map(|l| (l.source_slot.as_str(), l.target_port.as_str(), l.target_slot.as_str())).collect();
    assert_eq!(triples, [("Go", "port_b", "In"), ("Go", "port_a", "In")]);
}

/// LIVE 4.10.2: the C2's exterior doors' rest state and animation struct, and the front bottom hatch forwarding its
/// `Open` / `Close` to the doors and ramp on the vehicle's ports (skipped without a Data.p4k).
#[test]
fn c2_door_state_and_item_port_links_from_game_data() {
    let Ok(found) = starbreaker_common::discover::find_p4k() else {
        eprintln!("skipping: no Data.p4k installed");
        return;
    };
    let p4k = starbreaker_p4k::MappedP4k::open(found.path).unwrap();
    let dcb = p4k.read_file("Data\\Game2.dcb").or_else(|_| p4k.read_file("Data\\Game.dcb")).unwrap();
    let db = Database::from_bytes(&dcb).unwrap();
    let idx = starbreaker_datacore::loadout::EntityIndex::new(&db);
    let door = |name: &str| class_door(&db, idx.find_record(name).unwrap_or_else(|| panic!("{name}"))).expect(name);

    let rear = door("Door_RN_RoomConnector_Breachable_OpenReverse_crus_Starlifter_Rear_Ramp");
    assert_eq!(rear.is_opened, Some(false));
    assert_eq!(rear.is_locked, Some(false));
    assert_eq!(rear.animation_params.as_deref(), Some("SCItemDoorSingleProceduralParams"));
    assert_eq!(rear.transition_tag.as_deref(), Some("Open"));
    assert_eq!((rear.open_tag.as_deref(), rear.close_tag.as_deref()), (None, None));
    for name in [
        "Door_Ship_Exterior_CRUS_Starlifter_Front_Door_Left",
        "Door_Ship_Exterior_CRUS_Starlifter_Front_Door_Right",
        "Door_Ship_Exterior_CRUS_Starlifter_Door_Bottom",
        "Door_Ship_Exterior_CRUS_Starlifter_Front_Ramp",
    ] {
        let front = door(name);
        assert_eq!(front.is_opened, Some(false), "{name}");
        assert_eq!(front.animation_params.as_deref(), Some("SCItemDoorSimpleOpenCloseParams"), "{name}");
        assert_eq!((front.open_tag.as_deref(), front.close_tag.as_deref()), (Some("Open"), Some("Close")), "{name}");
        assert_eq!(front.transition_tag, None, "{name}");
    }

    let bottom = idx.find_record("Door_Ship_Exterior_CRUS_Starlifter_Door_Bottom").unwrap();
    let links: Vec<(String, String, String)> = class_item_port_links(&db, bottom)
        .into_iter()
        .map(|l| (l.source_slot, l.target_port, l.target_slot))
        .collect();
    let expected: Vec<(String, String, String)> = [
        ("hardpoint_front_door_right", "Open"),
        ("hardpoint_front_door_right", "Close"),
        ("hardpoint_front_door_left", "Open"),
        ("hardpoint_front_door_left", "Close"),
        ("hardpoint_front_ramp", "Open"),
        ("hardpoint_front_ramp", "Close"),
    ]
    .into_iter()
    .map(|(port, slot)| (slot.to_string(), port.to_string(), slot.to_string()))
    .collect();
    assert_eq!(links, expected, "the hatch's own Open/Close forward to the same-named interactions");
    let left = idx.find_record("Door_Ship_Exterior_CRUS_Starlifter_Front_Door_Left").unwrap();
    assert!(class_item_port_links(&db, left).is_empty());
}

#[test]
fn shared_index_reads_the_authored_path() {
    assert_eq!(shared_index("InteractableComponent.Interactable.SharedInteractions[5]"), Some(5));
    assert_eq!(shared_index("Interactable.InteractionPoints[1]"), None);
}
