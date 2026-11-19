//! Tests of `lift_panel_style`: stop names from the panel links, the door-pad rule, the label style read from IR
//! (synthetic documents with visibly invented values), and the C2's style from LIVE game data (skipped without it).

use serde_json::json;

use super::*;
use crate::pipeline::interior_interactions::ElevatorParams;
use crate::pipeline::lifts::LiftStop;
use crate::types::{InteractionLink, InteractionPoint, PlacementIdentity, PointInteraction, UiStateType};

const CARRIAGE: &str = "carriage-id";

fn lift(stops: usize) -> Lift {
    Lift {
        entity_id: CARRIAGE.into(),
        entity_name: None,
        entity_class_guid: None,
        entity_class: "TestCarriage".into(),
        params: ElevatorParams {
            max_speed: None,
            easing_distance: None,
            slow_down_at_bends: None,
            open_wait_time: None,
            queue_wait_time: None,
            idling_period: None,
            open_inner_door_interaction: None,
            open_inner_door_finished_interaction: None,
            close_all_doors_interaction: None,
            close_inner_door_finished_interaction: None,
        },
        grid_geometry: None,
        rest_node_id: "n0".into(),
        stops: (0..stops)
            .map(|index| LiftStop {
                index,
                node_entity_id: format!("n{index}"),
                node_entity_name: None,
                kind: "stop".into(),
                position: [0.0, 0.0, index as f32],
                landing_door_ids: Vec::new(),
                teleport_time_scale: None,
                gateway_timing_in_transit: None,
                gateway_interaction: None,
                accessibility: None,
            })
            .collect(),
        riders: Vec::new(),
    }
}

fn panel(links: &[(&str, &str, &str)], interactions: &[(&str, Option<&str>)]) -> InteriorPlacement {
    InteriorPlacement {
        mesh_index: 0,
        transform: [[0.0; 4]; 4],
        palette: None,
        ui_bindings: Vec::new(),
        identity: PlacementIdentity {
            entity_id: Some("panel".into()),
            interaction_links: links
                .iter()
                .map(|(source, target, slot)| InteractionLink {
                    source_slot: source.to_string(),
                    target_entity_id: target.to_string(),
                    target_slot: slot.to_string(),
                })
                .collect(),
            interaction_points: vec![InteractionPoint {
                index: 0,
                helper_name: "$proxy".into(),
                radius: None,
                helper_transform: None,
                interactions: interactions
                    .iter()
                    .map(|(name, display)| PointInteraction {
                        name: name.to_string(),
                        display_name_key: None,
                        display_name: display.map(str::to_string),
                        primary: false,
                    })
                    .collect(),
            }],
            ..Default::default()
        },
    }
}

#[test]
fn stop_names_come_from_the_panel_interactions_linked_to_each_stop() {
    let placements = [
        panel(
            &[
                ("GoLow", CARRIAGE, "From Elevator call 0"),
                ("GoHigh", CARRIAGE, "From Elevator call 2"),
                ("GoHigh", "hatch", "Open"),                     // a door link: not a stop
                ("GoMid", "other-carriage", "From Elevator call 1"), // another lift
                ("GoFar", CARRIAGE, "From Elevator call 9"),     // no such stop
                ("Unnamed", CARRIAGE, "From Elevator call 1"),   // no display name
            ],
            &[("GoLow", Some("Alpha Floor")), ("GoHigh", Some("Beta Floor")), ("GoMid", Some("Gamma")), ("GoFar", Some("Far")), ("Unnamed", None)],
        ),
        panel(&[("Call", CARRIAGE, "Request descent from 1")], &[("Call", Some("Call Lift"))]), // a call, not a floor pick
    ];
    let names = stop_display_names(&lift(3), &placements);
    assert_eq!(names, BTreeMap::from([(0, "Alpha Floor".to_string()), (2, "Beta Floor".to_string())]));
}

#[test]
fn a_door_pad_is_a_binding_whose_owner_mirrors_a_door() {
    let mut binding = UiBinding::default();
    assert!(!is_door_pad(&binding));
    binding.state_types.push(UiStateType { name: "PowerState".into(), states: vec![], default_state: String::new() });
    assert!(!is_door_pad(&binding));
    binding.state_types.push(UiStateType { name: "DoorOpenState".into(), states: vec![], default_state: String::new() });
    assert!(is_door_pad(&binding));
}

fn ir_node(id: u64, parent: Option<u64>, node_type: &str, active: bool, text: Option<&str>, symbol: Option<&str>) -> Value {
    let mut node = json!({ "id": id, "parent_id": parent, "node_type": node_type, "is_active": active });
    if let Some(text) = text {
        node["text_payload"] = json!({ "state": "resolved", "text": text });
        node["text_style"] = json!({
            "resolved_font_record": { "_RecordName_": "BuildingBlocks_FontStyle.TestFace", "_RecordValue_": { "font": symbol } },
            "font_size": { "kind": "fixed", "value": 50.0 },
            "letter_spacing": 2.5,
            "colour_token": "Bright",
        });
    }
    node
}

#[test]
fn label_style_is_the_first_active_label_inside_a_large_button() {
    let ir = json!({ "nodes": [
        ir_node(1, None, "display_widget", true, None, None),
        // Text outside any large button (a header): not the button label.
        ir_node(2, Some(1), "widget_text_field", true, Some("Header"), Some("$HeaderFace")),
        ir_node(3, Some(1), LARGE_BUTTON, true, None, None),
        // Inside a hidden branch of the button: skipped.
        ir_node(4, Some(3), "display_widget", false, None, None),
        ir_node(5, Some(4), "widget_text_field", true, Some("HIDDEN"), Some("$HiddenFace")),
        // The label.
        ir_node(6, Some(3), "widget_text_field", true, Some("PRESS"), Some("$TestFace")),
    ]});
    let style = label_style_from_ir(&ir).expect("label");
    assert_eq!(style.font_symbol, "$TestFace");
    assert_eq!(style.font_style.as_deref(), Some("BuildingBlocks_FontStyle.TestFace"));
    assert_eq!((style.font_size, style.letter_spacing), (50.0, 2.5));
    assert!(style.upper_case);
    assert_eq!(style.colour_token.as_deref(), Some("Bright"));

    let mixed = json!({ "nodes": [ir_node(3, None, LARGE_BUTTON, true, Some("Press"), Some("$TestFace"))] });
    assert!(!label_style_from_ir(&mixed).expect("label").upper_case, "a mixed-case label keeps its case");
    let none = json!({ "nodes": [ir_node(2, None, "widget_text_field", true, Some("Header"), Some("$HeaderFace"))] });
    assert!(label_style_from_ir(&none).is_none());
}

/// LIVE 4.10.2: the C2's lift panel style from its own data (the top deck's door pads, CRUS palette, shared fonts).
#[test]
fn c2_lift_panel_style_from_game_data() {
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
    let interiors = crate::pipeline::build_interiors_from_payloads(
        &db,
        &p4k,
        &[payload],
        false,
        0,
        &mut crate::socpak::light_component::LightResolver::new(&db),
    );
    let container = &interiors.containers[0];
    let lift = &container.lifts[0];

    let names = stop_display_names(lift, &container.placements);
    assert_eq!(
        names,
        BTreeMap::from([(0, "Exit Ship".to_string()), (1, "Lower Deck".to_string()), (2, "Top Deck".to_string())])
    );

    let (pad, binding) = container
        .placements
        .iter()
        .find_map(|p| p.ui_bindings.iter().find(|b| is_door_pad(b)).map(|b| (p, b)))
        .expect("a door pad on the top deck");
    let manufacturer = crate::ui_pipeline::root_manufacturer_id("CRUS_Starlifter_C2");
    let loc = crate::ui_pipeline::UiLocData::load(&p4k);
    let ship = crate::ui_pipeline::UiShipData::derive(&db, "CRUS_Starlifter_C2");
    let ir = crate::ui_pipeline::compile_ui_binding_ir_json(binding, None, &db, &p4k, 0, manufacturer.as_deref(), &loc, &ship)
        .expect("door pad IR");
    let ir: Value = serde_json::from_str(&ir).unwrap();
    // SB_LIFT_LABEL_DUMP=<dir>: the pad's IR and the label masks, for inspection.
    let dump = std::env::var("SB_LIFT_LABEL_DUMP").ok().map(std::path::PathBuf::from);
    if let Some(dir) = &dump {
        std::fs::write(dir.join("pad.ir.json"), serde_json::to_string_pretty(&ir).unwrap()).unwrap();
    }
    let label = label_style_from_ir(&ir).expect("the pad's button label");
    eprintln!("C2 door pad label style: {label:?}");
    assert!(label.upper_case, "the pads label in capitals");

    let fonts = crate::ui_pipeline::shared_ui_fonts(&p4k).expect("shared fonts");
    let palette = crate::ui_pipeline::manufacturer_colour_styles(&db, manufacturer.as_deref().unwrap());
    eprintln!("palette: {palette:?}");
    let source = StyleSource { entity_id: pad.identity.entity_id.as_deref(), binding, palette };
    let (value, files) = build(lift, &names, &label, &source, &fonts, |stem| format!("Data/UI/Generated/test/{stem}.png")).expect("style");
    eprintln!("{}", serde_json::to_string_pretty(&value).unwrap());
    assert_eq!(files.len(), 3);
    let texts: Vec<&str> = value["labels"].as_array().unwrap().iter().map(|l| l["text"].as_str().unwrap()).collect();
    assert_eq!(texts, ["EXIT SHIP", "LOWER DECK", "TOP DECK"]);
    assert!(value["palette"].as_array().unwrap().iter().any(|p| p["role"] == "Bright"));
    if let Some(dir) = &dump {
        for (path, png) in &files {
            std::fs::write(dir.join(path.rsplit('/').next().unwrap()), png).unwrap();
        }
    }
}
