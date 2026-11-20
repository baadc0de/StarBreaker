//! Curve evaluation and bus chains (gains, RTPC defaults, a Convolution Reverb slot) on a synthetic Init bank.

use super::*;
use crate::hirc_fixture::*;

#[test]
fn curves_interpolate_linear_and_hold_constant_and_clamp() {
    let pts = [(0.0, 0.0, 9), (1.0, 0.0, 4), (10.0, -96.0, 4)];
    assert_eq!(evaluate(&pts, -1.0), 0.0);
    assert_eq!(evaluate(&pts, 0.5), 0.0, "constant holds the left value");
    assert!((evaluate(&pts, 5.5) - -48.0).abs() < 1e-4);
    assert_eq!(evaluate(&pts, 20.0), -96.0);
    let shaped = [(0.0, 0.0, 8), (1.0, 1.0, 4)];
    assert!((evaluate(&shaped, 0.5) - 0.125).abs() < 1e-6, "exp3 is t^3");
    let s = [(0.0, 0.0, 3), (1.0, 1.0, 4)];
    assert!((evaluate(&s, 0.5) - 0.5).abs() < 1e-5 && (evaluate(&s, 0.25) - 0.3264).abs() < 1e-3, "inverse S-curve");
}

#[test]
fn a_bus_chain_carries_gains_rtpc_defaults_and_its_convolution_slot() {
    let mix = crate::fnv1_hash("MIX_Test");
    let objects = vec![
        bus(1, 0, false, &[(crate::hirc::props::prop_id::BUS_VOLUME, -2.0)], None, None),
        bus(2, 1, false, &[(crate::hirc::props::prop_id::VOLUME, -4.0)], None, Some((mix, vec![(0.0, -6.0, 4), (1.0, 0.0, 4)]))),
        bus(crate::fnv1_hash("VERB_Room_Bus"), 2, true, &[], Some(50), None),
        convolution_share_set(50, 777),
    ];
    let mut atl = atl(&[], &[("MIX_Test", 0.25)]);
    let env_doc = r#"<ATLConfig atl_name="E"><AudioEnvironments><ATLEnvironment atl_name="VERB_Room"><WwiseAuxBus wwise_name="VERB_Room_Bus"/></ATLEnvironment></AudioEnvironments></ATLConfig>"#;
    atl.add_document(&crate::game_audio::parse_document(env_doc.as_bytes()).unwrap().unwrap()).unwrap();
    let lib = library(atl, vec![("Init.bnk", objects, Vec::new())]);
    let r = crate::playable::Resolver::new(&lib);
    let chain = bus_chain(&r, crate::fnv1_hash("VERB_Room_Bus"));
    assert_eq!(chain.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![crate::fnv1_hash("VERB_Room_Bus"), 2, 1]);
    let (_, aux) = &chain[0];
    assert!(aux.aux);
    assert_eq!(aux.name.as_deref(), Some("VERB_Room_Bus"));
    assert_eq!(aux.effects[0].kind, "convolution_reverb");
    assert_eq!(aux.effects[0].ir_media, Some(777));
    assert_eq!(aux.effects[0].convolution.unwrap().dry_level_db, -96.0);
    assert_eq!(chain[1].1.voice_volume_db, -4.0);
    assert!((chain[1].1.rtpc_default_db - to_db(2, -4.5)).abs() < 1e-5, "MIX_Test at its default 0.25 on the dB-scaled curve");
    assert_eq!(chain[2].1.bus_volume_db, -2.0);
}

#[test]
fn db_scaled_curve_values_are_linear_gain_minus_one() {
    assert!((to_db(2, -0.292_054_2) - -3.0).abs() < 1e-4);
    assert!((to_db(2, -0.498_812_8) - -6.0).abs() < 1e-4);
    assert_eq!(to_db(2, -1.0), -200.0);
    assert_eq!(to_db(0, -12.0), -12.0);
}
