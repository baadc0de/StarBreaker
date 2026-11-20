//! Attenuation (type 14) on synthetic v154 blobs: the leading height-spread flag, the 19 curve slots,
//! curves of (x, y, interpolation) points and the trailing initial RTPC list.

use super::*;

/// A v154 attenuation blob: height spread on, cone `cone`, slot 0 (dry volume) and 5 (spread) → curve 0 / 1.
fn blob(cone: bool) -> Vec<u8> {
    let mut d = vec![1u8, u8::from(cone)];
    if cone {
        for v in [90.0f32, 270.0, -6.0, 10.0, 0.0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut slots = [-1i8; 19];
    slots[0] = 0;
    slots[5] = 1;
    d.extend(slots.iter().map(|&s| s as u8));
    d.push(2); // two curves
    // curve 0: dB scaling, 3 points
    d.push(2);
    d.extend_from_slice(&3u16.to_le_bytes());
    for (x, y, i) in [(0.0f32, 0.0f32, 9u32), (1.0, 0.0, 6), (10.0, -200.0, 4)] {
        d.extend_from_slice(&x.to_le_bytes());
        d.extend_from_slice(&y.to_le_bytes());
        d.extend_from_slice(&i.to_le_bytes());
    }
    // curve 1: no scaling, 2 points
    d.push(0);
    d.extend_from_slice(&2u16.to_le_bytes());
    for (x, y, i) in [(0.0f32, 100.0f32, 4u32), (10.0, 0.0, 4)] {
        d.extend_from_slice(&x.to_le_bytes());
        d.extend_from_slice(&y.to_le_bytes());
        d.extend_from_slice(&i.to_le_bytes());
    }
    d.extend_from_slice(&0u16.to_le_bytes()); // InitialRtpc: none
    d
}

#[test]
fn v154_layout_reads_height_spread_flag_then_cone_and_curves_to_the_end() {
    for cone in [false, true] {
        let data = blob(cone);
        let mut reader = SpanReader::new(&data);
        let att = Attenuation::parse(7, &mut reader).expect("parses");
        assert_eq!(reader.remaining(), 0, "the blob is consumed exactly (cone {cone})");
        assert!(att.height_spread);
        assert_eq!(att.cone.is_some(), cone);
        assert_eq!(att.curve_to_use[0], 0);
        assert_eq!(att.curve_to_use[5], 1);
        assert_eq!(att.curves.len(), 2);
        assert_eq!(att.curves[0].points[2].from, 10.0);
        assert_eq!(att.max_distance(), Some(10.0));
        assert_eq!(att.curve(AttenuationCurveSlot::VolumeDry).unwrap().scaling, 2);
        assert!(att.curve(AttenuationCurveSlot::LowPassFilter).is_none());
    }
}
