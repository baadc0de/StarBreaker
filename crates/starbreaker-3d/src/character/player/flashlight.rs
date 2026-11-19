//! Item flashlights for player kits. The game's personal light is an item component, `SCItemFlashlightParams`
//! (the worn helmet carries it: light parameters for the local player and for other viewers, the light cookie, the
//! helper it shines from and the effect tag that switches it on). [`item`] returns a loadout item's `flashlights[]`
//! entry with the component kept verbatim (source field names and types); [`package_cookies`] adds the cookies the
//! entries name to the export, the way scene projector lights package their gobos (an HDR BC6H texture as EXR,
//! anything else as package DDS), and records each one under the entry's `projector_textures` (source path ->
//! package path). [`cookie_sources`] lists the cookie paths of an entry.
use super::{
    ports::{query, text},
    xml::Result,
};
use crate::{DecomposedExport, ExportedFile, ExportedFileKind};
use serde_json::{Value, json};
use starbreaker_datacore::{Database, types::Record};
use starbreaker_p4k::MappedP4k;

const COMPONENT: &str = "SCItemFlashlightParams";

/// The `flashlights[]` entry of a loadout item, or `None` when the item carries no flashlight.
pub(super) fn item(db: &Database, record: &Record, entity: &str, port: Option<&Value>) -> Result<Option<Value>> {
    // A DataCore without the component type has no item flashlights at all.
    if db.struct_index_by_name(COMPONENT).is_none() {
        return Ok(None);
    }
    let params = query(db, record, &format!("Components[{COMPONENT}]"))?;
    if !params.is_object() {
        return Ok(None);
    }
    let port = port.map(|p| text(&p["name"])).unwrap_or("");
    Ok(Some(json!({"entity":entity,"port":port,"params":params,"projector_textures":{}})))
}

/// Every distinct non-empty `lightCookie.path` in an entry's params.
pub(super) fn cookie_sources(entry: &Value) -> Vec<String> {
    fn walk(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(fields) => {
                for (key, field) in fields {
                    if key == "lightCookie" {
                        let path = text(&field["path"]);
                        if !path.is_empty() && !out.iter().any(|p| p == path) {
                            out.push(path.to_owned());
                        }
                    } else {
                        walk(field, out);
                    }
                }
            }
            Value::Array(values) => values.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&entry["params"], &mut out);
    out
}

/// The archive DDS of a DataCore texture path (authored as `.tif`).
fn dds_source(path: &str) -> String {
    path.strip_suffix(".tif").map(|s| format!("{s}.dds")).unwrap_or_else(|| path.to_owned())
}

/// `(package path, bytes)` of one cookie at `mip`, or `None` when it is not in the archive or not decodable.
fn cookie(p4k: &MappedP4k, source: &str, mip: u32) -> Option<(String, Vec<u8>)> {
    let dds_path = dds_source(source);
    let archive = crate::pipeline::datacore_path_to_p4k(&dds_path);
    let entry = p4k.entry_case_insensitive(&archive)?;
    let bytes = p4k.read(entry).ok()?;
    let siblings = crate::pipeline::P4kSiblingReader { p4k, base_path: archive };
    let dds = starbreaker_dds::DdsFile::from_split(&bytes, &siblings).ok()?;
    let normalized = crate::decomposed::normalize_source_path(p4k, &dds_path);
    if let Some(exr) = crate::decomposed::gobo_exr(&dds, mip) {
        return Some((crate::decomposed::replace_extension(&normalized, ".exr"), exr));
    }
    starbreaker_dds::package::write_package_dds(&dds, mip, false).map(|bytes| (normalized, bytes))
}

/// Add every cookie the entries name to the export (once per path) and fill each entry's `projector_textures`.
/// A cookie the archive cannot supply stays out of the map; consumers report it.
pub(super) fn package_cookies(p4k: &MappedP4k, mip: u32, entries: &mut [Value], export: &mut DecomposedExport) {
    for entry in entries {
        for source in cookie_sources(entry) {
            let Some((path, bytes)) = cookie(p4k, &source, mip) else {
                log::warn!("player flashlight cookie '{source}' is not exportable");
                continue;
            };
            if !export.files.iter().any(|f| f.relative_path == path) {
                export.files.push(ExportedFile { relative_path: path.clone(), bytes, kind: ExportedFileKind::TextureAsset });
            }
            entry["projector_textures"][source] = json!(path);
        }
    }
}
