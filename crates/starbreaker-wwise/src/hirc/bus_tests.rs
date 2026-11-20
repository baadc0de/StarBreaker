//! v154 bus layout on synthetic blobs: an aux bus with one effect slot, a state group and no RTPC.

use super::*;

fn aux_bus_blob(parent: u32, volume: Option<f32>) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&parent.to_le_bytes());
    if parent == 0 {
        d.extend_from_slice(&0xABCDu32.to_le_bytes()); // device share set
    }
    match volume {
        Some(v) => {
            d.extend_from_slice(&[1, 0x04]); // one prop: BusVolume
            d.extend_from_slice(&v.to_le_bytes());
        }
        None => d.push(0),
    }
    d.push(1); // positioning: override parent, no 3D
    d.push(0x15); // aux bits: no aux ids
    d.extend_from_slice(&0u32.to_le_bytes()); // reflections aux bus
    d.push(0); // behaviour bits
    d.extend_from_slice(&0u16.to_le_bytes()); // max instances
    d.extend_from_slice(&0x0204u32.to_le_bytes()); // channel config
    d.push(0); // hdr bits
    d.extend_from_slice(&0i32.to_le_bytes()); // recovery time
    d.extend_from_slice(&(-96.3f32).to_le_bytes()); // max duck volume
    d.extend_from_slice(&0u32.to_le_bytes()); // ducks
    d.push(1); // one effect slot
    d.push(0); // bypass all
    d.push(0); // slot index
    d.extend_from_slice(&0x1234_5678u32.to_le_bytes()); // share set id
    d.push(2); // bit vector: share set
    d.push(0); // metadata slots
    d.extend_from_slice(&0u16.to_le_bytes()); // initial RTPC
    // state chunk: 1 state prop (Volume, accum 2, in dB), 1 group with one state at -6 dB on prop 4
    d.extend_from_slice(&[1, 0, 2, 1, 1]);
    d.extend_from_slice(&0xCA42_00FDu32.to_le_bytes());
    d.push(0); // sync type
    d.push(1); // one state
    d.extend_from_slice(&0x75B4_4BA2u32.to_le_bytes());
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&4u16.to_le_bytes());
    d.extend_from_slice(&(-6.0f32).to_le_bytes());
    d
}

#[test]
fn v154_aux_bus_is_consumed_exactly_with_its_effect_and_state() {
    for (parent, volume) in [(0xAE18_EF1Bu32, None), (0, Some(-3.5f32))] {
        let data = aux_bus_blob(parent, volume);
        let mut reader = SpanReader::new(&data);
        let bus = AudioBus::parse(9, &mut reader).expect("parses");
        assert_eq!(reader.remaining(), 0, "consumed exactly");
        assert_eq!(bus.parent(), (parent != 0).then_some(parent));
        assert_eq!(bus.channel_config & 0xFF, 4);
        assert_eq!(bus.fx_params.fx_list.len(), 1);
        assert_eq!(bus.fx_params.fx_list[0].fx_id, 0x1234_5678);
        assert_eq!(bus.state_chunk.state_groups[0].states[0].properties.props[0], (4, -6.0));
        assert_eq!(bus.prop(super::super::props::prop_id::BUS_VOLUME), volume);
    }
}
