//! For every attenuation and bus object in a directory of banks: does the typed parser consume the entry exactly?
//! Usage: hirc_exact <directory>. Prints, per type, entries parsed exactly / with bytes left / failed.
use std::collections::BTreeMap;
use std::{env, fs, path::Path};

use starbreaker_common::SpanReader;
use starbreaker_wwise::BnkFile;
use starbreaker_wwise::hirc::{Attenuation, AudioBus, BlendContainer, RanSeqContainer, Sound};

fn files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bnk")) {
            out.push(path);
        }
    }
}

fn main() {
    let dir = env::args().nth(1).expect("usage: hirc_exact <directory>");
    let mut paths = Vec::new();
    files(Path::new(&dir), &mut paths);
    let mut stats: BTreeMap<u8, [u32; 3]> = BTreeMap::new();
    for path in paths {
        let Ok(data) = fs::read(&path) else { continue };
        let Ok(bnk) = BnkFile::parse(&data) else { continue };
        let Some(hirc) = bnk.hirc else { continue };
        for e in &hirc.entries {
            let mut r = SpanReader::new(&e.data);
            let ok = match e.type_id {
                2 => Sound::parse(e.object_id, &mut r).is_ok(),
                5 => RanSeqContainer::parse(e.object_id, &mut r).is_ok(),
                8 | 18 => AudioBus::parse(e.object_id, &mut r).is_ok(),
                9 => BlendContainer::parse(e.object_id, &mut r).is_ok(),
                14 => Attenuation::parse(e.object_id, &mut r).is_ok(),
                _ => continue,
            };
            let s = stats.entry(e.type_id).or_default();
            s[if !ok { 2 } else if r.remaining() == 0 { 0 } else { 1 }] += 1;
        }
    }
    for (t, [exact, left, failed]) in stats {
        println!("type {t:2}: exact {exact}, bytes left {left}, failed {failed}");
    }
}
