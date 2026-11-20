//! Wwise Vorbis WEM → standard Ogg Vorbis (ww2ogg), with the codebook library chosen from the header.
//!
//! A Wwise 2012+ media (its `vorb` data inside a 0x42-byte `fmt ` chunk) references the aoTuV 6.03 codebook library,
//! as vgmstream treats `.wem` of that layout: it converts with aoTuV, and only a conversion error (the setup header
//! does not rebuild) falls back to the standard library. Older layouts may use either library, so both are tried and
//! the output validated (`ww2ogg::validate`: decode errors, and a clipping heuristic for a wrong codebook that still
//! decodes). The clipping heuristic is not applied to the header-determined case: a loud media that peaks above full
//! scale is valid there and was rejected by it.
use std::io::Cursor;

use ww2ogg::{CodebookLibrary, WwiseRiffVorbis};

use crate::riff::RiffFile;
use crate::WemError;

type Library = fn() -> ww2ogg::WemResult<CodebookLibrary>;

/// Size of a `fmt ` chunk that carries the `vorb` data itself (Wwise 2012+).
const FMT_WITH_VORB: usize = 0x42;

/// Whether the WEM's layout determines its codebook library (aoTuV 6.03).
pub(crate) fn uses_aotuv_codebooks(wem_bytes: &[u8]) -> bool {
    RiffFile::parse(wem_bytes).ok().is_some_and(|riff| {
        riff.find_chunk(b"vorb").is_none() && riff.find_chunk(b"fmt ").is_some_and(|c| c.data_size == FMT_WITH_VORB)
    })
}

/// Convert a Wwise Vorbis WEM (raw bytes) to standard Ogg Vorbis.
pub fn wem_to_ogg(wem_bytes: &[u8]) -> Result<Vec<u8>, WemError> {
    if uses_aotuv_codebooks(wem_bytes) {
        match try_with_codebooks(wem_bytes, CodebookLibrary::aotuv_codebooks) {
            Ok(ogg) => return Ok(ogg),
            Err(aotuv) => {
                // Not seen in game data: a 2012+ layout over the standard library.
                let ogg = try_with_codebooks(wem_bytes, CodebookLibrary::default_codebooks)
                    .map_err(|e| WemError::Decode(format!("aoTuV: {aotuv}; standard codebooks: {e}")))?;
                ww2ogg::validate(&ogg).map_err(|e| WemError::Decode(format!("aoTuV: {aotuv}; standard codebooks: {e}")))?;
                return Ok(ogg);
            }
        }
    }
    // Older layouts: the standard library first, validated (a wrong codebook can decode into garbled audio).
    if let Ok(ogg) = try_with_codebooks(wem_bytes, CodebookLibrary::default_codebooks) {
        if ww2ogg::validate(&ogg).is_ok() {
            return Ok(ogg);
        }
    }
    let ogg = try_with_codebooks(wem_bytes, CodebookLibrary::aotuv_codebooks)?;
    if let Err(e) = ww2ogg::validate(&ogg) {
        return Err(WemError::Decode(format!("validation failed with both codebooks: {e}")));
    }
    Ok(ogg)
}

fn try_with_codebooks(wem_bytes: &[u8], codebook_fn: Library) -> Result<Vec<u8>, WemError> {
    let codebooks = codebook_fn().map_err(|e| WemError::Decode(e.to_string()))?;
    let input = Cursor::new(wem_bytes);
    let mut converter = WwiseRiffVorbis::new(input, codebooks).map_err(|e| WemError::Decode(e.to_string()))?;

    let mut ogg_data = Vec::new();
    converter.generate_ogg(&mut ogg_data).map_err(|e| WemError::Decode(e.to_string()))?;

    Ok(ogg_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A RIFF WAVE with a Vorbis `fmt ` chunk of `fmt_size` bytes and, optionally, a separate `vorb` chunk.
    fn wem(fmt_size: u32, vorb: bool) -> Vec<u8> {
        let mut b = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        b.extend_from_slice(&fmt_size.to_le_bytes());
        b.extend_from_slice(&0xFFFFu16.to_le_bytes());
        b.extend(std::iter::repeat_n(0u8, fmt_size as usize - 2));
        if vorb {
            b.extend_from_slice(b"vorb");
            b.extend_from_slice(&0x2Au32.to_le_bytes());
            b.extend(std::iter::repeat_n(0u8, 0x2A));
        }
        b.extend_from_slice(b"data\x04\0\0\0\0\0\0\0");
        let size = (b.len() - 8) as u32;
        b[4..8].copy_from_slice(&size.to_le_bytes());
        b
    }

    #[test]
    fn the_2012_layout_determines_the_aotuv_library() {
        assert!(uses_aotuv_codebooks(&wem(0x42, false)));
        assert!(!uses_aotuv_codebooks(&wem(0x28, true)), "a separate vorb chunk: an older layout, tried both ways");
        assert!(!uses_aotuv_codebooks(&wem(0x18, true)));
        assert!(!uses_aotuv_codebooks(b"not a riff file"));
    }
}
