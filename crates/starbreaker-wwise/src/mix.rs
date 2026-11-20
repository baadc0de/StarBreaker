//! The mix side of the banks: buses and their gains and effects, attenuation curves, and curve evaluation.
//!
//! - [`bus_chain`] / [`BusOut`]: a bus, its parent, its own voice volume (`Volume`, applied to each voice routed
//!   through it), bus volume (`BusVolume`, applied to the bus mix), make-up gain, RTPC curves (and their value at the
//!   RTPCs' ATL defaults) and effect slots (a Convolution Reverb slot carries its parameters and IR media id; Gain, Parametric EQ and
//!   Peak Limiter slots their parameters).
//! - [`AttenuationOut`]: the curves of an attenuation object by slot.
//! - [`evaluate`]: a Wwise curve at `x` (linear and constant interpolation exactly; the other named shapes by their
//!   documented forms, see the function).

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

/// A Wwise Parametric EQ (`CAkParameterEQFXParams`, three bands in v154).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ParametricEq {
    pub bands: Vec<EqBand>,
    pub output_level_db: f32,
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
        Some(Self { bands, output_level_db: f(51)? })
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
    pub make_up_gain_db: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub pitch_cents: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rtpc: Vec<RtpcCurve>,
    /// The gain RTPC curves (volume, bus volume, output-bus volume, make-up gain) evaluated at their RTPCs' ATL
    /// defaults (dB, summed).
    #[serde(skip_serializing_if = "is_zero")]
    pub rtpc_default_db: f32,
    /// RTPCs with a volume curve on this bus but no ATL default (not applied).
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
            match resolver.lib.atl.rtpc(&c.rtpc).and_then(|r| r.default) {
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
                    convolution: None,
                    ir_media: None,
                    gain_db: None,
                    eq: None,
                    limiter: None,
                };
                if let Some(HircObject::FxShareSet(f) | HircObject::FxCustom(f)) = resolver.lib.object(&banks, fx.fx_id) {
                    out.plugin = format!("{:#010x}", f.fx_id);
                    out.kind = effect_kind(f.fx_id);
                    let params = &f.plugin_params;
                    match f.fx_id {
                        0x007F_0003 => {
                            out.convolution = ConvolutionParams::parse(params);
                            out.ir_media = f.bank_data.first().map(|d| d.source_id);
                        }
                        0x008B_0003 => out.gain_db = params.get(0..4).map(|b| f32::from_le_bytes(b.try_into().unwrap())),
                        0x0069_0003 => out.eq = ParametricEq::parse(params),
                        0x006E_0003 => out.limiter = PeakLimiter::parse(params),
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
/// point's shape (`AkCurveInterpolation`): linear (4) and constant (9, holds the left value) are exact. The other
/// shapes use their named forms on the normalised segment t ∈ [0, 1]: log3 1−(1−t)³, log1 1−(1−t)^1.41, exp1 t^1.41,
/// exp3 t³, sine sin(πt/2), sine-recip 1−cos(πt/2), S-curve and inverse S-curve the cubic smoothstep and its inverse.
/// These shape formulas are the named curves, not code read from the engine (open item in the contract).
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
    let shaped = match a.2 {
        9 => 0.0,
        0 => 1.0 - (1.0 - t).powi(3),
        2 => 1.0 - (1.0 - t).powf(1.41),
        6 => t.powf(1.41),
        8 => t.powi(3),
        1 => (t * std::f32::consts::FRAC_PI_2).sin(),
        7 => 1.0 - (t * std::f32::consts::FRAC_PI_2).cos(),
        5 => t * t * (3.0 - 2.0 * t),
        3 => 0.5 - (((1.0 - 2.0 * t).asin()) / 3.0).sin(),
        _ => t,
    };
    a.1 + (b.1 - a.1) * shaped
}

#[cfg(test)]
#[path = "mix_tests.rs"]
mod tests;
