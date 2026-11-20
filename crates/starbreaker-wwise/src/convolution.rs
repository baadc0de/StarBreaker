//! Wwise Convolution Reverb (plugin `0x007F0003`): its parameter block and its impulse-response media.
//!
//! The impulse response is a media file (a bank DIDX entry, `RIFF`-like chunks `PLUG` → `hash`, `junk`, `data`) holding
//! the IR already partitioned for uniformly-partitioned FFT convolution. Layout of the `data` payload, as read from the
//! game's `AUXB_VERB_SYS_INT_*` banks (LIVE 4.10.2; every payload of the four banks ends exactly where this layout
//! says):
//!
//! | offset | type | field |
//! |---|---|---|
//! | 0 | `u16` | partition length in samples (1024) |
//! | 2 | `u16` | channels (2) |
//! | 4 | `u32` | FFT length (2048 = 2 × partition) |
//! | 8 | `u32` | sample rate (48000) |
//! | 12 | `u32` | unknown (12546 in every bank) |
//! | 16 | `u32` | partitions of the IR (including trailing silent ones that are not stored) |
//! | 20 | `f32` | unknown, grows with the room (0.386 … 4.81) |
//! | 24 | `f32` | peak level in dBFS of the time-domain IR (matches the decoded peak within 0.6 dB) |
//! | 28 | `u32` | stored partitions `k` (the list below) |
//! | 32 | `u32` | total bins over the stored partitions (the list's sum) |
//! | 36 | `u32` | bins of a full partition (1024) |
//! | 40 | `u32` × 2 | zero in every bank |
//! | 48 | `u16` × k | bins kept per partition (its lowest-frequency ones) |
//! | aligned to 16 | | per partition, per channel: `bins` complex values, each `(re, im)` as **bfloat16** |
//!
//! Open: whether the two zero words at 40 are two leading empty partitions (a 2048-sample pre-delay, identical in every
//! room) or reserved fields; the data offset is the same either way. They are read as reserved here, because the
//! stored-partition count at 28 then equals the list length in all four banks.
//!
//! Partition `p`, channel `c` is the DFT of 1024 IR samples zero-padded to 2048: its inverse transform has the samples in
//! the first half and silence in the second (true to 1e-6 on the game's data with bfloat16 values; read as IEEE half
//! floats instead, the second half holds 15 % of the energy). Bins past the stored ones are zero, and bin 0 is DC with a
//! zero imaginary part. [`decode_impulse_response`] rebuilds the time-domain IR.

use serde::Serialize;

/// The Convolution Reverb parameter block (`CAkConvolutionReverbFXParams::SetParamsBlock`, wwiser; 57 bytes in v154).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ConvolutionParams {
    pub pre_delay_ms: f32,
    pub front_rear_delay_ms: f32,
    pub stereo_width_deg: f32,
    pub input_center_level_db: f32,
    pub input_lfe_level_db: f32,
    pub input_stereo_width_deg: f32,
    pub front_level_db: f32,
    pub rear_level_db: f32,
    pub center_level_db: f32,
    pub lfe_level_db: f32,
    pub dry_level_db: f32,
    pub wet_level_db: f32,
    pub algorithm: u32,
}

impl ConvolutionParams {
    /// Parse the plugin parameter block (at least the 52 bytes up to the algorithm type).
    pub fn parse(block: &[u8]) -> Option<Self> {
        let f = |i: usize| block.get(i * 4..i * 4 + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap()));
        let u = |i: usize| block.get(i * 4..i * 4 + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
        Some(Self {
            pre_delay_ms: f(0)?,
            front_rear_delay_ms: f(1)?,
            stereo_width_deg: f(2)?,
            input_center_level_db: f(3)?,
            input_lfe_level_db: f(4)?,
            input_stereo_width_deg: f(5)?,
            front_level_db: f(6)?,
            rear_level_db: f(7)?,
            center_level_db: f(8)?,
            lfe_level_db: f(9)?,
            dry_level_db: f(10)?,
            wet_level_db: f(11)?,
            algorithm: u(12)?,
        })
    }
}

/// A decoded impulse response: interleaved `f32` samples.
#[derive(Debug, Clone, PartialEq)]
pub struct ImpulseResponse {
    pub channels: u16,
    pub sample_rate: u32,
    /// Interleaved samples, `frames × channels`.
    pub samples: Vec<f32>,
    /// The header's peak level (dBFS).
    pub header_peak_db: f32,
    /// The header's unidentified second float (kept for the record).
    pub header_unknown_f32: f32,
}

impl ImpulseResponse {
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }

    /// Peak absolute sample in dBFS.
    pub fn peak_db(&self) -> f32 {
        let peak = self.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        20.0 * peak.max(1e-12).log10()
    }

    /// Reverberation time (s) from the Schroeder energy decay curve: the time the summed-channel decay takes from
    /// −5 dB to −5 − `range_db` dB, extrapolated to 60 dB (T20 with `range_db` 20, T30 with 30). `None` when the decay
    /// never reaches the lower level.
    pub fn reverb_time(&self, range_db: f32) -> Option<f32> {
        let ch = usize::from(self.channels.max(1));
        let energy: Vec<f64> = self
            .samples
            .chunks(ch)
            .map(|f| f.iter().map(|&s| f64::from(s) * f64::from(s)).sum())
            .collect();
        let total: f64 = energy.iter().sum();
        if total <= 0.0 {
            return None;
        }
        let at = |db: f32| -> Option<usize> {
            let threshold = total * 10f64.powf(f64::from(db) / 10.0);
            let mut remaining = total;
            for (i, e) in energy.iter().enumerate() {
                if remaining <= threshold {
                    return Some(i);
                }
                remaining -= e;
            }
            None
        };
        let (start, end) = (at(-5.0)?, at(-5.0 - range_db)?);
        Some((end - start) as f32 / self.sample_rate as f32 * 60.0 / range_db)
    }

    /// Time (s) of the first sample above −60 dB of the peak: the IR's pre-delay.
    pub fn onset_s(&self) -> f32 {
        let peak = self.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let ch = usize::from(self.channels.max(1));
        let first = self.samples.iter().position(|s| s.abs() > peak * 1e-3).unwrap_or(0);
        (first / ch) as f32 / self.sample_rate as f32
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum IrError {
    #[error("impulse response: {0}")]
    Format(String),
}

fn chunk<'a>(data: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    // `PLUG` <size> then sub-chunks (`hash`, `junk`, `data`), each `tag u32-size payload`.
    let body = if data.get(0..4) == Some(b"PLUG") { data.get(8..)? } else { data };
    let mut at = 0usize;
    while at + 8 <= body.len() {
        let size = u32::from_le_bytes(body[at + 4..at + 8].try_into().ok()?) as usize;
        let payload = body.get(at + 8..at + 8 + size)?;
        if &body[at..at + 4] == tag {
            return Some(payload);
        }
        at += 8 + size;
    }
    None
}

fn bf16(bits: u16) -> f32 {
    f32::from_bits(u32::from(bits) << 16)
}

/// Rebuild the time-domain impulse response of a Convolution Reverb IR media file (see the module docs).
pub fn decode_impulse_response(media: &[u8]) -> Result<ImpulseResponse, IrError> {
    let err = |m: &str| IrError::Format(m.to_owned());
    let p = chunk(media, b"data").ok_or_else(|| err("no PLUG data chunk"))?;
    let u16_at = |o: usize| p.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |o: usize| p.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let f32_at = |o: usize| u32_at(o).map(f32::from_bits);
    let partition = usize::from(u16_at(0).ok_or_else(|| err("short header"))?);
    let channels = u16_at(2).ok_or_else(|| err("short header"))?;
    let fft = u32_at(4).ok_or_else(|| err("short header"))? as usize;
    let sample_rate = u32_at(8).ok_or_else(|| err("short header"))?;
    let partitions = u32_at(16).ok_or_else(|| err("short header"))? as usize;
    let header_unknown_f32 = f32_at(20).ok_or_else(|| err("short header"))?;
    let header_peak_db = f32_at(24).ok_or_else(|| err("short header"))?;
    let stored = u32_at(28).ok_or_else(|| err("short header"))? as usize;
    let total_bins = u32_at(32).ok_or_else(|| err("short header"))? as usize;
    if channels == 0 || partition == 0 || fft != partition * 2 || !fft.is_power_of_two() {
        return Err(err("unsupported partition / FFT size"));
    }
    const LIST: usize = 48;
    let bins = (0..stored)
        .map(|i| u16_at(LIST + 2 * i).map(usize::from).ok_or_else(|| err("truncated bin list")))
        .collect::<Result<Vec<_>, _>>()?;
    if bins.iter().any(|&b| b > partition) {
        return Err(err("a partition holds more bins than the FFT has"));
    }
    if bins.iter().sum::<usize>() != total_bins || bins.len() > partitions {
        return Err(err("bin list does not add up to the header's total"));
    }
    let data_start = (LIST + 2 * bins.len()).div_ceil(16) * 16;
    let ch = usize::from(channels);
    let need = data_start + total_bins * ch * 4;
    if p.len() != need {
        return Err(err(&format!("payload is {} bytes, layout needs {need}", p.len())));
    }
    // The declared length (trailing silent partitions are not stored) must be addressable before allocating: checked
    // arithmetic, a frame count that fits the u32 the header's fields use, and a non-zero rate.
    let len = partitions
        .checked_mul(partition)
        .filter(|frames| u32::try_from(*frames).is_ok() && sample_rate > 0)
        .and_then(|frames| frames.checked_mul(ch))
        .ok_or_else(|| err("declared IR length out of range"))?;
    let mut samples = vec![0.0f32; len];
    let mut at = data_start;
    let mut spectrum = vec![(0.0f32, 0.0f32); fft];
    for (k, &nb) in bins.iter().enumerate() {
        for c in 0..ch {
            if nb == 0 {
                continue;
            }
            spectrum.iter_mut().for_each(|v| *v = (0.0, 0.0));
            for bin in 0..nb {
                let re = bf16(u16::from_le_bytes([p[at], p[at + 1]]));
                let im = bf16(u16::from_le_bytes([p[at + 2], p[at + 3]]));
                at += 4;
                spectrum[bin] = (re, im);
                if bin > 0 {
                    spectrum[fft - bin] = (re, -im);
                }
            }
            inverse_fft(&mut spectrum);
            for s in 0..partition {
                samples[(k * partition + s) * ch + c] = spectrum[s].0;
            }
        }
    }
    Ok(ImpulseResponse { channels, sample_rate, samples, header_peak_db, header_unknown_f32 })
}

/// In-place inverse DFT (radix-2, scaled by 1/n) of `(re, im)` pairs; `data.len()` must be a power of two.
pub(crate) fn inverse_fft(data: &mut [(f32, f32)]) {
    transform(data, true);
}

fn transform(data: &mut [(f32, f32)], inverse: bool) {
    let n = data.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            data.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = if inverse { 2.0 } else { -2.0 } * std::f64::consts::PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (angle * k as f64).sin_cos();
                let (wr, wi) = (c as f32, s as f32);
                let (ur, ui) = data[start + k];
                let (xr, xi) = data[start + k + len / 2];
                let (vr, vi) = (xr * wr - xi * wi, xr * wi + xi * wr);
                data[start + k] = (ur + vr, ui + vi);
                data[start + k + len / 2] = (ur - vr, ui - vi);
            }
        }
        len <<= 1;
    }
    if inverse {
        let scale = 1.0 / n as f32;
        data.iter_mut().for_each(|v| *v = (v.0 * scale, v.1 * scale));
    }
}

#[cfg(test)]
pub(crate) fn forward_fft(data: &mut [(f32, f32)]) {
    transform(data, false);
}

#[cfg(test)]
#[path = "convolution_tests.rs"]
mod tests;
