//! Light groups, light-control chipsets and light panels of the C2 resolved from the game's own data (scviewer C2
//! fixes; `hold_c2` / `top_deck_c2` / `modular_c2` socpaks + DataCore, LIVE 4.10.2; skipped without a Data.p4k).
use super::*;

const LG_HOLD: &str = "4a043d0e-eb7f-5b77-06a6-8be5e93ea681";
const HOLD_CHIPSET: &str = "4f2e8470-2451-2c22-be08-d5644f32a3a1";
const TOP_DECK_CHIPSET: &str = "488a81bc-a927-c8d8-9d7d-52eb7bbe33b1";
const TOP_DECK_DEFCON: &str = "407080ce-dd65-a82c-1c54-e1e3afc61297";

fn containers(names: &[&str]) -> Option<crate::pipeline::LoadedInteriors> {
    let Ok(found) = starbreaker_common::discover::find_p4k() else {
        eprintln!("skipping: no Data.p4k installed");
        return None;
    };
    let p4k = starbreaker_p4k::MappedP4k::open(found.path).unwrap();
    let dcb = p4k.read_file("Data\\Game2.dcb").or_else(|_| p4k.read_file("Data\\Game.dcb")).unwrap();
    let db = starbreaker_datacore::database::Database::from_bytes(&dcb).unwrap();
    let identity = glam::Mat4::IDENTITY.to_cols_array_2d();
    let payloads: Vec<_> = names
        .iter()
        .map(|name| {
            crate::socpak::load_interior_from_socpak(
                &p4k,
                &format!("Data\\ObjectContainers\\Ships\\CRUS\\Starlifter\\{name}.socpak"),
                identity,
                identity,
                &[],
            )
            .unwrap_or_else(|e| panic!("{name}.socpak: {e:?}"))
        })
        .collect();
    Some(super::super::build_interiors_from_payloads(
        &db,
        &p4k,
        &payloads,
        true,
        0,
        &mut crate::socpak::light_component::LightResolver::new(&db),
    ))
}

/// `LG_Hold` is a `light_group` entity with its links and its four state interactions mapped to their light states;
/// its lights name it; the hold's chipset is a `logic` entity.
#[test]
fn c2_hold_light_group_resolves_from_game_data() {
    let Some(interiors) = containers(&["hold_c2"]) else { return };
    let hold = &interiors.containers[0];
    let group = hold.entities.iter().find(|e| e.entity_id == LG_HOLD).expect("LG_Hold entity");
    assert_eq!(group.kind, "light_group");
    assert_eq!(group.entity_name.as_deref(), Some("LG_Hold"));
    assert_eq!(group.links, [HOLD_CHIPSET]);
    let facts = group.light_group.as_ref().expect("light group facts");
    let by_tag: HashMap<&str, Option<&str>> = facts
        .interactions
        .iter()
        .filter_map(|i| Some((i.linking_tag.as_deref()?, i.light_state.as_deref())))
        .collect();
    assert_eq!(by_tag.get("/Interaction/LightGroup/SetDefault"), Some(&Some("Default")));
    assert_eq!(by_tag.get("/Interaction/LightGroup/SetAuxiliary"), Some(&Some("Auxiliary")));
    assert_eq!(by_tag.get("/Interaction/LightGroup/SetEmergency"), Some(&Some("Emergency")));
    assert_eq!(by_tag.get("/Interaction/LightGroup/SetOff"), Some(&Some("Off")));
    let names: Vec<&str> = facts.interactions.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"Emergency Override"), "{names:?}");

    let members: Vec<_> = hold
        .lights
        .iter()
        .filter(|l| l.attachment.as_ref().and_then(|a| a.group_entity_id.as_deref()) == Some(LG_HOLD))
        .collect();
    assert!(!members.is_empty(), "the group's baked lights name it");
    assert!(members.iter().all(|l| l.attachment.as_ref().unwrap().group_links == [HOLD_CHIPSET]));

    let chipset = hold.entities.iter().find(|e| e.entity_id == HOLD_CHIPSET).expect("the hold chipset");
    assert_eq!(chipset.kind, "logic");
    let sent: Vec<&str> = chipset.sendable_interactions.iter().map(|s| s.name.as_str()).collect();
    assert!(sent.contains(&"SetLightsAlarm"), "{sent:?}");
}

/// The four C2 light panels send `LightsOn` / `LightsOff` / `LightsLow` / `LightsAlarm` with the light-group tags;
/// the top deck's chipset (which the panel and the light group both link) is exported with its link.
#[test]
fn c2_light_panels_send_the_light_group_tags() {
    let Some(interiors) = containers(&["top_deck_c2", "hold_c2", "modular_c2"]) else { return };
    let panels: Vec<_> = interiors
        .containers
        .iter()
        .flat_map(|c| c.placements.iter())
        .filter(|p| p.identity.entity_name.as_deref().is_some_and(|n| n.starts_with("ControlPanel_Screen_LightControl")))
        .collect();
    assert_eq!(panels.len(), 4, "{:?}", panels.iter().map(|p| &p.identity.entity_name).collect::<Vec<_>>());
    for panel in &panels {
        let sent: Vec<(&str, Option<&str>)> = panel
            .identity
            .sendable_interactions
            .iter()
            .map(|s| (s.name.as_str(), s.linking_tag.as_deref()))
            .collect();
        assert_eq!(
            sent,
            [
                ("LightsOn", Some("/Interaction/LightGroup/SetDefault")),
                ("LightsOff", Some("/Interaction/LightGroup/SetOff")),
                ("LightsLow", Some("/Interaction/LightGroup/SetAuxiliary")),
                ("LightsAlarm", Some("/Interaction/LightGroup/SetEmergency")),
            ],
            "{:?}",
            panel.identity.entity_name
        );
    }
    let top = interiors.containers.iter().find(|c| c.name == "top_deck_c2").expect("top_deck_c2");
    let chipset = top.entities.iter().find(|e| e.entity_id == TOP_DECK_CHIPSET).expect("the top-deck chipset");
    assert_eq!((chipset.kind, chipset.links.as_slice()), ("logic", [TOP_DECK_DEFCON.to_string()].as_slice()));
}
