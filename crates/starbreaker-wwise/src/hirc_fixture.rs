//! Test-only encoders of synthetic v154 HIRC objects (node base, sound, random/sequence and blend containers,
//! actor-mixer, event, action, attenuation, bus) and a one-call in-memory [`AudioLibrary`] over them.
//! Every value is synthetic; no game data.

use crate::hirc::{HircEntry, HircSection, Hierarchy};
use crate::library::{AudioLibrary, LoadedBank};

/// Node-base settings of a synthetic object.
#[derive(Clone, Default)]
pub struct Base {
    pub parent: u32,
    pub bus: u32,
    /// `(prop id, raw u32 value)`.
    pub props: Vec<(u8, u32)>,
    /// `(prop id, min, max)`.
    pub random: Vec<(u8, f32, f32)>,
    /// Override positioning: `Some(listener-relative 3D)`.
    pub positioning: Option<bool>,
    /// The 3D bits of a 3D positioning override (default `0x09`: position spatialisation, attenuation on).
    pub bits3d: Option<u8>,
    /// Aux flags (bit0 override game aux, bit1 use game aux, bit2 override user aux, bit3 has aux ids).
    pub aux_flags: u8,
    pub aux_ids: [u32; 4],
    /// RTPC curves `(rtpc id, param id, points)`.
    pub rtpc: Vec<(u32, u32, Vec<(f32, f32, u32)>)>,
}

pub fn f(v: f32) -> u32 {
    v.to_bits()
}

pub fn node_base(b: &Base) -> Vec<u8> {
    let mut d = vec![0u8, 0, 0, 0]; // fx: no override, none; metadata: no override, none
    d.extend_from_slice(&b.bus.to_le_bytes());
    d.extend_from_slice(&b.parent.to_le_bytes());
    d.push(0); // priority flags
    d.push(b.props.len() as u8);
    d.extend(b.props.iter().map(|p| p.0));
    for p in &b.props {
        d.extend_from_slice(&p.1.to_le_bytes());
    }
    d.push(b.random.len() as u8);
    d.extend(b.random.iter().map(|r| r.0));
    for r in &b.random {
        d.extend_from_slice(&r.1.to_le_bytes());
        d.extend_from_slice(&r.2.to_le_bytes());
    }
    match b.positioning {
        Some(true) => d.extend_from_slice(&[0x03, b.bits3d.unwrap_or(0x09)]), // override + 3D; bits3d
        Some(false) => d.push(0x01),
        None => d.push(0x00),
    }
    d.push(b.aux_flags);
    if b.aux_flags & 0x08 != 0 {
        for id in b.aux_ids {
            d.extend_from_slice(&id.to_le_bytes());
        }
    }
    d.extend_from_slice(&0u32.to_le_bytes()); // reflections aux bus
    d.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // adv settings
    d.extend_from_slice(&[0, 0]); // state chunk: no props, no groups
    d.extend_from_slice(&(b.rtpc.len() as u16).to_le_bytes());
    for (rtpc, param, points) in &b.rtpc {
        d.extend_from_slice(&rtpc.to_le_bytes());
        d.push(0); // game parameter
        d.push(0); // accumulation
        d.push(*param as u8); // VLQ param id (< 128)
        d.extend_from_slice(&0u32.to_le_bytes()); // curve id
        d.push(2); // dB scaling
        d.extend_from_slice(&(points.len() as u16).to_le_bytes());
        for (x, y, i) in points {
            d.extend_from_slice(&x.to_le_bytes());
            d.extend_from_slice(&y.to_le_bytes());
            d.extend_from_slice(&i.to_le_bytes());
        }
    }
    d
}

pub fn sound(id: u32, media: u32, plugin: u32, b: &Base) -> HircEntry {
    let mut d = Vec::new();
    d.extend_from_slice(&plugin.to_le_bytes());
    d.push(0); // embedded
    d.extend_from_slice(&media.to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes());
    d.push(0);
    if plugin & 0x0F == 2 {
        d.extend_from_slice(&0u32.to_le_bytes()); // plugin param size
    }
    d.extend(node_base(b));
    HircEntry { type_id: 2, object_id: id, data: d }
}

/// A random (`mode` 0) or sequence (1) container. `flags`: bit1 reset each play, bit3 continuous.
pub fn ranseq(id: u32, b: &Base, mode: u8, flags: u8, loop_count: u16, avoid: u16, items: &[(u32, i32)]) -> HircEntry {
    let mut d = node_base(b);
    d.extend_from_slice(&loop_count.to_le_bytes());
    d.extend_from_slice(&0u16.to_le_bytes());
    d.extend_from_slice(&0u16.to_le_bytes());
    for v in [1000.0f32, 0.0, 0.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&avoid.to_le_bytes());
    d.extend_from_slice(&[1, 0, mode, flags]); // crossfade amp, normal random
    d.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for (c, _) in items {
        d.extend_from_slice(&c.to_le_bytes());
    }
    d.extend_from_slice(&(items.len() as u16).to_le_bytes());
    for (c, w) in items {
        d.extend_from_slice(&c.to_le_bytes());
        d.extend_from_slice(&w.to_le_bytes());
    }
    HircEntry { type_id: 5, object_id: id, data: d }
}

pub fn blend(id: u32, b: &Base, children: &[u32]) -> HircEntry {
    let mut d = node_base(b);
    d.extend_from_slice(&(children.len() as u32).to_le_bytes());
    for c in children {
        d.extend_from_slice(&c.to_le_bytes());
    }
    d.extend_from_slice(&0u32.to_le_bytes()); // no layers
    d.push(0);
    HircEntry { type_id: 9, object_id: id, data: d }
}

pub fn mixer(id: u32, b: &Base, children: &[u32]) -> HircEntry {
    let mut d = node_base(b);
    d.extend_from_slice(&(children.len() as u32).to_le_bytes());
    for c in children {
        d.extend_from_slice(&c.to_le_bytes());
    }
    HircEntry { type_id: 7, object_id: id, data: d }
}

pub fn event(name: &str, actions: &[u32]) -> HircEntry {
    let mut d = vec![actions.len() as u8];
    for a in actions {
        d.extend_from_slice(&a.to_le_bytes());
    }
    HircEntry { type_id: 4, object_id: crate::fnv1_hash(name), data: d }
}

/// A Play (`0x0403`) or Stop (`0x0102`) action with an optional delay prop (ms).
pub fn action(id: u32, action_type: u16, target: u32, delay_ms: Option<u32>) -> HircEntry {
    let mut d = Vec::new();
    d.extend_from_slice(&action_type.to_le_bytes());
    d.extend_from_slice(&target.to_le_bytes());
    d.push(0);
    match delay_ms {
        Some(ms) => {
            d.extend_from_slice(&[1, crate::hirc::props::prop_id::DELAY_TIME]);
            d.extend_from_slice(&ms.to_le_bytes());
        }
        None => d.push(0),
    }
    d.push(0); // ranged
    if action_type >> 8 == 0x04 {
        d.push(0);
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
    } else {
        d.extend_from_slice(&[0, 0, 0]); // fade curve, flags, no exceptions
    }
    HircEntry { type_id: 3, object_id: id, data: d }
}

pub fn hierarchy(entries: Vec<HircEntry>) -> Hierarchy {
    Hierarchy::from_section(&HircSection { entries })
}

/// An ATL text document declaring `triggers` `(name, bank, duration type)`, and its index.
pub fn atl(triggers: &[(&str, &str, &str)], rtpcs: &[(&str, f32)]) -> crate::AtlIndex {
    let mut xml = String::from("<ATLConfig atl_name=\"T\"><AudioTriggers>");
    for (name, bank, dur) in triggers {
        xml += &format!(
            "<ATLTrigger atl_name=\"{name}\"><WwiseEvent wwise_name=\"{name}\" wwise_duration_type=\"{dur}\" wwise_radius_max=\"10\"/><ATLPreload atl_name=\"P_{bank}\"/></ATLTrigger>"
        );
    }
    xml += "</AudioTriggers><AudioPreloads>";
    for (_, bank, _) in triggers {
        xml += &format!("<ATLPreloadRequest atl_name=\"P_{bank}\"><WwiseFile wwise_name=\"{bank}\"/></ATLPreloadRequest>");
    }
    xml += "</AudioPreloads><AudioRTPCs>";
    for (name, default) in rtpcs {
        xml += &format!("<ATLRtpc atl_name=\"{name}\"><WwiseRtpc wwise_name=\"{name}\" min=\"0\" max=\"1\" default=\"{default}\"/></ATLRtpc>");
    }
    xml += "</AudioRTPCs></ATLConfig>";
    let doc = crate::game_audio::parse_document(xml.as_bytes()).unwrap().unwrap();
    crate::game_audio::index_documents([doc]).unwrap().0
}

/// An in-memory library of `(bank name, objects, embedded media)`.
pub fn library(atl: crate::AtlIndex, banks: Vec<(&str, Vec<HircEntry>, Vec<(u32, Vec<u8>)>)>) -> AudioLibrary<'static> {
    let banks = banks.into_iter().map(|(name, entries, media)| LoadedBank::from_parts(name, hierarchy(entries), &media)).collect();
    AudioLibrary::in_memory(atl, banks)
}

/// A bus (`aux` → type 18) with `(prop id, f32)` props, one optional effect slot and one optional volume RTPC curve.
pub fn bus(id: u32, parent: u32, aux: bool, props: &[(u8, f32)], effect: Option<u32>, rtpc: Option<(u32, Vec<(f32, f32, u32)>)>) -> HircEntry {
    bus_with_fx_bits(id, parent, aux, props, effect, 2, rtpc)
}

/// [`bus`] with the effect slot's bit vector (bit 0 bypass, bit 1 share set).
pub fn bus_with_fx_bits(id: u32, parent: u32, aux: bool, props: &[(u8, f32)], effect: Option<u32>, fx_bits: u8, rtpc: Option<(u32, Vec<(f32, f32, u32)>)>) -> HircEntry {
    let mut d = Vec::new();
    d.extend_from_slice(&parent.to_le_bytes());
    if parent == 0 {
        d.extend_from_slice(&0u32.to_le_bytes());
    }
    d.push(props.len() as u8);
    d.extend(props.iter().map(|p| p.0));
    for p in props {
        d.extend_from_slice(&p.1.to_le_bytes());
    }
    d.push(0); // positioning: inherit
    d.push(0); // aux flags
    d.extend_from_slice(&0u32.to_le_bytes());
    d.push(0);
    d.extend_from_slice(&0u16.to_le_bytes());
    d.extend_from_slice(&0x0002u32.to_le_bytes());
    d.push(0);
    d.extend_from_slice(&0i32.to_le_bytes());
    d.extend_from_slice(&0f32.to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes());
    match effect {
        Some(fx) => {
            d.extend_from_slice(&[1, 0, 0]);
            d.extend_from_slice(&fx.to_le_bytes());
            d.push(fx_bits);
        }
        None => d.push(0),
    }
    d.push(0); // metadata
    match rtpc {
        Some((rtpc, points)) => {
            d.extend_from_slice(&1u16.to_le_bytes());
            d.extend_from_slice(&rtpc.to_le_bytes());
            d.extend_from_slice(&[0, 0, 4]); // game parameter, accum, param 4 (bus volume)
            d.extend_from_slice(&0u32.to_le_bytes());
            d.push(2);
            d.extend_from_slice(&(points.len() as u16).to_le_bytes());
            for (x, y, i) in points {
                d.extend_from_slice(&x.to_le_bytes());
                d.extend_from_slice(&y.to_le_bytes());
                d.extend_from_slice(&i.to_le_bytes());
            }
        }
        None => d.extend_from_slice(&0u16.to_le_bytes()),
    }
    d.extend_from_slice(&[0, 0]); // state chunk
    HircEntry { type_id: if aux { 18 } else { 8 }, object_id: id, data: d }
}

/// A Convolution Reverb share set (type 16) with wet 0 dB, dry -96 dB and IR media `ir`.
pub fn convolution_share_set(id: u32, ir: u32) -> HircEntry {
    let mut d = Vec::new();
    d.extend_from_slice(&0x007F_0003u32.to_le_bytes());
    let mut params = Vec::new();
    for v in [0.0f32, 0.0, 180.0, 0.0, -96.3, 180.0, 0.0, 0.0, 0.0, -96.3, -96.0, 0.0] {
        params.extend_from_slice(&v.to_le_bytes());
    }
    params.extend_from_slice(&0u32.to_le_bytes());
    d.extend_from_slice(&(params.len() as u32).to_le_bytes());
    d.extend(params);
    d.push(1);
    d.push(0);
    d.extend_from_slice(&ir.to_le_bytes());
    d.extend_from_slice(&0u16.to_le_bytes()); // rtpc
    d.extend_from_slice(&[0, 0]); // state chunk
    d.extend_from_slice(&0u16.to_le_bytes()); // state property values
    HircEntry { type_id: 16, object_id: id, data: d }
}

/// A Wwise Peak Limiter share set (type 16): threshold, ratio, look-ahead, release, output level, process LFE, channel link.
pub fn peak_limiter_share_set(id: u32, threshold_db: f32, ratio: f32, look_ahead_s: f32, release_s: f32, output_db: f32) -> HircEntry {
    let mut d = Vec::new();
    d.extend_from_slice(&0x006E_0003u32.to_le_bytes());
    let mut params = Vec::new();
    for v in [threshold_db, ratio, look_ahead_s, release_s, output_db] {
        params.extend_from_slice(&v.to_le_bytes());
    }
    params.extend_from_slice(&[1, 1]);
    d.extend_from_slice(&(params.len() as u32).to_le_bytes());
    d.extend(params);
    d.push(0); // no media
    d.extend_from_slice(&0u16.to_le_bytes()); // rtpc
    d.extend_from_slice(&[0, 0]); // state chunk
    d.extend_from_slice(&0u16.to_le_bytes()); // state property values
    HircEntry { type_id: 16, object_id: id, data: d }
}
