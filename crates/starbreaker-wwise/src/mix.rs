//! The mix side of the banks: buses and their gains and effects, attenuation curves, and curve evaluation.
//!
//! - [`bus_chain`] / [`BusOut`]: a bus, its parent, its own voice volume (`Volume`, applied to each voice routed
//!   through it), bus volume (`BusVolume`, applied to the bus mix), make-up gain, RTPC curves (and their value at the
//!   RTPCs' defaults: ATL, else the Wwise game parameter's) and effect slots (a Convolution Reverb slot carries its parameters and IR media id; Gain, Parametric EQ and
//!   Peak Limiter slots their parameters; a Meter slot its ballistics and the game parameter it writes; every slot the
//!   RTPC curves on its own effect parameters).
//! - [`AttenuationOut`]: the curves of an attenuation object by slot.
//! - [`evaluate`]: a Wwise curve at `x`, every `AkCurveInterpolation` shape (see the function).

use serde::Serialize;

use crate::convolution::ConvolutionParams;
use crate::hirc::attenuation::{Attenuation, AttenuationCurveSlot};
use crate::hirc::props::prop_id;
use crate::hirc::HircObject;
use crate::playable::{Resolver, RtpcCurve};

/// A curve as `(x, y, interpolation)` points with its scaling.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Curve {
    pub scaling: u8,
    pub points: Vec<(f32, f32, u32)>,
}

/// An attenuation object's curves.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AttenuationOut {
    /// The dry-volume curve's last x (game units, metres).
    pub max_distance: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cone: Option<[f32; 5]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_dry: Option<Curve>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_aux_game: Option<Curve>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_aux_user: Option<Curve>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lpf: Option<Curve>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hpf: Option<Curve>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spread: Option<Curve>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<Curve>,
}

impl AttenuationOut {
    pub fn from_attenuation(a: &Attenuation) -> Self {
        let curve = |slot| {
            a.curve(slot).map(|c| Curve { scaling: c.scaling, points: c.points.iter().map(|p| (p.from, p.to, p.interp)).collect() })
        };
        Self {
            max_distance: a.max_distance(),
            cone: a.cone.as_ref().map(|c| [c.inside_degrees, c.outside_degrees, c.outside_volume, c.lo_pass, c.hi_pass]),
            volume_dry: curve(AttenuationCurveSlot::VolumeDry),
            volume_aux_game: curve(AttenuationCurveSlot::VolumeAuxGameDefined),
            volume_aux_user: curve(AttenuationCurveSlot::VolumeAuxUserDefined),
            lpf: curve(AttenuationCurveSlot::LowPassFilter),
            hpf: curve(AttenuationCurveSlot::HighPassFilter),
            spread: curve(AttenuationCurveSlot::Spread),
            focus: curve(AttenuationCurveSlot::Focus),
        }
    }
}

/// One effect slot of a bus.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EffectOut {
    pub slot: u8,
    /// The share set / custom effect object id.
    pub id: u32,
    /// The plugin id in hex (`0x007f0003` Convolution Reverb, `0x00760003` RoomVerb, …).
    pub plugin: String,
    pub kind: &'static str,
    /// The slot is bypassed (the bus's bypass-all byte, or bit 0 of the slot's bit vector): not applied.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub convolution: Option<ConvolutionParams>,
    /// The impulse-response media of a Convolution Reverb.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ir_media: Option<u32>,
    /// Wwise Gain (`CAkGainFXParams`: full-band gain, LFE gain; dB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain_db: Option<f32>,
    /// Wwise Parametric EQ (v154: three bands and an output level).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq: Option<ParametricEq>,
    /// Wwise Peak Limiter (`CAkPeakLimiterFXParams`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limiter: Option<PeakLimiter>,
    /// Wwise Meter (`CAkMeterFXParams`): measures the bus and writes a game parameter (a side-chain source).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meter: Option<Meter>,
    /// RTPC curves on the effect's own parameters (`param` is `fx_param_<plugin parameter id>`; the id's meaning is
    /// the plugin's).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rtpc: Vec<RtpcCurve>,
    /// The bus's state groups that bypass this slot in some of their states (`BypassFX` 0x30 of the slot, the slot
    /// number in the high byte, or `BypassAllFX` 0x31): the slot is on or off depending on the game's state.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub state_bypass: Vec<StateBypass>,
}

/// A state group and the states (ids) in which it bypasses an effect slot.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StateBypass {
    pub group: u32,
    pub states: Vec<u32>,
}

/// A Wwise Meter (`CAkMeterFXParams`, bank v154, 28 bytes: attack s, release s, min dB, max dB, hold s, infinite hold,
/// mode, scope, apply downstream volume, game parameter id).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Meter {
    pub attack_s: f32,
    pub release_s: f32,
    pub min_db: f32,
    pub max_db: f32,
    pub hold_s: f32,
    pub infinite_hold: bool,
    /// `peak` or `rms` (`AkMeterMode`).
    pub mode: &'static str,
    /// `global` or `game_object` (`AkMeterScope`).
    pub scope: &'static str,
    pub apply_downstream_volume: bool,
    /// The written game parameter's ATL name (its id in hex when the ATL does not name it).
    pub game_parameter: String,
    pub game_parameter_id: u32,
}

impl Meter {
    pub fn parse(b: &[u8], name: impl Fn(u32) -> String) -> Option<Self> {
        let f = |o: usize| b.get(o..o + 4).map(|x| f32::from_le_bytes(x.try_into().unwrap()));
        let id = b.get(24..28).map(|x| u32::from_le_bytes(x.try_into().unwrap()))?;
        Some(Self {
            attack_s: f(0)?,
            release_s: f(4)?,
            min_db: f(8)?,
            max_db: f(12)?,
            hold_s: f(16)?,
            infinite_hold: *b.get(20)? != 0,
            mode: if *b.get(21)? == 1 { "rms" } else { "peak" },
            scope: if *b.get(22)? == 1 { "game_object" } else { "global" },
            apply_downstream_volume: *b.get(23)? != 0,
            game_parameter: name(id),
            game_parameter_id: id,
        })
    }
}

/// A Wwise Peak Limiter (`CAkPeakLimiterFXParams`: threshold dB, ratio, look-ahead s, release s, output level dB,
/// process LFE, channel link).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PeakLimiter {
    pub threshold_db: f32,
    pub ratio: f32,
    pub look_ahead_s: f32,
    pub release_s: f32,
    pub output_level_db: f32,
    pub process_lfe: bool,
    pub channel_link: bool,
}

impl PeakLimiter {
    pub fn parse(b: &[u8]) -> Option<Self> {
        let f = |o: usize| b.get(o..o + 4).map(|x| f32::from_le_bytes(x.try_into().unwrap()));
        Some(Self {
            threshold_db: f(0)?,
            ratio: f(4)?,
            look_ahead_s: f(8)?,
            release_s: f(12)?,
            output_level_db: f(16)?,
            process_lfe: *b.get(20)? != 0,
            channel_link: *b.get(21)? != 0,
        })
    }
}

/// A Wwise Parametric EQ (`CAkParameterEQFXParams`, three bands in v154, 56 bytes: bands, output level, process LFE).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ParametricEq {
    pub bands: Vec<EqBand>,
    pub output_level_db: f32,
    pub process_lfe: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EqBand {
    /// lowpass, highpass, bandpass, notch, lowshelf, highshelf, peaking.
    pub filter: &'static str,
    pub gain_db: f32,
    pub frequency_hz: f32,
    pub q: f32,
    pub on: bool,
}

impl ParametricEq {
    pub fn parse(b: &[u8]) -> Option<Self> {
        let f = |o: usize| b.get(o..o + 4).map(|x| f32::from_le_bytes(x.try_into().unwrap()));
        let mut bands = Vec::new();
        for i in 0..3 {
            let o = i * 17;
            let kind = b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()))?;
            bands.push(EqBand {
                filter: ["lowpass", "highpass", "bandpass", "notch", "lowshelf", "highshelf", "peaking"].get(kind as usize).copied().unwrap_or("unknown"),
                gain_db: f(o + 4)?,
                frequency_hz: f(o + 8)?,
                q: f(o + 12)?,
                on: *b.get(o + 16)? != 0,
            });
        }
        Some(Self { bands, output_level_db: f(51)?, process_lfe: *b.get(55)? != 0 })
    }
}

/// A bus of the mix.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BusOut {
    /// The bus name when the data names it (ATL environments name their aux buses).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<u32>,
    pub aux: bool,
    pub voice_volume_db: f32,
    pub bus_volume_db: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub output_bus_lpf: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub output_bus_hpf: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub make_up_gain_db: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub pitch_cents: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rtpc: Vec<RtpcCurve>,
    /// The gain RTPC curves (volume, bus volume, output-bus volume, make-up gain) evaluated at their RTPCs' defaults
    /// ([`rtpc_default`]: ATL, else the Wwise game parameter's; dB, summed).
    #[serde(skip_serializing_if = "is_zero")]
    pub rtpc_default_db: f32,
    /// RTPCs with a volume curve on this bus but no default (not applied).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rtpc_without_default: Vec<String>,
    /// State groups with property offsets on this bus (applied only while the game sets that state; not modelled).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub state_groups: Vec<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectOut>,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

fn effect_kind(plugin: u32) -> &'static str {
    match plugin {
        0x007F_0003 => "convolution_reverb",
        0x0076_0003 => "room_verb",
        0x0073_0003 => "matrix_reverb",
        0x0069_0003 => "parametric_eq",
        0x006A_0003 => "delay",
        0x006C_0003 => "compressor",
        0x006E_0003 => "peak_limiter",
        0x0081_0003 => "meter",
        0x0087_0003 => "stereo_delay",
        0x008B_0003 => "gain",
        p if (p >> 4) & 0xFFF != 0 => "third_party",
        _ => "other",
    }
}

/// `bus` and its ancestors (nearest first) as `(id, BusOut)`, from `Init.bnk`.
pub fn bus_chain(resolver: &Resolver<'_, '_>, bus: u32) -> Vec<(u32, BusOut)> {
    let Some(init) = resolver.lib.init_bank() else { return Vec::new() };
    let banks = [init];
    let mut out = Vec::new();
    let mut current = Some(bus);
    while let Some(id) = current {
        if out.iter().any(|(b, _)| *b == id) || out.len() > 32 {
            break;
        }
        let (object, aux) = match resolver.lib.object(&banks, id) {
            Some(HircObject::AudioBus(b)) => (b, false),
            Some(HircObject::AuxBus(b)) => (b, true),
            _ => break,
        };
        let mut rtpc_default_db = 0.0;
        let mut rtpc_without_default = Vec::new();
        let curves = resolver.curves(&object.rtpc);
        for c in &curves {
            if !is_gain_param(&c.param) {
                continue;
            }
            match rtpc_default(resolver, &c.rtpc, c.rtpc_id) {
                Some(x) if c.rtpc_type == 0 => rtpc_default_db += to_db(c.scaling, evaluate(&c.points, x)),
                _ => rtpc_without_default.push(c.rtpc.clone()),
            }
        }
        let effects = object
            .fx_params
            .fx_list
            .iter()
            .map(|fx| {
                let mut out = EffectOut {
                    slot: fx.fx_index,
                    id: fx.fx_id,
                    plugin: String::new(),
                    kind: "missing",
                    bypass: object.fx_params.bypass_all || fx.bits & 0x01 != 0,
                    convolution: None,
                    ir_media: None,
                    gain_db: None,
                    eq: None,
                    limiter: None,
                    meter: None,
                    rtpc: Vec::new(),
                    state_bypass: state_bypass(&object.state_chunk, fx.fx_index),
                };
                if let Some(HircObject::FxShareSet(f) | HircObject::FxCustom(f)) = resolver.lib.object(&banks, fx.fx_id) {
                    out.plugin = format!("{:#010x}", f.fx_id);
                    out.kind = effect_kind(f.fx_id);
                    let params = &f.plugin_params;
                    // Effect RTPCs drive the plugin's own parameter ids, not the node property table.
                    out.rtpc = resolver.curves(&f.rtpc);
                    for (c, raw) in out.rtpc.iter_mut().zip(&f.rtpc.curves) {
                        c.param = format!("fx_param_{}", raw.param_id);
                    }
                    match f.fx_id {
                        0x007F_0003 => {
                            out.convolution = ConvolutionParams::parse(params);
                            out.ir_media = f.bank_data.first().map(|d| d.source_id);
                        }
                        0x008B_0003 => out.gain_db = params.get(0..4).map(|b| f32::from_le_bytes(b.try_into().unwrap())),
                        0x0069_0003 => out.eq = ParametricEq::parse(params),
                        0x006E_0003 => out.limiter = PeakLimiter::parse(params),
                        0x0081_0003 => out.meter = Meter::parse(params, |id| resolver.rtpc_name(id)),
                        _ => {}
                    }
                }
                out
            })
            .collect();
        let name = resolver.lib.atl.environments().find(|e| crate::fnv1_hash(&e.aux_bus) == id).map(|e| e.aux_bus.clone());
        out.push((
            id,
            BusOut {
                name,
                parent: object.parent(),
                aux,
                voice_volume_db: object.prop(prop_id::VOLUME).unwrap_or(0.0),
                bus_volume_db: object.prop(prop_id::BUS_VOLUME).unwrap_or(0.0),
                output_bus_lpf: object.prop(prop_id::OUTPUT_BUS_LPF).unwrap_or(0.0),
                output_bus_hpf: object.prop(prop_id::OUTPUT_BUS_HPF).unwrap_or(0.0),
                make_up_gain_db: object.prop(prop_id::MAKE_UP_GAIN).unwrap_or(0.0),
                pitch_cents: object.prop(prop_id::PITCH).unwrap_or(0.0),
                rtpc: curves,
                rtpc_default_db,
                rtpc_without_default,
                state_groups: object.state_chunk.state_groups.iter().map(|g| g.group_id).collect(),
                effects,
            },
        ));
        current = object.parent();
    }
    out
}

/// The value of a game parameter until the game sets it: its ATL default, else the Wwise game parameter's default
/// (`Init.bnk` STMG) — one resolver for the static bus levels and the manifest's `rtpcs`.
pub fn rtpc_default(resolver: &Resolver<'_, '_>, name: &str, id: u32) -> Option<f32> {
    resolver.lib.atl.rtpc(name).and_then(|r| r.default).or_else(|| resolver.lib.init_bank().and_then(|b| b.game_parameter_defaults.get(&id).copied()))
}

/// The state groups of a bus that bypass effect slot `slot` in some state, with those states.
fn state_bypass(chunk: &crate::hirc::rtpc::StateChunk, slot: u8) -> Vec<StateBypass> {
    chunk
        .state_groups
        .iter()
        .filter_map(|g| {
            let states: Vec<u32> = g
                .states
                .iter()
                .filter(|s| s.properties.props.iter().any(|&(id, v)| v != 0.0 && (id == 0x31 || id == 0x30 | (u16::from(slot) << 8))))
                .map(|s| s.state_id)
                .collect();
            (!states.is_empty()).then_some(StateBypass { group: g.group_id, states })
        })
        .collect()
}

/// Properties whose values are gains in dB (they add along buses and nodes).
pub fn is_gain_param(param: &str) -> bool {
    matches!(param, "volume" | "bus_volume" | "output_bus_volume" | "make_up_gain")
}

/// A gain curve value in dB. Curves with dB scaling (2) store `10^(dB/20) − 1`: 0 is unity, −1 silence (the C2
/// banks' values hit round dB exactly: −0.2920542 = −3 dB, −0.369043 = −4 dB, −0.4988128 = −6 dB, −0.7488114 = −12 dB).
/// Unscaled curves (0) on a volume property hold dB directly. Silence is returned as −200 dB.
pub fn to_db(scaling: u8, y: f32) -> f32 {
    if scaling == 2 {
        let linear = 1.0 + y;
        if linear <= 1e-10 { -200.0 } else { 20.0 * linear.log10() }
    } else {
        y
    }
}

/// A Wwise curve at `x` (clamped to its first and last points). Interpolation between two points follows the left
/// point's shape (`AkCurveInterpolation`) on the normalised segment t ∈ [0, 1], in the curve's stored units (a dB-scaled
/// curve converts after interpolating): linear (4) t, constant (9) holds the left value, log3 (0) 1−(1−t)³, sine (1)
/// sin(πt/2), log1 (2) t(3−t)/2, inverse S-curve (3) sin(πt)/2 below t = ½ and 1−sin(πt)/2 above, S-curve (5)
/// (1−cos πt)/2, exp1 (6) (t+1)t/2, sine reciprocal (7) 1−cos(πt/2), exp3 (8) t³. These are the closed forms of
/// Wwise's `AkInterpolation` (its polynomial approximations of the trigonometric shapes agree to ~1e-5) as wwiser's
/// renderer reproduces it (`generator/render/bnode_rtpc.py`).
pub fn evaluate(points: &[(f32, f32, u32)], x: f32) -> f32 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else { return 0.0 };
    if x <= first.0 {
        return first.1;
    }
    if x >= last.0 {
        return last.1;
    }
    let i = points.windows(2).position(|w| x >= w[0].0 && x < w[1].0).unwrap_or(0);
    let (a, b) = (points[i], points[i + 1]);
    let t = if b.0 > a.0 { (x - a.0) / (b.0 - a.0) } else { 0.0 };
    use std::f32::consts::{FRAC_PI_2, PI};
    let shaped = match a.2 {
        9 => 0.0,
        0 => 1.0 - (1.0 - t).powi(3),
        1 => (t * FRAC_PI_2).sin(),
        2 => t * (3.0 - t) * 0.5,
        3 if t > 0.5 => 1.0 - (t * PI).sin() * 0.5,
        3 => (t * PI).sin() * 0.5,
        5 => (1.0 - (t * PI).cos()) * 0.5,
        6 => (t + 1.0) * t * 0.5,
        7 => 1.0 - (t * FRAC_PI_2).cos(),
        8 => t * t * t,
        _ => t,
    };
    a.1 + (b.1 - a.1) * shaped
}

#[cfg(test)]
#[path = "mix_tests.rs"]
mod tests;
