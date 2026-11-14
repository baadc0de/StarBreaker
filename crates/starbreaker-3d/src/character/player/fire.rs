//! Weapon fire actions for player kits: the authored `SCItemWeaponComponentParams.fireActions` (rate, Mannequin
//! fragments, launch and effect helpers) plus the fragments those actions request, resolved from the weapon's
//! enslavement ADB (`SEntityComponentCarryableParams.enslavementAnimationDatabase`: the database the game runs on the
//! held item's own rig, slaved to the wearer's action controller).
//!
//! [`actions`] returns the `fire_actions` / `fire_fragments` fields of a player-kit weapon entry. Fragment options
//! keep their authored `tags` / `frag_tags`, so a consumer selects the variant for its own state (e.g. `ADS`).
use super::{
    motion,
    ports::{query, text},
    xml::Result,
};
use serde_json::{Value, json};
use starbreaker_datacore::{Database, types::Record};
use starbreaker_p4k::MappedP4k;
use std::collections::BTreeSet;

/// The archive's own spelling of a Data-relative path (DataCore stores some paths lower-cased).
fn canonical(p4k: &MappedP4k, path: &str) -> Option<String> {
    let archive = format!("Data\\{}", path.replace('/', "\\"));
    let entry = p4k.entry_case_insensitive(&archive)?;
    Some(entry.name.strip_prefix("Data\\").unwrap_or(&entry.name).replace('\\', "/"))
}

fn fragment(action: &Value, key: &str) -> Option<String> {
    let name = text(&action[key]["fragment"]);
    (!name.is_empty()).then(|| name.to_owned())
}

/// `(fire_actions, fire_fragments)` for a weapon record. A record without fire actions yields an empty array and
/// no fragments; a missing enslavement database yields the actions without fragments.
pub(super) fn actions(p4k: &MappedP4k, db: &Database, record: &Record, tags: &[String]) -> Result<(Value, Value)> {
    // A polymorphic pointer array is read through its owning component.
    let weapon = query(db, record, "Components[SCItemWeaponComponentParams]")?;
    let authored = &weapon["fireActions"];
    let mut out = Vec::new();
    let mut wanted = BTreeSet::new();
    for action in authored.as_array().into_iter().flatten() {
        let fire = fragment(action, "fireFragment");
        let stop = fragment(action, "stopFireFragment");
        let one_shot = fragment(action, "oneShotFireFragment");
        wanted.extend(fire.iter().chain(&stop).chain(&one_shot).cloned());
        let effects = action["fireEffects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["helper"].as_str().filter(|h| !h.is_empty()))
            .collect::<Vec<_>>();
        out.push(json!({"kind":action["_Type_"],"fire_rate":action["fireRate"],"fire_fragment":fire,"stop_fragment":stop,
            "one_shot_fragment":one_shot,"fire_helper":action["launchParams"]["fireHelper"],
            "muzzle_helper":action["launchParams"]["muzzleHelper"],"effect_helpers":effects}));
    }
    let database = query(db, record, "Components[SEntityComponentCarryableParams].enslavementAnimationDatabase")?;
    let mut fragments = Vec::new();
    if let Some(path) = database.as_str().filter(|p| !wanted.is_empty() && !p.is_empty()).and_then(|p| canonical(p4k, p)) {
        // Every option whose tags the weapon carries, aimed or not: the consumer picks per state.
        let mut query_tags = tags.to_vec();
        query_tags.push("ADS".into());
        motion::adb(p4k, &path, &wanted, &query_tags, &mut BTreeSet::new(), &mut fragments, &mut Default::default())?;
    }
    Ok((json!(out), json!(fragments)))
}
