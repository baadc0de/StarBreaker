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
    assert!((evaluate(&s, 0.25) - 0.353_553).abs() < 1e-4 && (evaluate(&s, 0.75) - 0.646_447).abs() < 1e-4, "inverse S-curve");
}

/// The non-linear shapes (Wwise's `AkInterpolation`, as wwiser's renderer reproduces it) at interior points.
#[test]
fn non_linear_shapes_match_the_reference_forms() {
    let at = |shape: u32, t: f32| evaluate(&[(0.0, 0.0, shape), (1.0, 1.0, 4)], t);
    assert!((at(6, 0.25) - 0.156_25).abs() < 1e-6, "exp1 (t + 1) t / 2");
    assert!((at(2, 0.25) - 0.343_75).abs() < 1e-6, "log1 t (3 - t) / 2");
    assert!((at(5, 0.25) - 0.146_447).abs() < 1e-4, "S-curve (1 - cos pi t) / 2");
    assert!((at(0, 0.5) - 0.875).abs() < 1e-6, "log3 1 - (1 - t)^3");
    assert!((at(1, 0.5) - 0.707_107).abs() < 1e-4, "sine sin(pi t / 2)");
    assert!((at(7, 0.5) - 0.292_893).abs() < 1e-4, "sine reciprocal 1 - cos(pi t / 2)");
    // A falling segment uses the same shape from the left point to the right one.
    assert!((evaluate(&[(0.0, 1.0, 6), (1.0, 0.0, 4)], 0.25) - 0.843_75).abs() < 1e-6);
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

#[test]
fn a_peak_limiter_slot_carries_its_parameters() {
    let objects = vec![bus(1, 0, false, &[], Some(60), None), peak_limiter_share_set(60, -1.5, 20.0, 0.01, 0.25, -0.5)];
    let lib = library(atl(&[], &[]), vec![("Init.bnk", objects, Vec::new())]);
    let r = crate::playable::Resolver::new(&lib);
    let chain = bus_chain(&r, 1);
    let fx = &chain[0].1.effects[0];
    assert_eq!(fx.kind, "peak_limiter");
    let l = fx.limiter.as_ref().expect("limiter parameters");
    assert_eq!((l.threshold_db, l.ratio, l.look_ahead_s, l.release_s, l.output_level_db), (-1.5, 20.0, 0.01, 0.25, -0.5));
    assert!(l.process_lfe && l.channel_link);
}

#[test]
fn a_bypassed_effect_slot_says_so() {
    let objects = vec![
        bus_with_fx_bits(1, 0, false, &[], Some(60), 0x03, None),
        bus(2, 1, false, &[], Some(61), None),
        peak_limiter_share_set(60, -1.0, 10.0, 0.01, 0.1, 0.0),
        peak_limiter_share_set(61, -1.0, 10.0, 0.01, 0.1, 0.0),
    ];
    let lib = library(atl(&[], &[]), vec![("Init.bnk", objects, Vec::new())]);
    let r = crate::playable::Resolver::new(&lib);
    let chain = bus_chain(&r, 2);
    assert!(!chain[0].1.effects[0].bypass);
    assert!(chain[1].1.effects[0].bypass, "bit 0 of the slot's bit vector");
}

#[test]
fn a_meter_slot_carries_its_ballistics_and_the_game_parameter_it_writes() {
    let gp = crate::fnv1_hash("DM_Test_Meter");
    let objects = vec![bus(1, 0, false, &[], Some(70), None), meter_share_set(70, 0.05, 0.4, -48.0, 0.0, 0.1, 1, 0, true, gp)];
    let lib = library(atl(&[], &[("DM_Test_Meter", -48.0)]), vec![("Init.bnk", objects, Vec::new())]);
    let r = crate::playable::Resolver::new(&lib);
    let fx = &bus_chain(&r, 1)[0].1.effects[0];
    assert_eq!(fx.kind, "meter");
    let m = fx.meter.as_ref().expect("meter parameters");
    assert_eq!((m.attack_s, m.release_s, m.min_db, m.max_db, m.hold_s), (0.05, 0.4, -48.0, 0.0, 0.1));
    assert!(!m.infinite_hold && m.apply_downstream_volume);
    assert_eq!((m.mode, m.scope), ("rms", "global"));
    assert_eq!(m.game_parameter, "DM_Test_Meter", "the ATL name of the written game parameter");
    assert_eq!(m.game_parameter_id, gp);
}

#[test]
fn effect_parameter_rtpcs_come_with_the_effect() {
    let gp = crate::fnv1_hash("FX_Test");
    let eq = parametric_eq_share_set(80, [(0, 0.0, 400.0, 1.0, true), (6, 3.0, 1000.0, 0.7, true), (5, 0.0, 5000.0, 1.0, false)], &[(gp, 3, vec![(0.0, 400.0, 4), (1.0, 20000.0, 4)])]);
    let objects = vec![bus(1, 0, false, &[], Some(80), None), eq];
    let lib = library(atl(&[], &[("FX_Test", 1.0)]), vec![("Init.bnk", objects, Vec::new())]);
    let r = crate::playable::Resolver::new(&lib);
    let fx = &bus_chain(&r, 1)[0].1.effects[0];
    let eq = fx.eq.as_ref().unwrap();
    assert_eq!((eq.bands[0].filter, eq.bands[0].frequency_hz), ("lowpass", 400.0));
    assert_eq!(fx.rtpc.len(), 1);
    let c = &fx.rtpc[0];
    assert_eq!((c.rtpc.as_str(), c.param.as_str()), ("FX_Test", "fx_param_3"), "effect parameters keep their plugin id");
    assert_eq!(c.points, vec![(0.0, 400.0, 4), (1.0, 20000.0, 4)]);
}

/// A state group that sets an effect slot's bypass (`BypassFX` 0x30 for slot 0, 0x130 for slot 1) makes the slot's
/// bypass depend on the game's state: the slot names the groups.
#[test]
fn a_slot_bypassed_by_state_names_its_state_groups() {
    let eq = parametric_eq_share_set(80, [(0, 0.0, 400.0, 1.0, true), (6, 0.0, 1000.0, 0.7, true), (5, 0.0, 5000.0, 1.0, false)], &[]);
    let b = with_state_group(bus(1, 0, false, &[], Some(80), None), 77, &[(5, &[(0x130, 1.0)]), (6, &[(0x30, 1.0)])]);
    let lib = library(atl(&[], &[]), vec![("Init.bnk", vec![b, eq], Vec::new())]);
    let r = crate::playable::Resolver::new(&lib);
    let fx = &bus_chain(&r, 1)[0].1.effects[0];
    assert_eq!(fx.slot, 0);
    assert_eq!(fx.state_bypass, vec![StateBypass { group: 77, states: vec![6] }], "slot 0 is bypassed in state 6 only");
}

/// A gain curve on a game parameter the ATL does not name still has a default: the Wwise game parameter's own
/// (Init.bnk STMG). The static bus level includes the curve at that default (one resolver for every default).
#[test]
fn a_gain_curve_without_an_atl_default_uses_the_stmg_default() {
    let gp = 0x00C0_FFEE;
    let objects = vec![bus(1, 0, false, &[], None, Some((gp, vec![(0.0, -0.4988128, 4), (1.0, 0.0, 4)])))];
    let mut init = crate::library::LoadedBank::from_parts("Init.bnk", hierarchy(objects), &[]);
    init.game_parameter_defaults.insert(gp, 0.0);
    let lib = crate::library::AudioLibrary::in_memory(atl(&[], &[]), vec![init]);
    let r = crate::playable::Resolver::new(&lib);
    let out = &bus_chain(&r, 1)[0].1;
    assert!((out.rtpc_default_db - -6.0).abs() < 1e-3, "{}", out.rtpc_default_db);
    assert!(out.rtpc_without_default.is_empty());
}

#[test]
fn an_eq_slot_says_whether_it_processes_the_lfe() {
    let mut eq = parametric_eq_share_set(80, [(0, 0.0, 400.0, 1.0, true), (6, 0.0, 1000.0, 0.7, false), (5, 0.0, 5000.0, 1.0, false)], &[]);
    // The fixture writes process-LFE 0 as the last parameter byte; v154 blocks are 56 bytes.
    let objects = vec![bus(1, 0, false, &[], Some(80), None), { eq.data[8 + 55] = 1; eq }];
    let lib = library(atl(&[], &[]), vec![("Init.bnk", objects, Vec::new())]);
    let r = crate::playable::Resolver::new(&lib);
    assert!(bus_chain(&r, 1)[0].1.effects[0].eq.as_ref().unwrap().process_lfe);
}
