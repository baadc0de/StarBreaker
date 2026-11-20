//! Convolution Reverb IR decoding on a synthetic partitioned IR (built the way the module docs describe), and the
//! parameter block.

use super::*;

const PART: usize = 1024;

fn to_bf16(v: f32) -> u16 {
    // Round to nearest (the stored values are bfloat16; the test only needs a faithful quantiser).
    let bits = v.to_bits();
    ((bits + 0x7FFF + ((bits >> 16) & 1)) >> 16) as u16
}

/// A two-channel IR: silence for two partitions, then a decaying, deterministic pseudo-noise tail.
fn synthetic_ir(partitions: usize) -> Vec<[f32; 2]> {
    let mut seed = 0x1234_5678u32;
    (0..partitions * PART)
        .map(|i| {
            if i < 2 * PART {
                return [0.0, 0.0];
            }
            let mut next = || {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
            };
            let decay = (-((i - 2 * PART) as f32) / 4000.0).exp() * 0.25;
            [next() * decay, next() * decay]
        })
        .collect()
}

/// Encode `ir` as a PLUG media blob, keeping `keep(p)` bins of partition `p`.
fn encode(ir: &[[f32; 2]], keep: impl Fn(usize) -> usize) -> Vec<u8> {
    let partitions = ir.len() / PART;
    let bins: Vec<usize> = (0..partitions).map(|p| if ir[p * PART..(p + 1) * PART].iter().all(|s| s == &[0.0, 0.0]) { 0 } else { keep(p) }).collect();
    let total: usize = bins.iter().sum();
    let mut data = Vec::new();
    data.extend_from_slice(&(PART as u16).to_le_bytes());
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(&(2 * PART as u32).to_le_bytes());
    data.extend_from_slice(&48_000u32.to_le_bytes());
    data.extend_from_slice(&7u32.to_le_bytes());
    data.extend_from_slice(&(partitions as u32 + 3).to_le_bytes()); // trailing silent partitions are not stored
    data.extend_from_slice(&1.5f32.to_le_bytes());
    data.extend_from_slice(&(-20.0f32).to_le_bytes());
    data.extend_from_slice(&(bins.len() as u32).to_le_bytes());
    data.extend_from_slice(&(total as u32).to_le_bytes());
    data.extend_from_slice(&(PART as u32).to_le_bytes());
    data.extend_from_slice(&[0u8; 8]);
    for &b in &bins {
        data.extend_from_slice(&(b as u16).to_le_bytes());
    }
    while data.len() % 16 != 0 {
        data.push(0);
    }
    for (p, &nb) in bins.iter().enumerate() {
        for c in 0..2 {
            let mut spec: Vec<(f32, f32)> = (0..2 * PART)
                .map(|i| (if i < PART { ir[p * PART + i][c] } else { 0.0 }, 0.0))
                .collect();
            forward_fft(&mut spec);
            for &(re, im) in spec.iter().take(nb) {
                data.extend_from_slice(&to_bf16(re).to_le_bytes());
                data.extend_from_slice(&to_bf16(im).to_le_bytes());
            }
        }
    }
    let mut chunks = Vec::new();
    for (tag, payload) in [(b"hash", vec![0u8; 16]), (b"junk", Vec::new()), (b"data", data)] {
        chunks.extend_from_slice(tag);
        chunks.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        chunks.extend_from_slice(&payload);
    }
    let mut blob = b"PLUG".to_vec();
    blob.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
    blob.extend_from_slice(&chunks);
    blob
}

#[test]
fn full_partitions_round_trip_to_the_time_domain_ir() {
    let ir = synthetic_ir(40);
    let decoded = decode_impulse_response(&encode(&ir, |_| PART)).expect("decodes");
    assert_eq!((decoded.channels, decoded.sample_rate), (2, 48_000));
    assert_eq!(decoded.frames(), (40 + 3) * PART, "trailing silent partitions are kept as silence");
    // Amplitude decays with tau = 4000 samples: energy falls 60 dB in 6.91 tau = 0.576 s.
    let t20 = decoded.reverb_time(20.0).expect("decays 25 dB");
    assert!((0.54..0.62).contains(&t20), "T20 {t20}");
    let mut err = 0.0f64;
    let mut energy = 0.0f64;
    for (i, frame) in ir.iter().enumerate() {
        for c in 0..2 {
            let d = f64::from(decoded.samples[i * 2 + c] - frame[c]);
            err += d * d;
            energy += f64::from(frame[c]) * f64::from(frame[c]);
        }
    }
    // bfloat16 keeps 8 mantissa bits: the residual is far below the signal (here < -35 dB).
    assert!(10.0 * (err / energy).log10() < -35.0, "residual {:.1} dB", 10.0 * (err / energy).log10());
    assert!((decoded.onset_s() - 2.0 * PART as f32 / 48_000.0).abs() < 1e-3, "two empty partitions are 2048 samples of silence");
    assert_eq!(decoded.header_peak_db, -20.0);
}

#[test]
fn truncated_partitions_keep_their_low_bins_and_the_layout_is_checked() {
    let ir = synthetic_ir(5);
    let blob = encode(&ir, |p| PART >> (p.saturating_sub(2)));
    let decoded = decode_impulse_response(&blob).expect("decodes");
    assert_eq!(decoded.frames(), 8 * PART);
    // Partition 4 keeps a quarter of its bins: its tail is low-passed, not lost.
    let tail: f32 = decoded.samples[4 * PART * 2..5 * PART * 2].iter().map(|s| s * s).sum();
    assert!(tail > 0.0);
    let mut short = blob.clone();
    short.truncate(blob.len() - 4);
    assert!(decode_impulse_response(&short).is_err(), "a truncated payload is rejected");
}

#[test]
fn convolution_parameter_block() {
    let mut block = Vec::new();
    for v in [0.0f32, 0.0, 180.0, 0.0, -96.3, 180.0, 0.0, 0.0, 0.0, -96.3, -96.0, 0.0] {
        block.extend_from_slice(&v.to_le_bytes());
    }
    block.extend_from_slice(&0u32.to_le_bytes());
    block.extend_from_slice(&(-80.0f32).to_le_bytes());
    block.push(2);
    let p = ConvolutionParams::parse(&block).expect("57 bytes");
    assert_eq!((p.dry_level_db, p.wet_level_db, p.stereo_width_deg), (-96.0, 0.0, 180.0));
    assert!(ConvolutionParams::parse(&block[..40]).is_none());
}

#[test]
fn fft_round_trip() {
    let orig: Vec<(f32, f32)> = (0..16).map(|i| ((i as f32 * 0.37).sin(), 0.0)).collect();
    let mut d = orig.clone();
    forward_fft(&mut d);
    inverse_fft(&mut d);
    for (a, b) in orig.iter().zip(&d) {
        assert!((a.0 - b.0).abs() < 1e-5 && b.1.abs() < 1e-5, "{a:?} {b:?}");
    }
    let to = to_bf16(0.3);
    assert!((bf16(to) - 0.3).abs() < 0.002, "{}", bf16(to));
}
