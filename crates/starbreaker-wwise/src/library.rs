//! The game's audio library for one archive: the ATL index, banks loaded on demand, and a global media index.
//!
//! [`AudioLibrary::open`] indexes the ATL documents and the archive's bank and loose-media entries. Banks are parsed
//! when first asked for ([`AudioLibrary::bank`]); `Init.bnk` holds the buses, aux buses, effects and global
//! attenuations. [`AudioLibrary::media`] finds a media file wherever it lives: `Media/<id>.wem` (language-specific
//! sounds under `Media/<language>/`), else the DIDX/DATA of the bank that references it, else **any** bank's DIDX (a
//! media of `SSAM_CRUS_StarLifter.bnk` can sit in `SSAM_CRUS_Spirit.bnk`; HIRC says `Embedded` either way). The
//! all-bank DIDX index is built on the first miss and kept.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use starbreaker_p4k::MappedP4k;

use crate::atl::AtlIndex;
use crate::bnk::BnkFile;
use crate::didx::DataIndexEntry;
use crate::hirc::{Hierarchy, HircObject};

const SOUNDS_DIR: &str = "data\\sounds\\wwise\\";
const MEDIA_DIR: &str = "data\\sounds\\wwise\\media\\";
/// The language whose localized media and banks are used.
pub const LANGUAGE: &str = "english(us)";

/// A parsed bank: its hierarchy and its embedded media.
pub struct LoadedBank {
    pub name: String,
    pub version: u32,
    pub hierarchy: Hierarchy,
    bytes: Vec<u8>,
    data: std::ops::Range<usize>,
    didx: Vec<DataIndexEntry>,
}

impl LoadedBank {
    /// A bank of an already-built hierarchy and embedded media `(id, bytes)` (tests, tools).
    pub fn from_parts(name: &str, hierarchy: Hierarchy, media: &[(u32, Vec<u8>)]) -> Self {
        let mut bytes = Vec::new();
        let mut didx = Vec::new();
        for (id, data) in media {
            didx.push(DataIndexEntry { id: *id, offset: bytes.len() as u32, size: data.len() as u32 });
            bytes.extend_from_slice(data);
        }
        let data = 0..bytes.len();
        Self { name: name.to_owned(), version: 154, hierarchy, bytes, data, didx }
    }

    /// Parse a bank file.
    pub fn parse(name: &str, bytes: Vec<u8>) -> Result<Self, crate::BnkError> {
        let (version, hierarchy, data, didx) = {
            let bnk = BnkFile::parse(&bytes)?;
            let hierarchy = bnk.hirc.as_ref().map(Hierarchy::from_section).unwrap_or_else(Hierarchy::empty);
            (bnk.header.version, hierarchy, bnk.data_section_range(), bnk.data_index.clone())
        };
        Ok(Self { name: name.to_owned(), version, hierarchy, bytes, data, didx })
    }

    /// An embedded media file's bytes.
    pub fn embedded(&self, id: u32) -> Option<&[u8]> {
        let e = self.didx.iter().find(|e| e.id == id)?;
        let start = self.data.start + e.offset as usize;
        self.bytes.get(start..start + e.size as usize).filter(|_| start + e.size as usize <= self.data.end)
    }
}

/// Where a media file was found.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaSource {
    /// `Data/Sounds/wwise/Media/[<language>/]<id>.wem`.
    Loose { path: String },
    /// The DIDX/DATA of a bank.
    Bank { bank: String },
}

/// The audio library of one archive.
pub struct AudioLibrary<'p> {
    /// `None` for an in-memory library ([`Self::in_memory`], tests).
    p4k: Option<&'p MappedP4k>,
    pub atl: AtlIndex,
    /// Lower-case bank file name → archive entry name (root banks before language ones).
    bank_entries: HashMap<String, String>,
    /// Loose media ids: root `Media/` and the language's `Media/<language>/`.
    loose: HashSet<u32>,
    loose_language: HashSet<u32>,
    banks: Mutex<HashMap<String, Option<Arc<LoadedBank>>>>,
    /// media id → bank file names holding it in their DIDX (built on the first cross-bank lookup).
    didx_index: OnceLock<HashMap<u32, Vec<String>>>,
}

impl<'p> AudioLibrary<'p> {
    /// Index the archive's audio: ATL documents, bank entries, loose media.
    pub fn open(p4k: &'p MappedP4k) -> Result<Self, crate::BnkError> {
        let (atl, _) = crate::game_audio::load(p4k)?;
        Ok(Self::with_atl(p4k, atl))
    }

    /// Like [`Self::open`] with an ATL index already built.
    pub fn with_atl(p4k: &'p MappedP4k, atl: AtlIndex) -> Self {
        let mut bank_entries = HashMap::new();
        let mut language_banks = HashMap::new();
        let (mut loose, mut loose_language) = (HashSet::new(), HashSet::new());
        for entry in p4k.entries() {
            let lower = entry.name.to_ascii_lowercase();
            let Some(rest) = lower.strip_prefix(SOUNDS_DIR) else { continue };
            if let Some(media) = rest.strip_prefix("media\\") {
                let (dir, file) = media.rsplit_once('\\').unwrap_or(("", media));
                if let Some(id) = file.strip_suffix(".wem").and_then(|s| s.parse::<u32>().ok()) {
                    match dir {
                        "" => {
                            loose.insert(id);
                        }
                        d if d == LANGUAGE => {
                            loose_language.insert(id);
                        }
                        _ => {}
                    }
                }
            } else if let Some(stem) = rest.strip_suffix(".bnk") {
                let (dir, file) = stem.rsplit_once('\\').unwrap_or(("", stem));
                let key = format!("{file}.bnk");
                match dir {
                    "" => {
                        bank_entries.insert(key, entry.name.clone());
                    }
                    d if d == LANGUAGE => {
                        language_banks.insert(key, entry.name.clone());
                    }
                    _ => {}
                }
            }
        }
        for (key, name) in language_banks {
            bank_entries.entry(key).or_insert(name);
        }
        Self {
            p4k: Some(p4k),
            atl,
            bank_entries,
            loose,
            loose_language,
            banks: Mutex::new(HashMap::new()),
            didx_index: OnceLock::new(),
        }
    }

    /// A library of banks already in memory (no archive): media only from their DIDX/DATA.
    pub fn in_memory(atl: AtlIndex, banks: Vec<LoadedBank>) -> Self {
        let mut map = HashMap::new();
        let mut index: HashMap<u32, Vec<String>> = HashMap::new();
        for bank in banks {
            let key = bank.name.to_ascii_lowercase();
            for e in &bank.didx {
                index.entry(e.id).or_default().push(key.clone());
            }
            map.insert(key, Some(Arc::new(bank)));
        }
        let didx_index = OnceLock::new();
        let _ = didx_index.set(index);
        Self {
            p4k: None,
            atl,
            bank_entries: HashMap::new(),
            loose: HashSet::new(),
            loose_language: HashSet::new(),
            banks: Mutex::new(map),
            didx_index,
        }
    }

    /// A bank by file name (`SSMP_CRUS_StarLifter.bnk`, case-insensitive), parsed on first use.
    pub fn bank(&self, name: &str) -> Option<Arc<LoadedBank>> {
        let key = name.to_ascii_lowercase();
        if let Some(cached) = self.banks.lock().unwrap().get(&key) {
            return cached.clone();
        }
        let loaded = self.bank_entries.get(&key).and_then(|entry| {
            let p4k = self.p4k?;
            let bytes = p4k.entry_case_insensitive(entry).and_then(|e| p4k.read(e).ok())?;
            match LoadedBank::parse(name, bytes) {
                Ok(bank) => Some(Arc::new(bank)),
                Err(e) => {
                    log::warn!("[audio] bank {name}: {e}");
                    None
                }
            }
        });
        self.banks.lock().unwrap().insert(key, loaded.clone());
        loaded
    }

    /// `Init.bnk`: buses, aux buses, effect share sets, global attenuations.
    pub fn init_bank(&self) -> Option<Arc<LoadedBank>> {
        self.bank("Init.bnk")
    }

    /// A HIRC object from the first of `banks` that holds it.
    pub fn object<'b>(&self, banks: &'b [Arc<LoadedBank>], id: u32) -> Option<&'b HircObject> {
        banks.iter().find_map(|b| b.hierarchy.get(id))
    }

    /// Bank file names holding `id` in their DIDX (every bank of the archive is scanned once).
    fn banks_embedding(&self, id: u32) -> &[String] {
        let index = self.didx_index.get_or_init(|| {
            let mut index: HashMap<u32, Vec<String>> = HashMap::new();
            let Some(p4k) = self.p4k else { return index };
            for (key, entry) in &self.bank_entries {
                let Some(bytes) = p4k.entry_case_insensitive(entry).and_then(|e| p4k.read(e).ok()) else {
                    continue;
                };
                if let Ok(bnk) = BnkFile::parse(&bytes) {
                    for e in &bnk.data_index {
                        index.entry(e.id).or_default().push(key.clone());
                    }
                }
            }
            for banks in index.values_mut() {
                banks.sort();
            }
            index
        });
        index.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// A media file's bytes and where it was found. `home` is the bank whose HIRC references it (searched first after
    /// the loose files); `language_specific` sounds look in the language's media folder first.
    pub fn media(&self, id: u32, home: Option<&LoadedBank>, language_specific: bool) -> Option<(Vec<u8>, MediaSource)> {
        let read = |path: String| {
            let p4k = self.p4k?;
            p4k.entry_case_insensitive(&path)
                .and_then(|e| p4k.read(e).ok())
                .map(|b| (b, MediaSource::Loose { path: path.replace('\\', "/") }))
        };
        let language = format!("{MEDIA_DIR}{LANGUAGE}\\{id}.wem");
        let root = format!("{MEDIA_DIR}{id}.wem");
        if language_specific && self.loose_language.contains(&id) {
            if let Some(found) = read(language.clone()) {
                return Some(found);
            }
        }
        if self.loose.contains(&id) {
            if let Some(found) = read(root) {
                return Some(found);
            }
        }
        if self.loose_language.contains(&id) {
            if let Some(found) = read(language) {
                return Some(found);
            }
        }
        if let Some(bytes) = home.and_then(|b| b.embedded(id)) {
            return Some((bytes.to_vec(), MediaSource::Bank { bank: home.unwrap().name.clone() }));
        }
        for key in self.banks_embedding(id) {
            if let Some(bank) = self.bank(key) {
                if let Some(bytes) = bank.embedded(id) {
                    return Some((bytes.to_vec(), MediaSource::Bank { bank: bank.name.clone() }));
                }
            }
        }
        None
    }

    /// Whether a media id exists anywhere (without reading it).
    pub fn media_exists(&self, id: u32, home: Option<&LoadedBank>) -> bool {
        self.loose.contains(&id)
            || self.loose_language.contains(&id)
            || home.is_some_and(|b| b.didx.iter().any(|e| e.id == id))
            || !self.banks_embedding(id).is_empty()
    }
}
