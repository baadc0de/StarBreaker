//! The Ogg page walk behind media durations (synthetic pages; no game data).

use super::*;

/// One Ogg page: capture pattern, version, header type, granule, serial, sequence, CRC (not checked), segments.
fn page(granule: u64, body: &[u8]) -> Vec<u8> {
    let mut p = b"OggS".to_vec();
    p.push(0);
    p.push(0);
    p.extend_from_slice(&granule.to_le_bytes());
    p.extend_from_slice(&[0; 12]);
    let mut lacing = vec![255u8; body.len() / 255];
    lacing.push((body.len() % 255) as u8);
    p.push(lacing.len() as u8);
    p.extend(lacing);
    p.extend_from_slice(body);
    p
}

#[test]
fn the_last_granule_comes_from_the_last_page_not_from_packet_bytes() {
    let mut ogg = page(0, b"header");
    ogg.extend(page(48_000, &[1; 300]));
    // The last page's packet happens to contain the capture pattern followed by junk.
    let mut body = b"xxOggS\0\0\xff\xff\xff\xff\xff\xff\xff\x7f".to_vec();
    body.extend_from_slice(&[2; 40]);
    ogg.extend(page(96_000, &body));
    assert_eq!(last_granule(&ogg), Some(96_000));
}

#[test]
fn a_truncated_last_page_keeps_the_previous_granule() {
    let mut ogg = page(48_000, &[1; 10]);
    let mut cut = page(96_000, &[2; 100]);
    cut.truncate(40);
    ogg.extend(cut);
    assert_eq!(last_granule(&ogg), Some(48_000));
}
