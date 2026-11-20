//! Version 154 Wwise project settings in Init.bnk: STMG game parameters and transitions, and ENVS project filters.
//!
//! Switch-graph and acoustic-texture data follow the bank layout but the audio manifest only needs the game
//! parameters, state transition times and project environmental curves.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use starbreaker_common::{ParseError, SpanReader};

#[derive(Debug, Clone, Serialize)]
pub struct GameParameter {
    pub default: f32,
    /// Wwise: 0 immediate, 1 slew rate (units/s), 2 filtering time (s).
    pub ramp_type: u32,
    pub ramp_up: f32,
    pub ramp_down: f32,
    /// Raw AkBuiltInParam ID.
    pub builtin: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateTransition {
    pub from: u32,
    pub to: u32,
    pub duration_ms: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateGroup {
    pub default_transition_ms: u32,
    pub transitions: Vec<StateTransition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSettings {
    /// Wwise's accumulation mode for node/bus low- and high-pass filter values.
    pub filter_behaviour: u16,
    pub game_parameters: BTreeMap<u32, GameParameter>,
    pub state_groups: BTreeMap<u32, StateGroup>,
}

pub fn parse(section: &[u8]) -> Result<GlobalSettings, ParseError> {
    let mut r = SpanReader::new(section);
    let filter_behaviour = r.read_u16()?;
    r.read_f32()?; // volume threshold
    r.read_u16()?; // max voices
    r.read_u16()?; // max virtual voices
    let mut state_groups = BTreeMap::new();
    for _ in 0..r.read_u32()? {
        let id = r.read_u32()?;
        let default_transition_ms = r.read_u32()?;
        let mut transitions = Vec::new();
        for _ in 0..r.read_u32()? {
            transitions.push(StateTransition {
                from: r.read_u32()?,
                to: r.read_u32()?,
                duration_ms: r.read_u32()?,
            });
        }
        state_groups.insert(id, StateGroup { default_transition_ms, transitions });
    }
    for _ in 0..r.read_u32()? {
        r.read_u32()?; // switch group id
        r.read_u32()?; // controlling game parameter
        r.read_u8()?; // controller type
        let points = r.read_u32()? as usize;
        r.advance(points.saturating_mul(12))?;
    }
    let mut game_parameters = BTreeMap::new();
    for _ in 0..r.read_u32()? {
        game_parameters.insert(r.read_u32()?, GameParameter {
            default: r.read_f32()?,
            ramp_type: r.read_u32()?,
            ramp_up: r.read_f32()?,
            ramp_down: r.read_f32()?,
            builtin: r.read_u8()?,
        });
    }
    Ok(GlobalSettings { filter_behaviour, game_parameters, state_groups })
}

impl GlobalSettings {
    pub fn defaults(&self) -> HashMap<u32, f32> {
        self.game_parameters.iter().map(|(&id, value)| (id, value.default)).collect()
    }
}

/// Game parameter id to default value (legacy consumer API).
pub fn game_parameter_defaults(section: &[u8]) -> Result<HashMap<u32, f32>, ParseError> {
    Ok(parse(section)?.defaults())
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentalCurve {
    pub operation: &'static str,
    pub property: &'static str,
    pub enabled: bool,
    pub scaling: u8,
    /// (input, output, Wwise interpolation id).
    pub points: Vec<(f32, f32, u32)>,
}

/// ENVS contains four operations, each with a volume, LPF and HPF curve in that order.
pub fn parse_environment_curves(section: &[u8]) -> Result<Vec<EnvironmentalCurve>, ParseError> {
    let mut r = SpanReader::new(section);
    let mut curves = Vec::new();
    for operation in ["obstruction", "occlusion", "diffraction", "transmission"] {
        for property in ["volume", "lpf", "hpf"] {
            let enabled = r.read_bool()?;
            let scaling = r.read_u8()?;
            let mut points = Vec::new();
            for _ in 0..r.read_u16()? {
                points.push((r.read_f32()?, r.read_f32()?, r.read_u32()?));
            }
            curves.push(EnvironmentalCurve { operation, property, enabled, scaling, points });
        }
    }
    if !r.is_empty() {
        return Err(ParseError::InvalidLayout(format!("{} trailing bytes in ENVS", r.remaining())));
    }
    Ok(curves)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_parameter_defaults_after_state_and_switch_groups() {
        let mut d = Vec::new();
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&(-80.0f32).to_le_bytes());
        d.extend_from_slice(&256u16.to_le_bytes());
        d.extend_from_slice(&256u16.to_le_bytes());
        d.extend_from_slice(&1u32.to_le_bytes()); // one state group with one transition
        d.extend_from_slice(&7u32.to_le_bytes());
        d.extend_from_slice(&500u32.to_le_bytes());
        d.extend_from_slice(&1u32.to_le_bytes());
        for n in [123u32, 456, 750] {
            d.extend_from_slice(&n.to_le_bytes());
        }
        d.extend_from_slice(&1u32.to_le_bytes()); // one switch group with two points
        d.extend_from_slice(&[9, 0, 0, 0, 3, 0, 0, 0, 0, 2, 0, 0, 0]);
        d.extend_from_slice(&[0u8; 24]);
        d.extend_from_slice(&2u32.to_le_bytes());
        for (id, v) in [(11u32, 1.0f32), (12, 0.25)] {
            d.extend_from_slice(&id.to_le_bytes());
            d.extend_from_slice(&v.to_le_bytes());
            d.extend_from_slice(&2u32.to_le_bytes());
            d.extend_from_slice(&0.25f32.to_le_bytes());
            d.extend_from_slice(&0.75f32.to_le_bytes());
            d.push(9);
        }
        let settings = parse(&d).unwrap();
        assert_eq!(settings.state_groups[&7].default_transition_ms, 500);
        assert_eq!(settings.state_groups[&7].transitions[0].duration_ms, 750);
        assert_eq!(settings.game_parameters[&11].ramp_type, 2);
        assert_eq!(settings.game_parameters[&11].ramp_up, 0.25);
        assert_eq!(settings.game_parameters[&11].ramp_down, 0.75);
        assert_eq!(settings.game_parameters[&11].builtin, 9);
        assert!(parse(&d[..d.len() - 1]).is_err());

        let defaults = game_parameter_defaults(&d).unwrap();
        assert_eq!(defaults.get(&11), Some(&1.0));
        assert_eq!(defaults.get(&12), Some(&0.25));
    }

    #[test]
    fn reads_disabled_environment_curves_without_dropping_their_points() {
        // Synthetic project data with just one enabled curve.
        let mut data = Vec::new();
        for index in 0..12 {
            data.push(u8::from(index == 4));
            data.push((index % 3) as u8);
            data.extend_from_slice(&1u16.to_le_bytes());
            data.extend_from_slice(&2.5f32.to_le_bytes());
            data.extend_from_slice(&3.5f32.to_le_bytes());
            data.extend_from_slice(&4u32.to_le_bytes());
        }
        let curves = parse_environment_curves(&data).unwrap();
        assert_eq!(curves.len(), 12);
        assert_eq!(curves[4].operation, "occlusion");
        assert_eq!(curves[4].property, "lpf");
        assert!(curves[4].enabled);
        assert!(!curves[11].enabled);
        assert_eq!(curves[11].points, [(2.5, 3.5, 4)]);
        assert!(parse_environment_curves(&data[..data.len() - 1]).is_err());
    }
}
