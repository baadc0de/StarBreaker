//! Body-only clip packages for actions requested after a player kit is loaded.
use super::{
    clips,
    loadouts::read,
    sources::Sources,
    xml::{Result, error},
};
use crate::{DecomposedExport, ExportedFile, ExportedFileKind};
use serde_json::json;
use starbreaker_datacore::Database;
use starbreaker_p4k::MappedP4k;
use std::collections::BTreeSet;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PlayerClipsRequest {
    pub gender: String,
    pub clips: Vec<String>,
}

impl PlayerClipsRequest {
    pub fn names(&self) -> BTreeSet<String> {
        self.clips
            .iter()
            .map(|clip| clip.trim().to_ascii_lowercase())
            .filter(|clip| !clip.is_empty())
            .collect()
    }
}

pub fn export_player_clips(
    p4k: &MappedP4k,
    db: &Database,
    request: &PlayerClipsRequest,
) -> Result<DecomposedExport> {
    let gender: super::super::Gender = request.gender.parse().map_err(error)?;
    let skeleton = super::super::body::actor_skeleton_path(db, gender.tag())
        .ok_or_else(|| error("no player skeleton"))?;
    let bones = crate::skeleton::parse_skeleton(&read(p4k, &skeleton)?)
        .ok_or_else(|| error("player skeleton failed to parse"))?;
    let mut sources = Sources::new(p4k, &skeleton)?;
    let root = "Packages/player_clips";
    let mut export = DecomposedExport::default();
    let mut index = Vec::new();
    let mut filenames = BTreeSet::new();
    let mut unsupported = Vec::new();
    let mut missing = Vec::new();
    for name in request.names() {
        if name.ends_with(".aim")
            || sources
                .files
                .get(&name)
                .is_some_and(|path| !path.to_ascii_lowercase().ends_with(".caf"))
        {
            unsupported.push(name);
            continue;
        }
        let (clip, flags) = match sources.clip(&name) {
            Ok(value) => value,
            Err(reason) => {
                missing.push(json!({"name":name,"reason":reason.to_string()}));
                continue;
            }
        };
        let filename = crate::animation::sanitize_clip_filename(&name);
        if !filenames.insert(filename.clone()) {
            return Err(error(format!("body clip filename collision: {name}")));
        }
        let sidecar = format!("animations/{filename}.json");
        let (record, body) = crate::animation::split_clip_for_sidecar(
            &clips::sidecar(clip, *flags, &skeleton, &bones)?,
            &sidecar,
        );
        index.push(record);
        export.files.push(ExportedFile {
            relative_path: format!("{root}/{sidecar}"),
            bytes: serde_json::to_vec(&body).map_err(|e| error(e.to_string()))?,
            kind: ExportedFileKind::PackageManifest,
        });
    }
    export.files.push(ExportedFile {
        relative_path: format!("{root}/scene.json"),
        bytes: serde_json::to_vec(&json!({
            "format_version":1,
            "player_clips":{"gender":request.gender.to_ascii_lowercase(),"skeleton":skeleton,"missing":missing,"unsupported":unsupported},
            "root_entity":{"animations":index},
        })).map_err(|e| error(e.to_string()))?,
        kind: ExportedFileKind::PackageManifest,
    });
    Ok(export)
}
