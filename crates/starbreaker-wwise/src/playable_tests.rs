//! Trigger resolution on synthetic banks: honest trees (random containers stay containers), loops, own volumes,
//! inherited attenuation / bus / aux routing, the context of the target's ancestors, cross-bank media and the reasons
//! of unresolvable triggers.

use super::*;
use crate::hirc::props::prop_id;
use crate::hirc_fixture::*;

const BANK: &str = "Test_Bank.bnk";
const OTHER: &str = "Other_Bank.bnk";

/// Mixer 10 (bus 99, 3D, attenuation 77, game aux used, -3 dB) → random 20 (avoid 2, weights 50/25, +2 dB,
/// pitch ±100) → sounds 30 (media 300, loops forever) and 31 (media 301, embedded in another bank).
fn library_with_ambience() -> crate::library::AudioLibrary<'static> {
    let mix = Base {
        bus: 99,
        props: vec![(prop_id::VOLUME, f(-3.0)), (prop_id::ATTENUATION_ID, 77), (prop_id::GAME_AUX_SEND_VOLUME, f(-6.0))],
        positioning: Some(true),
        aux_flags: 0x01 | 0x02,
        ..Base::default()
    };
    let random = Base {
        parent: 10,
        props: vec![(prop_id::VOLUME, f(2.0))],
        random: vec![(prop_id::PITCH, -100.0, 100.0)],
        rtpc: vec![(crate::fnv1_hash("area_fade"), 0, vec![(0.0, 0.0, 4), (1.0, -96.0, 4)])],
        ..Base::default()
    };
    let looped = Base { parent: 20, props: vec![(prop_id::LOOP, 0)], ..Base::default() };
    let once = Base { parent: 20, ..Base::default() };
    let objects = vec![
        mixer(10, &mix, &[20, 32]),
        ranseq(20, &random, 0, 0x08, 0, 2, &[(30, 50_000), (31, 25_000)]),
        sound(30, 300, 0x0001_0001, &looped),
        sound(31, 301, 0x0001_0001, &once),
        sound(32, 0, 0x0064_0002, &Base { parent: 10, ..Base::default() }),
        event("Play_Amb", &[1000, 1001]),
        action(1000, 0x0403, 20, Some(250)),
        action(1001, 0x0102, 20, None),
        event("Play_Silent", &[1002]),
        action(1002, 0x0403, 32, None),
        event("Play_Lost", &[1003]),
        action(1003, 0x0403, 12345, None),
    ];
    let atl = atl(
        &[("Play_Amb", BANK, "Infinite"), ("Play_Silent", BANK, "OneShot"), ("Play_Lost", BANK, "OneShot"), ("Play_NoBank", "Missing.bnk", "OneShot")],
        &[("area_fade", 0.0)],
    );
    library(atl, vec![(BANK, objects, vec![(300, b"RIFF300".to_vec())]), (OTHER, Vec::new(), vec![(301, b"RIFF301".to_vec())])])
}

#[test]
fn a_random_container_stays_a_container_with_its_rules() {
    let lib = library_with_ambience();
    let mut r = Resolver::new(&lib);
    let t = r.trigger("Play_Amb").clone();
    assert_eq!(t.status, "playable");
    assert_eq!(t.play.len(), 1);
    let root = &r.out.nodes[t.play[0].root];
    let NodeKind::Random { weights, avoid_repeat, play, .. } = &root.kind else { panic!("{root:?}") };
    assert_eq!((weights.as_slice(), *avoid_repeat), (&[50.0, 25.0][..], 2));
    assert!(play.continuous);
    assert_eq!((play.loop_count, play.transition, play.transition_s), (0, "crossfade_amp", 1.0));
    assert_eq!(root.children.len(), 2, "both variations, not a union of leaves");
    let leaf = &r.out.nodes[root.children[0]];
    assert!(matches!(leaf.kind, NodeKind::Sound { media: Some(300), loop_count: 0, .. }), "{leaf:?}");
    let other = &r.out.nodes[root.children[1]];
    assert!(matches!(other.kind, NodeKind::Sound { media: Some(301), loop_count: 1, .. }), "cross-bank media resolves");
    assert_eq!(t.play[0].delay_s, 0.25);
    assert_eq!(t.other.len(), 1, "the stop action is kept");
}

#[test]
fn own_props_stay_on_their_node_and_override_settings_are_inherited() {
    let lib = library_with_ambience();
    let mut r = Resolver::new(&lib);
    let t = r.trigger("Play_Amb").clone();
    let root = &r.out.nodes[t.play[0].root];
    assert_eq!(root.props.volume_db, 2.0);
    assert_eq!(root.props.random, vec![("pitch".to_owned(), -100.0, 100.0)]);
    assert_eq!(root.props.rtpc[0].rtpc, "area_fade");
    assert_eq!(root.props.rtpc[0].param, "volume");
    // The mixer above the target is the context: its volume and aux-send gain add.
    assert_eq!((t.play[0].context.volume_db, t.play[0].context.game_aux_send_db), (-3.0, -6.0));
    let leaf = &r.out.nodes[root.children[0]];
    assert_eq!(leaf.effective.bus, Some(99));
    assert!(leaf.effective.spatial);
    assert_eq!(leaf.effective.attenuation, Some(77));
    assert!(leaf.effective.use_game_aux);
    assert_eq!(r.out.media_homes[&301].0, BANK, "the referencing bank is the home; the media is found elsewhere");
}

#[test]
fn unresolvable_triggers_say_why() {
    let lib = library_with_ambience();
    let mut r = Resolver::new(&lib);
    assert_eq!(r.trigger("Play_Silent").status, "no_media");
    assert!(matches!(r.out.nodes[r.out.triggers["Play_Silent"].play[0].root].kind, NodeKind::Sound { source_plugin: Some(ref p), .. } if p == "silence"));
    assert_eq!(r.trigger("Play_Lost").status, "no_media");
    assert_eq!(r.trigger("Play_NoBank").status, "bank_missing");
    assert_eq!(r.trigger("Play_Unknown").status, "not_in_atl");
    let media = lib.media(301, lib.bank(BANK).as_deref(), false).expect("found in the other bank");
    assert_eq!(media.1, crate::library::MediaSource::Bank { bank: OTHER.to_owned() });
}

#[test]
fn a_blend_container_keeps_every_child_and_a_sequence_its_order() {
    let objects = vec![
        blend(40, &Base { positioning: Some(false), ..Base::default() }, &[41, 42]),
        sound(41, 410, 0x0001_0001, &Base { parent: 40, ..Base::default() }),
        ranseq(42, &Base { parent: 40, ..Base::default() }, 1, 0, 1, 0, &[(43, 50_000), (44, 50_000)]),
        sound(43, 430, 0x0001_0001, &Base { parent: 42, ..Base::default() }),
        sound(44, 440, 0x0001_0001, &Base { parent: 42, ..Base::default() }),
        event("Play_Bed", &[1100]),
        action(1100, 0x0403, 40, None),
    ];
    let media = vec![(410, b"a".to_vec()), (430, b"b".to_vec()), (440, b"c".to_vec())];
    let lib = library(atl(&[("Play_Bed", BANK, "Infinite")], &[]), vec![(BANK, objects, media)]);
    let mut r = Resolver::new(&lib);
    let t = r.trigger("Play_Bed").clone();
    let root = &r.out.nodes[t.play[0].root];
    assert!(matches!(root.kind, NodeKind::Blend { .. }));
    assert!(!root.effective.spatial, "2D positioning override");
    assert_eq!(root.children.len(), 2);
    let seq = &r.out.nodes[root.children[1]];
    assert!(matches!(seq.kind, NodeKind::Sequence { .. }));
    let order: Vec<u32> = seq.children.iter().map(|&c| r.out.nodes[c].id).collect();
    assert_eq!(order, vec![43, 44]);
}
