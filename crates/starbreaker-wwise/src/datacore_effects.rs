//! DataCore audio beyond `audioTrigger` fields: entity sound effects, tag paths, every ATL trigger string of a
//! record, the game's audio environments and a room's propagation parameters.
//!
//! - [`tag_paths`]: tag GUID → its path in the tag database (`EntityEffectSystem/Triggers/Elevators/RampUp`).
//! - [`sound_effects`]: `SEntityComponentEffects.entityEffects.soundEffects` — `triggerEffects[]` (a trigger tag fires
//!   a one-shot) and `tagEffects[]` (a sound plays while a state tag is set), each with its trigger, bone attachment
//!   and offset.
//! - [`door_effects`]: a door's events (`SCItemDoorParams` effect groups and `SCItemDoorEffectParams` tags) and the
//!   effect-core sounds of its groups (`SEntityEffectCoreParams` → `SEffectParamsNodeSound`).
//! - [`audio_strings`]: every string value of a record that names an ATL trigger, whatever its field
//!   (`SoundParams.Name`, `heelLandAudioTrigger`, `Light_On_SFX`, `fireAudioTrigger`, …), with its property path.
//! - [`audio_environments`]: `AudioEnvironment` records (ATL environment, size and interiority ranges).
//! - [`propagation`]: an entity class's `AudioPropagationParams` (explicit environment, encapsulation).

use std::collections::HashMap;

use serde::Serialize;
use starbreaker_datacore::database::Database;
use starbreaker_datacore::query::value::Value;
use starbreaker_datacore::types::Record;

use crate::atl::AtlIndex;
use crate::manifest::EnvironmentRecord;

fn field<'a, 'b>(value: &'b Value<'a>, name: &str) -> Option<&'b Value<'a>> {
    let Value::Object { fields, .. } = value else { return None };
    fields.iter().find(|(key, _)| *key == name).map(|(_, v)| v)
}

fn string<'a>(value: Option<&Value<'a>>) -> Option<&'a str> {
    match value {
        Some(Value::String(s) | Value::Enum(s) | Value::Locale(s)) => Some(s),
        _ => None,
    }
}

fn number(value: Option<&Value<'_>>) -> Option<f32> {
    match value {
        Some(Value::Float(f)) => Some(*f),
        Some(Value::Double(f)) => Some(*f as f32),
        Some(Value::Int32(i)) => Some(*i as f32),
        _ => None,
    }
}

fn reference_id(value: Option<&Value<'_>>) -> Option<String> {
    match value {
        Some(Value::Object { record_id: Some(id), .. }) | Some(Value::Guid(id)) => Some(id.to_string().to_ascii_lowercase()),
        _ => None,
    }
}

fn query<'a>(db: &'a Database<'a>, record: &Record, path: &str) -> Vec<Value<'a>> {
    db.compile_path::<Value>(record.struct_id(), path)
        .ok()
        .and_then(|p| db.query::<Value>(&p, record).ok())
        .unwrap_or_default()
}

/// Tag GUID (lower case) → path of tag names from the tag database root. The database's `tags` and each tag's
/// `children` are references to `Tag` records.
pub fn tag_paths(db: &Database<'_>) -> HashMap<String, String> {
    fn ids(values: Vec<Value<'_>>) -> Vec<starbreaker_datacore::types::CigGuid> {
        let mut out = Vec::new();
        for v in values {
            match v {
                Value::Array(items) => out.extend(ids(items)),
                Value::Guid(id) | Value::Object { record_id: Some(id), .. } => out.push(id),
                _ => {}
            }
        }
        out
    }
    fn walk(db: &Database<'_>, id: &starbreaker_datacore::types::CigGuid, prefix: &str, out: &mut HashMap<String, String>, depth: usize) {
        let key = id.to_string().to_ascii_lowercase();
        if depth > 32 || out.contains_key(&key) {
            return;
        }
        let Some(record) = db.record_by_id(id) else { return };
        let Some(name) = query(db, record, "tagName").into_iter().find_map(|v| string(Some(&v)).map(str::to_owned)) else { return };
        let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        out.insert(key, path.clone());
        for child in ids(query(db, record, "children")) {
            walk(db, &child, &path, out, depth + 1);
        }
    }
    let mut out = HashMap::new();
    for record in db.records_by_type_name("TagDatabase") {
        for id in ids(query(db, record, "tags")) {
            walk(db, &id, "", &mut out, 0);
        }
    }
    out
}

/// One entity sound effect.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SoundEffect {
    /// `trigger`: the tag fires the sound once; `tag`: the sound plays while the tag is set.
    pub on: &'static str,
    /// The tag's own name (the last element of `tag_path`).
    pub tag: String,
    pub tag_path: String,
    /// The effect's authored label.
    pub label: String,
    pub trigger: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bone: Option<String>,
    pub offset: [f32; 3],
    pub multi_position: bool,
}

/// The sound effects of an entity class record (enabled ones with a trigger).
pub fn sound_effects(db: &Database<'_>, record: &Record, tags: &HashMap<String, String>) -> Vec<SoundEffect> {
    let mut out = Vec::new();
    for (on, list, tag_field) in [("trigger", "triggerEffects", "trigger"), ("tag", "tagEffects", "tag")] {
        let path = format!("Components[SEntityComponentEffects].entityEffects.soundEffects.{list}");
        for value in query(db, record, &path) {
            let items = match value {
                Value::Array(items) => items,
                item => vec![item],
            };
            for effect in &items {
                if matches!(field(effect, "enabled"), Some(Value::Bool(false))) {
                    continue;
                }
                let Some(trigger) = field(effect, "soundEffect").and_then(|s| string(field(s, "audioTrigger"))).filter(|t| !t.is_empty())
                else {
                    continue;
                };
                let tag_path = reference_id(field(effect, tag_field)).and_then(|id| tags.get(&id).cloned()).unwrap_or_default();
                let attach = field(effect, "attachmentSettings");
                let bone = attach.and_then(|a| string(field(a, "boneAttachment"))).filter(|b| !b.is_empty()).map(str::to_owned);
                let position = attach.and_then(|a| field(a, "offset")).and_then(|o| field(o, "Position"));
                let axis = |k| number(position.and_then(|p| field(p, k))).unwrap_or(0.0);
                out.push(SoundEffect {
                    on,
                    tag: tag_path.rsplit('/').next().unwrap_or_default().to_owned(),
                    tag_path,
                    label: string(field(effect, "name")).unwrap_or_default().to_owned(),
                    trigger: trigger.to_owned(),
                    bone,
                    offset: [axis("x"), axis("y"), axis("z")],
                    multi_position: matches!(field(effect, "multiPosition"), Some(Value::Bool(true))),
                });
            }
        }
    }
    out
}

/// A door event (`open`, `opened`, `opening`, …): the effect group it plays (`SCItemDoorParams.<Event>EffectGroup`) and
/// the entity-effect tag it fires or holds (`SCItemDoorParams.EffectParams`, `SCItemDoorEffectParams`).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct DoorEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// The tag's own name (the last element of `tag_path`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_path: Option<String>,
}

/// An effect-core sound (`SEntityEffectCoreParams` → `SEffectParamsNodeSound.SoundParams`) of an effect group.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DoorSound {
    /// The effect group (`SoundParams.tag`) whose event plays it.
    pub group: String,
    pub trigger: String,
    /// The helper it plays at (a bone or slot helper, `$`-prefixed for the entity's own helpers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub helper: Option<String>,
    pub offset: [f32; 3],
    /// Plays while its group's state lasts (the opening / closing loops), else once.
    pub looped: bool,
}

/// The sound side of an entity class's door.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct DoorEffects {
    /// Event name → its group and tag; events with neither are left out.
    pub events: std::collections::BTreeMap<String, DoorEvent>,
    /// Every effect-core sound with a trigger. `Enabled` is not a filter: it is the node's state at spawn (in the door
    /// records nearly every node is authored disabled, and the groups play them).
    pub sounds: Vec<DoorSound>,
}

/// `SCItemDoorParams` effect-group fields and `SCItemDoorEffectParams` tag fields, by event.
const DOOR_EVENTS: [(&str, &str, &str); 14] = [
    ("open", "OpenEffectGroup", "openTrigger"),
    ("close", "CloseEffectGroup", "closeTrigger"),
    ("opened", "OpenedEffectGroup", "openedTrigger"),
    ("closed", "ClosedEffectGroup", "closedTrigger"),
    ("opening", "OpeningEffectGroup", "openingTag"),
    ("closing", "ClosingEffectGroup", "closingTag"),
    ("override_open", "OverrideOpenEffectGroup", "overrideOpenTrigger"),
    ("override_close", "OverrideCloseEffectGroup", "overrideCloseTrigger"),
    ("override_opened", "OverrideOpenedEffectGroup", "overrideOpenedTrigger"),
    ("override_closed", "OverrideClosedEffectGroup", "overrideClosedTrigger"),
    ("override_opening", "OverrideOpeningEffectGroup", "overrideOpeningTag"),
    ("override_closing", "OverrideClosingEffectGroup", "overrideClosingTag"),
    ("locked", "LockedEffectGroup", "lockedTrigger"),
    ("unlocked", "UnlockedEffectGroup", "unlockedTrigger"),
];

/// The door effects of an entity class record; `None` when it has no `SCItemDoorParams`.
pub fn door_effects(db: &Database<'_>, record: &Record, tags: &HashMap<String, String>) -> Option<DoorEffects> {
    let door = query(db, record, "Components[SCItemDoorParams]").into_iter().next()?;
    let effect_params = field(&door, "EffectParams");
    let mut out = DoorEffects::default();
    for (event, group_field, tag_field) in DOOR_EVENTS {
        let group = string(field(&door, group_field)).filter(|g| !g.is_empty()).map(str::to_owned);
        let tag_path = effect_params.and_then(|p| reference_id(field(p, tag_field))).and_then(|id| tags.get(&id).cloned());
        if group.is_none() && tag_path.is_none() {
            continue;
        }
        let tag = tag_path.as_deref().map(|p| p.rsplit('/').next().unwrap_or_default().to_owned());
        out.events.insert(event.to_owned(), DoorEvent { group, tag, tag_path });
    }
    fn walk(node: &Value<'_>, out: &mut Vec<DoorSound>) {
        match node {
            Value::Array(items) => items.iter().for_each(|n| walk(n, out)),
            Value::Object { type_name, .. } => {
                if *type_name == "SEffectParamsNodeSound"
                    && let Some(sound) = field(node, "SoundParams")
                    && let Some(trigger) = string(field(sound, "Name")).filter(|t| !t.is_empty())
                {
                    let position = field(sound, "Offset").and_then(|o| field(o, "Position"));
                    let axis = |k| number(position.and_then(|p| field(p, k))).unwrap_or(0.0);
                    out.push(DoorSound {
                        group: string(field(sound, "tag")).unwrap_or_default().to_owned(),
                        trigger: trigger.to_owned(),
                        helper: string(field(sound, "Helper")).filter(|h| !h.is_empty()).map(str::to_owned),
                        offset: [axis("x"), axis("y"), axis("z")],
                        looped: matches!(field(sound, "IsLooped"), Some(Value::Bool(true))),
                    });
                }
                if let Some(sub) = field(node, "SubNodes") {
                    walk(sub, out);
                }
            }
            _ => {}
        }
    }
    // The node list is polymorphic: the query names the sound nodes' type (a typed path, as `Components[...]`). In the
    // door records every sound node is at the top level; a sound node's own sound sub-nodes are walked too.
    for nodes in query(db, record, "Components[SEntityEffectCoreParams].EffectParams[SEffectParamsNodeSound]") {
        walk(&nodes, &mut out.sounds);
    }
    Some(out)
}

/// An ATL trigger named by a record field.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AudioString {
    pub path: String,
    pub trigger: String,
}

/// Every string of `record`'s components that names an ATL trigger (references are not followed).
pub fn audio_strings(db: &Database<'_>, record: &Record, atl: &AtlIndex) -> Vec<AudioString> {
    fn walk(value: &Value<'_>, path: &str, atl: &AtlIndex, out: &mut Vec<AudioString>) {
        match value {
            Value::String(s) if !s.is_empty() && atl.get_trigger(s).is_some() => {
                out.push(AudioString { path: path.to_owned(), trigger: (*s).to_owned() })
            }
            Value::Object { type_name, fields, .. } => {
                for (name, v) in fields {
                    let p = if path.is_empty() { (*name).to_owned() } else { format!("{path}.{name}") };
                    walk(v, &p, atl, out);
                }
                let _ = type_name;
            }
            Value::Array(items) => {
                for (i, v) in items.iter().enumerate() {
                    let tag = match v {
                        Value::Object { type_name, .. } => format!("{path}[{type_name}]"),
                        _ => format!("{path}[{i}]"),
                    };
                    walk(v, &tag, atl, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    if let Ok(path) = db.compile_path::<Value>(record.struct_id(), "Components") {
        for value in db.query_no_references(&path, record).unwrap_or_default() {
            walk(&value, "Components", atl, &mut out);
        }
    }
    out
}

/// The `AudioEnvironment` records: `(ATL environment name, record)`.
pub fn audio_environments(db: &Database<'_>) -> Vec<(String, EnvironmentRecord)> {
    let mut out = Vec::new();
    for record in db.records_by_type_name("AudioEnvironment") {
        let name = db.resolve_string2(record.name_offset).to_owned();
        let get = |p: &str| query(db, record, p).into_iter().next();
        let Some(env) = string(get("wwiseEnvironmentName").as_ref()).map(str::to_owned) else { continue };
        let num = |p: &str| number(get(p).as_ref()).unwrap_or(0.0);
        out.push((
            env,
            EnvironmentRecord {
                record: name,
                size: (num("sizeMinimum"), num("sizeMaximum")),
                interiority: (num("interiorityMinimum"), num("interiorityMaximum")),
            },
        ));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// An entity class's `AudioPropagationParams`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Propagation {
    /// The explicit environment record name, when the class names one.
    pub audio_environment: Option<String>,
    pub physical_encapsulation: Option<f32>,
    pub physical_occlusion_when_closed: Option<f32>,
}

pub fn propagation(db: &Database<'_>, record: &Record) -> Option<Propagation> {
    let value = query(db, record, "Components[AudioPropagationParams]").into_iter().next()?;
    let audio_environment = match field(&value, "audioEnvironment") {
        Some(Value::Object { record_id: Some(id), .. }) | Some(Value::Guid(id)) if !id.is_empty() => {
            db.record_by_id(id).map(|r| db.resolve_string2(r.name_offset).to_owned())
        }
        Some(Value::String(s)) if !s.is_empty() => Some((*s).to_owned()),
        _ => None,
    };
    Some(Propagation {
        audio_environment,
        physical_encapsulation: number(field(&value, "physicalEncapsulationValue")),
        physical_occlusion_when_closed: number(field(&value, "physicalOcclusionValueWhenClosed")),
    })
}
