//! DataCore audio beyond `audioTrigger` fields: entity sound effects, tag paths, every ATL trigger string of a
//! record, the game's audio environments and a room's propagation parameters.
//!
//! - [`tag_paths`]: tag GUID → its path in the tag database (`EntityEffectSystem/Triggers/Elevators/RampUp`).
//! - [`sound_effects`]: `SEntityComponentEffects.entityEffects.soundEffects` — `triggerEffects[]` (a trigger tag fires
//!   a one-shot) and `tagEffects[]` (a sound plays while a state tag is set), each with its trigger, bone attachment
//!   and offset.
//! - [`audio_strings`]: every string value of a record that names an ATL trigger, whatever its field
//!   (`SoundParams.Name`, `heelLandAudioTrigger`, `Light_On_SFX`, `fireAudioTrigger`, …), with its property path.
//! - [`audio_environments`]: `AudioEnvironment` records (ATL environment, size and interiority ranges).
//! - [`propagation`]: an entity class's `AudioPropagationParams` (explicit environment, encapsulation).

use std::collections::{BTreeMap, HashMap};

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

/// One configured audio game context's authored state assignments and RTPC values.
#[derive(Debug, Serialize)]
pub struct GameContext {
    pub states: BTreeMap<String, String>,
    pub rtpcs: BTreeMap<String, f32>,
}

fn array_values(values: Vec<Value<'_>>) -> impl Iterator<Item = Value<'_>> {
    values.into_iter().flat_map(|value| match value {
        Value::Array(items) => items,
        item => vec![item],
    })
}

/// Emit only globals referenced by AudioGameContextSetup, keyed by the globals record's name.
pub fn game_contexts(db: &Database<'_>) -> BTreeMap<String, GameContext> {
    let file_stem = |path: &str| {
        let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
        file.rsplit_once('.').map_or(file, |(stem, _)| stem).to_ascii_lowercase()
    };
    let globals: HashMap<_, _> = db.records_by_type_name("AudioGameContextGlobals")
        .map(|record| (file_stem(db.resolve_string(record.file_name_offset)), record))
        .collect();
    let mut contexts = BTreeMap::new();
    for setup in db.records_by_type_name("AudioGameContextSetup") {
        for context in array_values(query(db, setup, "gameContexts")) {
            let globals_ref = field(&context, "globalRtpcsAndStates");
            let record = match globals_ref {
                Some(Value::Guid(id) | Value::Object { record_id: Some(id), .. }) => db.record_by_id(id),
                Some(Value::String(path)) => globals.get(&file_stem(path)).copied(),
                _ => None,
            };
            let Some(record) = record else { continue };
            let name = db.resolve_string2(record.name_offset)
                .rsplit('.').next().unwrap_or_default();
            let states = array_values(query(db, record, "globalStates")).filter_map(|entry| {
                let (group, state) = string(field(&entry, "switch"))?.split_once(':')?;
                Some((group.to_owned(), state.to_owned()))
            }).collect();
            let rtpcs = array_values(query(db, record, "globalRTPCs")).filter_map(|entry| {
                let name = string(field(&entry, "rtpc"))?;
                let value = number(field(&entry, "defaultValue"))?;
                Some((name.to_owned(), value))
            }).collect();
            contexts.insert(name.to_owned(), GameContext { states, rtpcs });
        }
    }
    contexts
}

/// Authored portal-pathing policy, separate from the room-graph attenuation parameter.
#[derive(Debug, Serialize)]
pub struct PropagationPathing {
    pub enabled: bool,
    pub actor_only: bool,
}

pub fn propagation_pathing(db: &Database<'_>) -> Option<PropagationPathing> {
    let record = db.records_by_type_name("GlobalAudioSettings")
        .find(|r| db.resolve_string2(r.name_offset).ends_with(".GlobalSettings"))?;
    let flag = |path| query(db, record, path).into_iter().find_map(|v| match v {
        Value::Bool(flag) => Some(flag),
        _ => None,
    });
    Some(PropagationPathing {
        enabled: flag("enablePropagationPathing")?,
        actor_only: flag("enablePropagationPathingActorOnly")?,
    })
}

/// A placement class carrying the authored room-connector component.
pub struct RoomConnectorClass {
    pub bounds: Option<[f32; 3]>,
    pub offset: Option<[f32; 3]>,
    pub is_door: bool,
    pub physical_occlusion_when_closed: Option<f32>,
}

pub fn room_connector(db: &Database<'_>, record: &Record) -> Option<RoomConnectorClass> {
    let component = query(db, record, "Components[SEntityComponentRoomConnectorParams]").into_iter().next()?;
    let vector = |key| {
        let value = field(&component, key)?;
        Some([
            number(field(value, "x"))?,
            number(field(value, "y"))?,
            number(field(value, "z"))?,
        ])
    };
    Some(RoomConnectorClass {
        bounds: vector("defaultAreaBounds"),
        offset: vector("boundsOffset"),
        is_door: !query(db, record, "Components[SCItemDoorParams]").is_empty(),
        physical_occlusion_when_closed: number(field(&component, "physicalOcclusionValueWhenClosed"))
            .or_else(|| propagation(db, record).and_then(|p| p.physical_occlusion_when_closed)),
    })
}
