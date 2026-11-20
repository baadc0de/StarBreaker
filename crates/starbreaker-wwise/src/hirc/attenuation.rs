//! Attenuation objects (HIRC type 14), bank version 154 (Wwise 2023.1).
//!
//! Layout (wwiser `CAkAttenuation::SetInitialValues`, v137+): `u8 bIsHeightSpreadEnabled`, `u8 bIsConeEnabled`
//! (+ five `f32` cone values when set), 19 × `i8 curveToUse` (the curve index each [`AttenuationCurveSlot`] uses;
//! negative: none), `u8` curve count, each curve `u8 scaling, u16 n, n × (f32 x, f32 y, u32 interpolation)`, then the
//! initial RTPC list. The last x of the dry-volume curve is the attenuation's maximum distance.

use serde::Serialize;
use starbreaker_common::{ParseError, SpanReader};

use super::rtpc::{InitialRtpc, RtpcGraphPoint};

#[derive(Debug, Clone, Serialize)]
pub struct ConeParams {
    pub inside_degrees: f32,
    pub outside_degrees: f32,
    pub outside_volume: f32,
    pub lo_pass: f32,
    pub hi_pass: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttenuationCurve {
    /// Wwise `AkCurveScaling` (v72+): 0 none, 2 dB, 3 log, 4 dB-to-linear.
    pub scaling: u8,
    pub points: Vec<RtpcGraphPoint>,
}

/// The 19 curve slots of a v154 attenuation (`AkAttenuationCurveType`, Wwise 2023.1). Slots 0–6 are the ones the
/// pre-v142 seven-slot layout already had; 7–18 are obstruction, occlusion, diffraction and transmission curves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttenuationCurveSlot {
    VolumeDry = 0,
    VolumeAuxGameDefined = 1,
    VolumeAuxUserDefined = 2,
    LowPassFilter = 3,
    HighPassFilter = 4,
    Spread = 5,
    Focus = 6,
}

#[derive(Debug, Clone, Serialize)]
pub struct Attenuation {
    pub id: u32,
    pub height_spread: bool,
    pub cone: Option<ConeParams>,
    pub curve_to_use: [i8; 19],
    pub curves: Vec<AttenuationCurve>,
    pub rtpc: InitialRtpc,
}

impl Attenuation {
    pub fn parse(id: u32, reader: &mut SpanReader) -> Result<Self, ParseError> {
        let height_spread = reader.read_u8()? != 0;
        let is_cone_enabled = reader.read_u8()? & 1 != 0;
        let cone = if is_cone_enabled {
            Some(ConeParams {
                inside_degrees: reader.read_f32()?,
                outside_degrees: reader.read_f32()?,
                outside_volume: reader.read_f32()?,
                lo_pass: reader.read_f32()?,
                hi_pass: reader.read_f32()?,
            })
        } else {
            None
        };

        let mut curve_to_use = [0i8; 19];
        for slot in &mut curve_to_use {
            *slot = reader.read_i8()?;
        }

        let num_curves = reader.read_u8()? as usize;
        let mut curves = Vec::with_capacity(num_curves);
        for _ in 0..num_curves {
            let scaling = reader.read_u8()?;
            let num_points = reader.read_u16()? as usize;
            let mut points = Vec::with_capacity(num_points);
            for _ in 0..num_points {
                points.push(RtpcGraphPoint {
                    from: reader.read_f32()?,
                    to: reader.read_f32()?,
                    interp: reader.read_u32()?,
                });
            }
            curves.push(AttenuationCurve { scaling, points });
        }

        let rtpc = InitialRtpc::parse(reader)?;

        Ok(Attenuation { id, height_spread, cone, curve_to_use, curves, rtpc })
    }

    /// The curve a slot uses, if any.
    pub fn curve(&self, slot: AttenuationCurveSlot) -> Option<&AttenuationCurve> {
        let index = self.curve_to_use[slot as usize];
        usize::try_from(index).ok().and_then(|i| self.curves.get(i))
    }

    /// The dry-volume curve's last distance: where the sound reaches its last authored level.
    pub fn max_distance(&self) -> Option<f32> {
        self.curve(AttenuationCurveSlot::VolumeDry).and_then(|c| c.points.last()).map(|p| p.from)
    }
}

#[cfg(test)]
#[path = "attenuation_tests.rs"]
mod tests;
