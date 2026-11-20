//! Room volumes, the environment choice and authored scalars.

use super::*;
use starbreaker_wwise::manifest::EnvironmentRecord;

fn env(name: &str, size: (f32, f32), interiority: (f32, f32)) -> (String, EnvironmentRecord) {
    (name.to_owned(), EnvironmentRecord { record: format!("AudioEnvironment.{name}"), size, interiority })
}

#[test]
fn shape_volumes_of_box_prism_and_trimesh_with_their_transform() {
    let scale2 = json!([[2.0, 0, 0, 0], [0, 1.0, 0, 0], [0, 0, 1.0, 0], [5.0, 5.0, 5.0, 1.0]]);
    let boxed = json!({ "kind": "box", "box_min": [0, 0, 0], "box_max": [1, 2, 3], "transform": scale2 });
    assert!((shape_volume(&boxed) - 12.0).abs() < 1e-9);
    let identity = json!([[1.0, 0, 0, 0], [0, 1.0, 0, 0], [0, 0, 1.0, 0], [0, 0, 0, 1.0]]);
    let prism = json!({ "kind": "prism", "points": [[0, 0, 0], [4, 0, 0], [4, 3, 0], [0, 3, 0]], "height": 2.5, "transform": identity });
    assert!((shape_volume(&prism) - 30.0).abs() < 1e-9);
    // A unit cube as 12 triangles (outward winding).
    let v = |x: f64, y: f64, z: f64| json!([x, y, z]);
    let quads = [
        [(0., 0., 0.), (0., 1., 0.), (1., 1., 0.), (1., 0., 0.)],
        [(0., 0., 1.), (1., 0., 1.), (1., 1., 1.), (0., 1., 1.)],
        [(0., 0., 0.), (1., 0., 0.), (1., 0., 1.), (0., 0., 1.)],
        [(0., 1., 0.), (0., 1., 1.), (1., 1., 1.), (1., 1., 0.)],
        [(0., 0., 0.), (0., 0., 1.), (0., 1., 1.), (0., 1., 0.)],
        [(1., 0., 0.), (1., 1., 0.), (1., 1., 1.), (1., 0., 1.)],
    ];
    let mut tris = Vec::new();
    for q in quads {
        for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            tris.extend(t.iter().map(|p| v(p.0, p.1, p.2)));
        }
    }
    let mesh = json!({ "kind": "trimesh", "vertices": tris, "transform": identity });
    assert!((shape_volume(&mesh) - 1.0).abs() < 1e-9);
}

#[test]
fn environment_by_interiority_then_size_range_then_nearest_range() {
    let envs = vec![
        env("Ext_Small", (30.0, 60.0), (0.25, 0.9)),
        env("Int_Tiny", (0.1, 2.0), (0.9, 1.0)),
        env("Int_Small", (3.0, 5.0), (0.9, 1.0)),
        env("Int_Huge", (31.0, 250.0), (0.75, 1.0)),
    ];
    assert_eq!(choose_environment(&envs, 4.0, 1.0), Some("Int_Small"));
    assert_eq!(choose_environment(&envs, 2.6, 1.0), Some("Int_Small"), "nearer the small range than the tiny one");
    assert_eq!(choose_environment(&envs, 40.0, 1.0), Some("Int_Huge"));
    assert_eq!(choose_environment(&envs, 40.0, 0.5), Some("Ext_Small"));
    assert_eq!(choose_environment(&envs, 4.0, 0.1), None);
}

#[test]
fn authored_scalars_parse_as_numbers_and_bools() {
    assert_eq!(scalar("1.5"), json!(1.5));
    assert_eq!(scalar("true"), json!(true));
    assert_eq!(scalar("Play_X"), json!("Play_X"));
}

/// Game data (skips without a Data.p4k): the C2 lift carriage's sound effects carry their tag paths, triggers, bone
/// and offset; the lift buttons' Activate trigger; the room environments come from the game's records.
#[test]
fn c2_lift_sound_effects_from_datacore() {
    let Some(p4k) = crate::test_p4k::p4k() else { return };
    let dcb = p4k.read_file(r"Data\Game2.dcb").or_else(|_| p4k.read_file(r"Data\Game.dcb")).unwrap();
    let db = Database::from_bytes(&dcb).unwrap();
    let tags = dce::tag_paths(&db);
    assert!(tags.len() > 1000, "{} tags", tags.len());
    let class = |name: &str| db.records_by_type_name("EntityClassDefinition").find(|r| db.resolve_string2(r.name_offset) == name).unwrap();
    let effects = dce::sound_effects(&db, class("EntityClassDefinition.CRUS_Hercules_Lift_Platform"), &tags);
    let by_tag: BTreeMap<&str, &dce::SoundEffect> = effects.iter().map(|e| (e.tag.as_str(), e)).collect();
    assert_eq!(by_tag.len(), 3, "{effects:?}");
    assert_eq!(by_tag["RampUp"].on, "trigger");
    assert!(by_tag["RampUp"].tag_path.ends_with("Elevators/RampUp"), "{}", by_tag["RampUp"].tag_path);
    assert_eq!(by_tag["InTransit"].on, "tag");
    assert_eq!(by_tag["Stop"].bone.as_deref(), Some("roomlg_elevator_platform_08x06x04_a"));
    assert_eq!(by_tag["Stop"].offset, [3.0, -3.0, 2.0]);
    let buttons = dce::sound_effects(&db, class("EntityClassDefinition.CRUS_Hercules_Elevator_ButtonsCall"), &tags);
    assert_eq!(buttons.len(), 1);
    assert_eq!(buttons[0].tag, "Activate");
    let envs = dce::audio_environments(&db);
    assert!(envs.iter().any(|(n, e)| n.starts_with("VERB_SYS_INT") && e.interiority.1 == 1.0));
}

