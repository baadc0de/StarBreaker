//! Resolves the player's authored ladder contact configuration and unarmed
//! mount, climb and dismount fragments through the regular Mannequin resolver.
use super::{
    motion, ports,
    xml::{Result, error},
};
use serde_json::{Value, json};
use starbreaker_datacore::{
    Database,
    loadout::EntityIndex,
    types::{CigGuid, Record},
};
use starbreaker_p4k::MappedP4k;
use std::collections::BTreeSet;

pub(crate) fn definition(db: &Database, record: &Record) -> Option<Value> {
    let class = ports::query(db, record, "Components[LadderComponentParams]").ok()?;
    class.is_object().then_some(class)
}

pub(crate) fn apply_overrides(
    definition: &mut Value,
    overrides: &std::collections::BTreeMap<String, String>,
) {
    for (key, raw) in overrides {
        if let Some(field) = definition.get_mut(key) {
            match field {
                Value::Bool(_) => {
                    let value = match raw.as_str() {
                        "1" => Some(true),
                        "0" => Some(false),
                        text => text.parse().ok(),
                    };
                    if let Some(value) = value {
                        *field = json!(value);
                    }
                }
                Value::Number(_) => {
                    if let Ok(value) = raw.parse::<f64>()
                        && let Some(number) = serde_json::Number::from_f64(value)
                    {
                        *field = Value::Number(number);
                    }
                }
                _ => {}
            }
        }
    }
}

pub(crate) fn resolve(p4k: &MappedP4k, db: &Database, index: &EntityIndex) -> Result<Value> {
    let actor = index
        .find_record(super::super::body::PLAYER_ACTOR)
        .ok_or_else(|| error("Player record missing for ladder fragments"))?;
    let reference = ports::query(
        db,
        actor,
        "Components[SActorComponentParams].ladderConfigV2",
    )?;
    let config = if reference.is_object() {
        reference
    } else {
        let record = reference
            .as_str()
            .and_then(|value| value.parse::<CigGuid>().ok())
            .and_then(|id| db.record_by_id(&id))
            .ok_or_else(|| error("Missing Player actor ladderConfigV2 record"))?;
        starbreaker_datacore::query::record_json::record_json(db, record)
            .map_err(|failure| error(failure.to_string()))?
    };
    let controller = ports::query(db, actor, "Components[SAnimatedCharacterComponentParams]")?;
    let database = controller["animDatabase3P"]
        .as_str()
        .filter(|path| !path.is_empty())
        .ok_or_else(|| error("Player actor has no third-person animation database"))?;
    const MOUNT: &str = "LadderGetOnV2";
    const CLIMB: &str = "LadderClimbV2";
    const DISMOUNT: &str = "LadderGetOffV2";
    let requests = [
        ("mount_bottom", MOUNT, "up+walk"),
        ("mount_top", MOUNT, "lfoot+down+walk"),
        ("climb", CLIMB, "loop+up+walk"),
        ("dismount_bottom", DISMOUNT, "down+walk"),
        ("dismount_back_left", DISMOUNT, "lfoot+up+walk"),
        ("dismount_back_right", DISMOUNT, "rfoot+up+walk"),
        ("dismount_left_left", DISMOUNT, "lfoot+upleft+walk"),
        ("dismount_left_right", DISMOUNT, "rfoot+upleft+walk"),
        ("dismount_right_left", DISMOUNT, "lfoot+upright+walk"),
        ("dismount_right_right", DISMOUNT, "rfoot+upright+walk"),
    ];
    let mut fragments = Vec::new();
    motion::adb(
        p4k,
        database,
        &BTreeSet::from([MOUNT.into(), CLIMB.into(), DISMOUNT.into()]),
        &[],
        &mut BTreeSet::new(),
        &mut fragments,
        &mut Default::default(),
    )?;
    let actions = requests
        .into_iter()
        .filter_map(|(state, fragment, tags)| {
            let mut chosen =
                motion::resolve(&fragments, fragment, &[], &ports::words(tags))?.clone();
            chosen["state"] = json!(state);
            Some(chosen)
        })
        .collect::<Vec<_>>();
    Ok(json!({"config": config, "actions": actions}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_instance_fields_override_only_their_class_values() {
        let mut definition = json!({"height": 5.0, "side": false, "untouched": 2.0, "nested": {}});
        apply_overrides(
            &mut definition,
            &[
                ("height".to_owned(), "3.25".to_owned()),
                ("side".to_owned(), "1".to_owned()),
                ("untouched".to_owned(), "NaN".to_owned()),
                ("missing".to_owned(), "1".to_owned()),
                ("nested".to_owned(), "1".to_owned()),
            ]
            .into(),
        );
        assert_eq!(
            definition,
            json!({"height": 3.25, "side": true, "untouched": 2.0, "nested": {}})
        );
    }
}
