//! A package's audio manifest (`audio.json`) and its media payloads, built from triggers and environments.
//!
//! [`ManifestBuilder`] collects the triggers a package references ([`ManifestBuilder::trigger`]) and the ATL
//! environments its rooms use ([`ManifestBuilder::environment`]), then [`ManifestBuilder::finish`] writes:
//! the resolved triggers and their shared node table ([`crate::playable`]), the attenuations and the bus chains met
//! ([`crate::mix`]), the RTPCs referenced (ATL range and default), every media file converted from Wwise Vorbis to
//! Ogg Vorbis (decoded only at play time), and each environment's aux bus with its Convolution Reverb impulse response
//! rebuilt to time-domain `f32` ([`crate::convolution`]). The contract is `docs/decomposed-export-contract.md`
//! §Audio.

use std::collections::BTreeMap;

use rayon::prelude::*;
use serde::Serialize;

use crate::hirc::HircObject;
use crate::library::{AudioLibrary, MediaSource};
use crate::mix::{AttenuationOut, BusOut, bus_chain};
use crate::playable::{PlayNode, Resolver, TriggerOut};

/// The manifest format version.
pub const AUDIO_MANIFEST_VERSION: u32 = 1;

/// One media file of the package.
#[derive(Debug, Clone, Serialize)]
pub struct MediaOut {
    /// Package-relative path of the Ogg file (absent when the media could not be converted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub codec: String,
    pub channels: u16,
    pub rate: u32,
    /// Duration from the last Ogg granule position.
    pub seconds: f32,
    pub source: MediaSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// An ATL RTPC a node or bus curve depends on.
#[derive(Debug, Clone, Serialize)]
pub struct RtpcOut {
    pub wwise_name: String,
    pub min: Option<f32>,
    pub max: Option<f32>,
    /// The ATL default (`WwiseRtpc default`).
    pub default: Option<f32>,
    /// The Wwise game parameter's default (`Init.bnk` STMG): the value until the game sets the RTPC.
    pub wwise_default: Option<f32>,
}

/// A game environment's DataCore record (`AudioEnvironment`): the room sizes and interiority it is chosen for.
#[derive(Debug, Clone, Default, Serialize)]
pub struct EnvironmentRecord {
    pub record: String,
    pub size: (f32, f32),
    pub interiority: (f32, f32),
}

/// An ATL environment: its aux bus and the impulse response of the bus's Convolution Reverb.
#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentOut {
    pub aux_bus: u32,
    #[serde(flatten)]
    pub record: EnvironmentRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impulse_response: Option<ImpulseResponseOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A decoded impulse response written to the package.
#[derive(Debug, Clone, Serialize)]
pub struct ImpulseResponseOut {
    pub media: u32,
    /// Package-relative path: interleaved little-endian `f32`, `frames × channels`.
    pub file: String,
    pub channels: u16,
    pub rate: u32,
    pub frames: usize,
    /// Measured on the decoded IR: peak (dBFS), Schroeder T20 and T30 extrapolated to 60 dB (s), onset (s).
    pub peak_db: f32,
    pub header_peak_db: f32,
    pub t20_s: Option<f32>,
    pub t30_s: Option<f32>,
    pub onset_s: f32,
}

/// `audio.json`.
#[derive(Debug, Serialize)]
pub struct AudioManifest {
    pub version: u32,
    pub triggers: BTreeMap<String, TriggerOut>,
    pub nodes: Vec<PlayNode>,
    pub attenuations: BTreeMap<String, AttenuationOut>,
    pub buses: BTreeMap<String, BusOut>,
    pub media: BTreeMap<String, MediaOut>,
    pub rtpcs: BTreeMap<String, RtpcOut>,
    pub environments: BTreeMap<String, EnvironmentOut>,
    #[serde(flatten)]
    pub global_settings: Option<crate::stmg::GlobalSettings>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub environmental_curves: Vec<crate::stmg::EnvironmentalCurve>,
}

/// A file to add to the package.
pub struct Payload {
    pub path: String,
    pub bytes: Vec<u8>,
}

pub struct ManifestBuilder<'l, 'p> {
    resolver: Resolver<'l, 'p>,
    environments: BTreeMap<String, EnvironmentRecord>,
    /// Package-relative directory of the media files (`Audio`).
    dir: String,
}

impl<'l, 'p> ManifestBuilder<'l, 'p> {
    pub fn new(lib: &'l AudioLibrary<'p>, dir: &str) -> Self {
        Self { resolver: Resolver::new(lib), environments: BTreeMap::new(), dir: dir.trim_end_matches('/').to_owned() }
    }

    /// Resolve a trigger (once); returns its status.
    pub fn trigger(&mut self, name: &str) -> &'static str {
        self.resolver.trigger(name).status
    }

    /// Add an ATL environment with its DataCore record.
    pub fn environment(&mut self, atl_name: &str, record: EnvironmentRecord) {
        self.environments.insert(atl_name.to_owned(), record);
    }

    pub fn resolver(&self) -> &Resolver<'l, 'p> {
        &self.resolver
    }

    /// Write the manifest and its media and impulse-response payloads.
    pub fn finish(self) -> (AudioManifest, Vec<Payload>) {
        let r = self.resolver;
        let lib = r.lib;
        let init = lib.init_bank();
        let mut attenuations = BTreeMap::new();
        for (&id, home) in &r.out.attenuations {
            let banks: Vec<_> = lib.bank(home).into_iter().chain(init.clone()).collect();
            if let Some(HircObject::Attenuation(a)) = lib.object(&banks, id) {
                attenuations.insert(id.to_string(), AttenuationOut::from_attenuation(a));
            }
        }
        let mut buses = BTreeMap::new();
        let mut environments = BTreeMap::new();
        let mut ir_media = Vec::new();
        for (name, record) in &self.environments {
            let Some(env) = lib.atl.environment(name) else {
                environments.insert(name.clone(), EnvironmentOut { aux_bus: 0, record: record.clone(), impulse_response: None, error: Some("no ATLEnvironment of this name".into()) });
                continue;
            };
            let aux = crate::fnv1_hash(&env.aux_bus);
            let chain = bus_chain(&r, aux);
            let ir = chain.first().and_then(|(_, b)| b.effects.iter().find_map(|e| e.ir_media));
            for (id, bus) in chain {
                buses.insert(id.to_string(), bus);
            }
            if let Some(ir) = ir {
                ir_media.push((name.clone(), ir));
            }
            environments.insert(name.clone(), EnvironmentOut { aux_bus: aux, record: record.clone(), impulse_response: None, error: ir.is_none().then(|| "the aux bus has no Convolution Reverb slot".into()) });
        }
        for &bus in r.out.buses.keys() {
            for (id, out) in bus_chain(&r, bus) {
                buses.entry(id.to_string()).or_insert(out);
            }
        }
        let mut rtpcs = BTreeMap::new();
        let defaults = init.as_ref().map(|b| b.game_parameter_defaults.clone()).unwrap_or_default();
        // An RTPC the ATL names, or one it does not (listed under its id in hex, as the curves name it) with the Wwise
        // game parameter's default.
        let mut note_rtpc_id = |name: &str, id: u32| {
            if let Some(a) = lib.atl.rtpc(name) {
                let wwise_default = defaults.get(&crate::fnv1_hash(&a.wwise_name)).copied();
                rtpcs.entry(name.to_owned()).or_insert(RtpcOut { wwise_name: a.wwise_name.clone(), min: a.min, max: a.max, default: a.default, wwise_default });
            } else if let Some(&d) = defaults.get(&id) {
                rtpcs.entry(name.to_owned()).or_insert(RtpcOut { wwise_name: name.to_owned(), min: None, max: None, default: None, wwise_default: Some(d) });
            }
        };
        for node in &r.out.nodes {
            node.props.rtpc.iter().for_each(|c| note_rtpc_id(&c.rtpc, c.rtpc_id));
        }
        for t in r.out.triggers.values() {
            t.play.iter().flat_map(|p| &p.context.rtpc).for_each(|c| note_rtpc_id(&c.rtpc, c.rtpc_id));
            t.rtpc_dependees.iter().for_each(|d| note_rtpc_id(d, 0));
        }
        for b in buses.values() {
            b.rtpc.iter().for_each(|c| note_rtpc_id(&c.rtpc, c.rtpc_id));
            for e in &b.effects {
                e.rtpc.iter().for_each(|c| note_rtpc_id(&c.rtpc, c.rtpc_id));
                if let Some(m) = &e.meter {
                    note_rtpc_id(&m.game_parameter, m.game_parameter_id);
                }
            }
        }

        // Media: read and convert in parallel (the conversion validates each stream by decoding it once).
        let jobs: Vec<(u32, String, bool)> = r.out.media_homes.iter().map(|(&id, (home, lang))| (id, home.clone(), *lang)).collect();
        let dir = self.dir.clone();
        let converted: Vec<(u32, MediaOut, Option<Payload>)> = jobs
            .par_iter()
            .map(|(id, home, lang)| convert_media(lib, *id, home, *lang, &dir))
            .collect();
        let mut media = BTreeMap::new();
        let mut payloads = Vec::new();
        for (id, out, payload) in converted {
            media.insert(id.to_string(), out);
            payloads.extend(payload);
        }

        for (name, ir) in ir_media {
            let found = lib.media(ir, None, false);
            let env = environments.get_mut(&name).unwrap();
            let Some((bytes, _)) = found else {
                env.error = Some(format!("impulse response media {ir} not found"));
                continue;
            };
            match crate::convolution::decode_impulse_response(&bytes) {
                Ok(decoded) => {
                    let file = format!("{dir}/ir/{ir}.f32");
                    env.impulse_response = Some(ImpulseResponseOut {
                        media: ir,
                        file: file.clone(),
                        channels: decoded.channels,
                        rate: decoded.sample_rate,
                        frames: decoded.frames(),
                        peak_db: decoded.peak_db(),
                        header_peak_db: decoded.header_peak_db,
                        t20_s: decoded.reverb_time(20.0),
                        t30_s: decoded.reverb_time(30.0),
                        onset_s: decoded.onset_s(),
                    });
                    if !payloads.iter().any(|p: &Payload| p.path == file) {
                        payloads.push(Payload { path: file, bytes: decoded.samples.iter().flat_map(|s| s.to_le_bytes()).collect() });
                    }
                }
                Err(e) => env.error = Some(e.to_string()),
            }
        }

        let manifest = AudioManifest {
            version: AUDIO_MANIFEST_VERSION,
            triggers: r.out.triggers,
            nodes: r.out.nodes,
            attenuations,
            buses,
            media,
            rtpcs,
            environments,
            global_settings: init.as_ref().and_then(|bank| bank.global_settings.clone()),
            environmental_curves: init.as_ref().map(|bank| bank.environment_curves.clone()).unwrap_or_default(),
        };
        (manifest, payloads)
    }
}

fn convert_media(lib: &AudioLibrary<'_>, id: u32, home: &str, language_specific: bool, dir: &str) -> (u32, MediaOut, Option<Payload>) {
    let home_bank = lib.bank(home);
    let Some((wem, source)) = lib.media(id, home_bank.as_deref(), language_specific) else {
        let out = MediaOut { file: None, codec: String::new(), channels: 0, rate: 0, seconds: 0.0, source: MediaSource::Bank { bank: home.to_owned() }, error: Some("media not found".into()) };
        return (id, out, None);
    };
    let fail = |codec: String, e: String| MediaOut { file: None, codec, channels: 0, rate: 0, seconds: 0.0, source: source.clone(), error: Some(e) };
    let wem_file = match starbreaker_wem::WemFile::parse(&wem) {
        Ok(w) => w,
        Err(e) => return (id, fail(String::new(), e.to_string()), None),
    };
    let codec = format!("{:?}", wem_file.codec_type()).to_ascii_lowercase();
    if wem_file.codec_type() != starbreaker_wem::WemCodec::Vorbis {
        return (id, fail(codec, "codec not decodable (only Wwise Vorbis is)".into()), None);
    }
    let (channels, rate) = (wem_file.channels(), wem_file.sample_rate());
    match starbreaker_wem::decode::vorbis_to_ogg(&wem) {
        Ok(ogg) => {
            let seconds = last_granule(&ogg).map_or(0.0, |g| g as f32 / rate.max(1) as f32);
            let file = format!("{dir}/{id}.ogg");
            let out = MediaOut { file: Some(file.clone()), codec, channels, rate, seconds, source, error: None };
            (id, out, Some(Payload { path: file, bytes: ogg }))
        }
        Err(e) => (id, fail(codec, e.to_string()), None),
    }
}

/// The granule position of an Ogg stream's last complete page (its sample count for Vorbis), walking the pages from
/// the start (header: capture pattern, version, type, granule at 6, serial, sequence, CRC, segment count at 26, the
/// lacing table, then the body). Pages without a granule (-1) are skipped.
fn last_granule(ogg: &[u8]) -> Option<u64> {
    let mut at = 0;
    let mut last = None;
    while ogg.get(at..at + 4) == Some(b"OggS") {
        let Some(&segments) = ogg.get(at + 26) else { break };
        let Some(lacing) = ogg.get(at + 27..at + 27 + usize::from(segments)) else { break };
        let end = at + 27 + lacing.len() + lacing.iter().map(|&l| usize::from(l)).sum::<usize>();
        if end > ogg.len() {
            break;
        }
        let granule = u64::from_le_bytes(ogg[at + 6..at + 14].try_into().unwrap());
        if granule != u64::MAX {
            last = Some(granule);
        }
        at = end;
    }
    last
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
