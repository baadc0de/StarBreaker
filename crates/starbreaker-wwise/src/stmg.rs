//! `STMG` (global settings, `Init.bnk`), bank version 154: state groups, switch groups and the **game parameters'
//! default values** (what an RTPC is until the game sets it).
//!
//! Layout (wwiser `CAkBankMgr::ProcessGlobalSettingsChunk`, v141–154): `u16` filter behaviour, `f32` volume
//! threshold, `u16` max voices, `u16` max virtual voices; `u32` state groups × (`u32` id, `u32` default transition time,
//! `u32` transitions × (`u32` from, `u32` to, `u32` time)); `u32` switch groups × (`u32` id, `u32` RTPC id, `u8` RTPC
//! type, `u32` points × (`f32`, `f32`, `u32`)); `u32` game parameters × (`u32` id, `f32` default, `u32` ramp type,
//! `f32` ramp up, `f32` ramp down, `u8` built-in binding); acoustic textures follow (not read).

use std::collections::HashMap;

use starbreaker_common::{ParseError, SpanReader};

/// Game parameter id → default value.
pub fn game_parameter_defaults(section: &[u8]) -> Result<HashMap<u32, f32>, ParseError> {
    let mut r = SpanReader::new(section);
    r.read_u16()?;
    r.read_f32()?;
    r.read_u16()?;
    r.read_u16()?;
    for _ in 0..r.read_u32()? {
        r.read_u32()?;
        r.read_u32()?;
        let transitions = r.read_u32()? as usize;
        r.advance(transitions * 12)?;
    }
    for _ in 0..r.read_u32()? {
        r.read_u32()?;
        r.read_u32()?;
        r.read_u8()?;
        let points = r.read_u32()? as usize;
        r.advance(points * 12)?;
    }
    let mut out = HashMap::new();
    for _ in 0..r.read_u32()? {
        let id = r.read_u32()?;
        let value = r.read_f32()?;
        r.read_u32()?;
        r.read_f32()?;
        r.read_f32()?;
        r.read_u8()?;
        out.insert(id, value);
    }
    Ok(out)
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
        d.extend_from_slice(&[7, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0]);
        d.extend_from_slice(&[0u8; 12]);
        d.extend_from_slice(&1u32.to_le_bytes()); // one switch group with two points
        d.extend_from_slice(&[9, 0, 0, 0, 3, 0, 0, 0, 0, 2, 0, 0, 0]);
        d.extend_from_slice(&[0u8; 24]);
        d.extend_from_slice(&2u32.to_le_bytes());
        for (id, v) in [(11u32, 1.0f32), (12, 0.25)] {
            d.extend_from_slice(&id.to_le_bytes());
            d.extend_from_slice(&v.to_le_bytes());
            d.extend_from_slice(&[0u8; 13]);
        }
        let defaults = game_parameter_defaults(&d).unwrap();
        assert_eq!(defaults.get(&11), Some(&1.0));
        assert_eq!(defaults.get(&12), Some(&0.25));
    }
}
