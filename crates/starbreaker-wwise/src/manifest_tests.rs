//! The Ogg page walk behind media durations (synthetic pages; no game data).

use super::*;

/// One Ogg page: capture pattern, version, header type, granule, serial, sequence, CRC (not checked), segments.
fn page(granule: u64, body: &[u8]) -> Vec<u8> {
    let mut p = b"OggS".to_vec();
    p.push(0);
    p.push(0);
    p.extend_from_slice(&granule.to_le_bytes());
    p.extend_from_slice(&[0; 12]);
    let mut lacing = vec![255u8; body.len() / 255];
    lacing.push((body.len() % 255) as u8);
    p.push(lacing.len() as u8);
    p.extend(lacing);
    p.extend_from_slice(body);
    p
}

#[test]
fn the_last_granule_comes_from_the_last_page_not_from_packet_bytes() {
    let mut ogg = page(0, b"header");
    ogg.extend(page(48_000, &[1; 300]));
    // The last page's packet happens to contain the capture pattern followed by junk.
    let mut body = b"xxOggS\0\0\xff\xff\xff\xff\xff\xff\xff\x7f".to_vec();
    body.extend_from_slice(&[2; 40]);
    ogg.extend(page(96_000, &body));
    assert_eq!(last_granule(&ogg), Some(96_000));
}

#[test]
fn a_truncated_last_page_keeps_the_previous_granule() {
    let mut ogg = page(48_000, &[1; 10]);
    let mut cut = page(96_000, &[2; 100]);
    cut.truncate(40);
    ogg.extend(cut);
    assert_eq!(last_granule(&ogg), Some(48_000));
}

/// A Wwise 2012+ media whose audio peaks above full scale (the quantum-travel exit, +1.2 dBFS once decoded) is
/// valid Vorbis with the aoTuV codebooks; the codebook check must not reject it for clipping (it was dropped from
/// packages with "validation failed with both codebooks"). Game data: skipped without a Data.p4k.
#[test]
fn a_clipping_media_still_decodes() {
    let p4k = match starbreaker_p4k::open_p4k() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skip: no Data.p4k ({e})");
            return;
        }
    };
    let wem = p4k.read_file(r"Data\Sounds\wwise\Media\72171900.wem").expect("the media is in the archive");
    let ogg = starbreaker_wem::decode::vorbis_to_ogg(&wem).expect("decodes");
    let wem_file = starbreaker_wem::WemFile::parse(&wem).unwrap();
    let seconds = last_granule(&ogg).unwrap() as f32 / wem_file.sample_rate() as f32;
    assert!((seconds - 3.634).abs() < 0.01, "{seconds} s");
}

/// The RTPCs a package needs include those on its buses' effect parameters and the game parameters its meters write
/// (their defaults are what a side-chain starts from).
#[test]
fn effect_and_meter_game_parameters_are_listed_with_their_defaults() {
    use crate::hirc_fixture::*;
    let objects = vec![
        sound(30, 300, 0x0001_0001, &Base { bus: 1, ..Base::default() }),
        event("Play_Synthetic", &[1000]),
        action(1000, 0x0403, 30, None),
    ];
    let init = vec![
        bus(1, 2, false, &[], Some(80), None),
        bus(2, 0, false, &[], Some(70), None),
        parametric_eq_share_set(80, [(0, 0.0, 400.0, 1.0, true), (6, 0.0, 1000.0, 0.7, false), (5, 0.0, 5000.0, 1.0, false)], &[(crate::fnv1_hash("FX_Test"), 2, vec![(0.0, 2.3, 4), (1.0, 4.3, 4)])]),
        meter_share_set(70, 0.0, 0.1, -48.0, 0.0, 0.0, 0, 0, false, crate::fnv1_hash("DM_Test")),
    ];
    let lib = library(atl(&[("Play_Synthetic", "Test_Bank.bnk", "OneShot")], &[("FX_Test", 1.0), ("DM_Test", -48.0)]), vec![("Test_Bank.bnk", objects, Vec::new()), ("Init.bnk", init, Vec::new())]);
    let mut b = ManifestBuilder::new(&lib, "Audio");
    b.trigger("Play_Synthetic");
    let (m, _) = b.finish();
    assert_eq!(m.rtpcs.get("FX_Test").and_then(|r| r.default), Some(1.0));
    assert_eq!(m.rtpcs.get("DM_Test").and_then(|r| r.default), Some(-48.0));
    assert_eq!(m.buses["2"].effects[0].meter.as_ref().unwrap().game_parameter, "DM_Test");
}

/// An RTPC the ATL does not name is listed under its id with the Wwise game parameter's default.
#[test]
fn an_unnamed_game_parameter_is_listed_with_its_stmg_default() {
    use crate::hirc_fixture::*;
    let gp = 0x00C0_FFEE;
    let objects = vec![sound(30, 300, 0x0001_0001, &Base { bus: 1, ..Base::default() }), event("Play_Synthetic", &[1000]), action(1000, 0x0403, 30, None)];
    let mut init = crate::library::LoadedBank::from_parts("Init.bnk", hierarchy(vec![bus(1, 0, false, &[], None, Some((gp, vec![(0.0, -1.0, 4), (1.0, 0.0, 4)])))]), &[]);
    init.game_parameter_defaults.insert(gp, 0.5);
    let bank = crate::library::LoadedBank::from_parts("Test_Bank.bnk", hierarchy(objects), &[]);
    let lib = crate::library::AudioLibrary::in_memory(atl(&[("Play_Synthetic", "Test_Bank.bnk", "OneShot")], &[]), vec![bank, init]);
    let mut b = ManifestBuilder::new(&lib, "Audio");
    b.trigger("Play_Synthetic");
    let (m, _) = b.finish();
    let name = format!("{gp:#010x}");
    assert_eq!(m.rtpcs.get(&name).and_then(|r| r.wwise_default), Some(0.5), "{:?}", m.rtpcs.keys().collect::<Vec<_>>());
}
