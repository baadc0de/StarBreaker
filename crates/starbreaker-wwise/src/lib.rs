//! Wwise soundbanks, the game's ATL trigger dictionary, DataCore audio references, and the playable-tree / package
//! audio model built from them (`library`, `playable`, `mix`, `manifest`, `convolution`).

pub mod atl;
pub mod convolution;
pub mod library;
pub mod playable;
pub mod mix;
pub mod manifest;
pub mod datacore_effects;
#[cfg(test)]
mod hirc_fixture;
pub mod bkhd;
pub mod bnk;
pub mod datacore_audio;
pub mod didx;
pub mod error;
pub mod external_sources;
pub mod fnv;
pub mod game_audio;
pub mod hirc;
pub mod section;
pub mod stid;
pub mod stmg;

pub use atl::{AtlIndex, AtlTrigger};
pub use bnk::BnkFile;
pub use datacore_audio::{AudioTriggerRef, EntityAudioInfo};
pub use didx::DataIndexEntry;
pub use error::BnkError;
pub use external_sources::{ExternalSourceEntry, ExternalSourceIndex};
pub use game_audio::{GameAudioDoc, load as load_game_audio};
pub use fnv::fnv1_hash;
pub use hirc::{
    HircEntry, HircObject, HircObjectType, HircSection, Hierarchy,
    ResolvedSound, SoundSource,
};
pub use section::SectionTag;
