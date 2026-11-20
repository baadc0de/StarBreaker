//! Full names of `Tag` records, as the tag database nests them
//! (`/Interaction/LightGroup/SetEmergency`).
//!
//! A `Tag` record carries only its own `tagName` and its `children`; its full
//! name is the path of names from the `TagDatabase` root down to it.
//! [`tag_paths`] builds the id → path map once from the main `TagDatabase`
//! record's JSON ([`tag_paths_from_json`] is the pure part);
//! [`crate::Database::tag_path`] caches it per database.

use std::collections::HashMap;

use crate::database::Database;
use crate::types::CigGuid;

/// Every tag of a tag-database JSON value (`tags[]`, each `{tagName, _RecordId_, children[]}`), by id, with its full
/// path. A tag without a parsable `_RecordId_` is not listed (its children still are).
pub fn tag_paths_from_json(database: &serde_json::Value) -> HashMap<CigGuid, String> {
    fn walk(value: &serde_json::Value, prefix: &str, out: &mut HashMap<CigGuid, String>) {
        for tag in value.as_array().into_iter().flatten() {
            let Some(name) = tag.get("tagName").and_then(|v| v.as_str()) else { continue };
            let path = format!("{prefix}/{name}");
            if let Some(id) = tag.get("_RecordId_").and_then(|v| v.as_str()).and_then(|s| s.parse::<CigGuid>().ok()) {
                out.insert(id, path.clone());
            }
            if let Some(children) = tag.get("children") {
                walk(children, &path, out);
            }
        }
    }
    let mut out = HashMap::new();
    let root = database.get("_RecordValue_").unwrap_or(database);
    if let Some(tags) = root.get("tags") {
        walk(tags, "", &mut out);
    }
    out
}

/// The full path of every tag of the main `TagDatabase` record of `db` (empty when it has none).
pub fn tag_paths(db: &Database) -> HashMap<CigGuid, String> {
    let Some(record) = db.records_by_type_name("TagDatabase").find(|r| db.is_main_record(r)) else {
        return HashMap::new();
    };
    let json = crate::export::to_json_compact(db, record)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    json.map(|json| tag_paths_from_json(&json)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic tag tree (structure of the tag database; the names are made up).
    #[test]
    fn paths_join_the_names_from_the_root() {
        let json = serde_json::json!({"_RecordValue_": {"tags": [
            {"tagName": "A", "_RecordId_": "00000000-0000-0000-0000-000000000001", "children": [
                {"tagName": "B", "_RecordId_": "00000000-0000-0000-0000-000000000002", "children": [
                    {"tagName": "C", "_RecordId_": "00000000-0000-0000-0000-000000000003", "children": []}
                ]}
            ]},
            {"tagName": "D", "children": [{"tagName": "E", "_RecordId_": "00000000-0000-0000-0000-000000000005"}]}
        ]}});
        let paths = tag_paths_from_json(&json);
        let id = |n: u8| format!("00000000-0000-0000-0000-00000000000{n}").parse::<CigGuid>().unwrap();
        assert_eq!(paths[&id(3)], "/A/B/C");
        assert_eq!(paths[&id(1)], "/A");
        assert_eq!(paths[&id(5)], "/D/E", "a tag without an id still names its children's prefix");
        assert_eq!(paths.len(), 4);
    }

    /// LIVE 4.10.2: the light-group interaction tag the C2 light panels send.
    #[test]
    fn live_tag_paths_name_the_light_group_tags() {
        let Ok(found) = starbreaker_common::discover::find_p4k() else {
            eprintln!("skipping: no Data.p4k installed");
            return;
        };
        let p4k = starbreaker_p4k::MappedP4k::open(found.path).unwrap();
        let dcb = p4k.read_file("Data\\Game2.dcb").or_else(|_| p4k.read_file("Data\\Game.dcb")).unwrap();
        let db = Database::from_bytes(&dcb).unwrap();
        let id: CigGuid = "b27ea599-9e3f-4887-a0eb-c6e61a8f5008".parse().unwrap();
        assert_eq!(db.tag_path(&id), Some("/Interaction/LightGroup/SetEmergency"));
    }
}
