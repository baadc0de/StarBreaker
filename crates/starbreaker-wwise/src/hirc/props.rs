//! Property bundles (`AkPropBundle`) and ranged modifiers, and the bank-version-154 property ids ([`prop_id`]).

use serde::Serialize;
use starbreaker_common::{ParseError, SpanReader};

#[derive(Debug, Clone, Serialize)]
pub struct AkProp {
    pub id: u8,
    pub value: u32,
}

impl AkProp {
    pub fn as_f32(&self) -> f32 {
        f32::from_bits(self.value)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AkPropBundle {
    pub props: Vec<AkProp>,
}

impl AkPropBundle {
    pub fn parse(reader: &mut SpanReader) -> Result<Self, ParseError> {
        let count = reader.read_u8()? as usize;
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            ids.push(reader.read_u8()?);
        }
        let mut props = Vec::with_capacity(count);
        for i in 0..count {
            props.push(AkProp {
                id: ids[i],
                value: reader.read_u32()?,
            });
        }
        Ok(AkPropBundle { props })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RangedModifier {
    pub id: u8,
    pub min: u32,
    pub max: u32,
}

impl RangedModifier {
    pub fn min_f32(&self) -> f32 { f32::from_bits(self.min) }
    pub fn max_f32(&self) -> f32 { f32::from_bits(self.max) }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RangedModifiers {
    pub modifiers: Vec<RangedModifier>,
}

impl RangedModifiers {
    pub fn parse(reader: &mut SpanReader) -> Result<Self, ParseError> {
        let count = reader.read_u8()? as usize;
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            ids.push(reader.read_u8()?);
        }
        let mut modifiers = Vec::with_capacity(count);
        for i in 0..count {
            let min = reader.read_u32()?;
            let max = reader.read_u32()?;
            modifiers.push(RangedModifier { id: ids[i], min, max });
        }
        Ok(RangedModifiers { modifiers })
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AkPropBundleF32 {
    pub props: Vec<(u16, f32)>,
}

impl AkPropBundleF32 {
    pub fn parse(reader: &mut SpanReader) -> Result<Self, ParseError> {
        let count = reader.read_u16()? as usize;
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            ids.push(reader.read_u16()?);
        }
        let mut props = Vec::with_capacity(count);
        for i in 0..count {
            props.push((ids[i], reader.read_f32()?));
        }
        Ok(AkPropBundleF32 { props })
    }
}

impl AkPropBundle {
    /// The value of property `id` as `f32`, if set.
    pub fn f32(&self, id: u8) -> Option<f32> {
        self.props.iter().find(|p| p.id == id).map(AkProp::as_f32)
    }
    /// The value of property `id` as a raw `u32` (ids, counts), if set.
    pub fn u32(&self, id: u8) -> Option<u32> {
        self.props.iter().find(|p| p.id == id).map(|p| p.value)
    }
}

impl RangedModifiers {
    /// The `(min, max)` range of property `id`, if randomised.
    pub fn range(&self, id: u8) -> Option<(f32, f32)> {
        self.modifiers.iter().find(|m| m.id == id).map(|m| (m.min_f32(), m.max_f32()))
    }
}

/// `AkPropID` for bank version 154 (Wwise 2023.1; wwiser `AkPropID_154`). Ids 0..=0x38 double as RTPC parameter ids in
/// this version (wwiser: "AkRTPC_ParameterID_150 = AkPropID_150"). Checked against the C2 banks: `LOOP` holds loop
/// counts on sounds (0 = infinite), `ATTENUATION_ID` names attenuation objects (25 of 25 sampled values).
pub mod prop_id {
    pub const VOLUME: u8 = 0x00;
    pub const PITCH: u8 = 0x01;
    pub const LPF: u8 = 0x02;
    pub const HPF: u8 = 0x03;
    pub const BUS_VOLUME: u8 = 0x04;
    pub const MAKE_UP_GAIN: u8 = 0x05;
    pub const PRIORITY: u8 = 0x06;
    pub const USER_AUX_SEND_VOLUME_0: u8 = 0x08;
    pub const GAME_AUX_SEND_VOLUME: u8 = 0x0C;
    pub const OUTPUT_BUS_VOLUME: u8 = 0x0D;
    pub const OUTPUT_BUS_HPF: u8 = 0x0E;
    pub const OUTPUT_BUS_LPF: u8 = 0x0F;
    pub const GAME_AUX_SEND_LPF: u8 = 0x18;
    pub const GAME_AUX_SEND_HPF: u8 = 0x19;
    pub const INITIAL_DELAY: u8 = 0x22;
    pub const CENTER_PERCENT: u8 = 0x29;
    pub const ATTENUATION_DISTANCE_SCALING: u8 = 0x38;
    pub const DELAY_TIME: u8 = 0x3A;
    pub const TRANSITION_TIME: u8 = 0x3B;
    pub const PROBABILITY: u8 = 0x3C;
    pub const LOOP: u8 = 0x4A;
    pub const ATTENUATION_ID: u8 = 0x4B;

    /// The name of an RTPC parameter id (the additive and overridable props, 0..=0x38).
    pub fn rtpc_param_name(id: u32) -> Option<&'static str> {
        Some(match id {
            0x00 => "volume",
            0x01 => "pitch",
            0x02 => "lpf",
            0x03 => "hpf",
            0x04 => "bus_volume",
            0x05 => "make_up_gain",
            0x08..=0x0B => "user_aux_send_volume",
            0x0C => "game_aux_send_volume",
            0x0D => "output_bus_volume",
            0x0E => "output_bus_hpf",
            0x0F => "output_bus_lpf",
            0x18 => "game_aux_send_lpf",
            0x19 => "game_aux_send_hpf",
            0x22 => "initial_delay",
            0x38 => "attenuation_distance_scaling",
            _ => return None,
        })
    }
}
