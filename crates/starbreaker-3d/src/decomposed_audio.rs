//! The package's audio (WP-A1): placed-audio JSON with class defaults, and the `audio.json` post-pass.
//!
//! - [`AudioClasses`]: `interiors[].audio_spots[]` / `audio_areas[]` — each placed entity's component overrides merged
//!   over its class's DataCore component (`AudioTriggerSpotParams` / `AudioAreaAmbienceComponentParams`), so every
//!   parameter a player needs is present.
//! - [`add_package_audio`]: after the scene and clips are written, collect every trigger the package references —
//!   spots and areas, clip-source `audio_events`, the entity classes' sound effects and the hooked item actions — and
//!   the rooms' audio environments, then write `Packages/<name>/audio.json` and the media and impulse responses under
//!   `Audio/` (shared between packages, like `Data/`). See `docs/decomposed-export-contract.md` §Audio.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::str::FromStr;

use serde_json::{Map, Value as Json, json};
use starbreaker_datacore::database::Database;
use starbreaker_datacore::query::value::Value;
use starbreaker_datacore::types::CigGuid;
use starbreaker_p4k::MappedP4k;
use starbreaker_wwise::datacore_effects as dce;
use starbreaker_wwise::library::AudioLibrary;
use starbreaker_wwise::manifest::ManifestBuilder;

use crate::socpak::audio_entities::{InteriorAudio, PlacedAudio};

/// Package-relative directory of the media and impulse responses.
pub(crate) const AUDIO_DIR: &str = "Audio";

/// Item components whose audio-trigger fields the viewer plays on the item's own actions (WP-A1 hooks: the
/// flashlight toggle, weapon fire and aim). Their triggers are resolved into the manifest.
const HOOKED_ITEM_COMPONENTS: [&str; 2] = ["SCItemFlashlightParams", "SCItemWeaponComponentParams"];

/// Class-default cache for placed audio.
#[derive(Default)]
pub(crate) struct AudioClasses {
    defaults: HashMap<(String, &'static str), Map<String, Json>>,
}

impl AudioClasses {
    fn class_defaults(&mut self, db: &Database<'_>, class: Option<&str>, component: &'static str) -> Map<String, Json> {
        let Some(class) = class else { return Map::new() };
        self.defaults
            .entry((class.to_owned(), component))
            .or_insert_with(|| {
                let Some(record) = CigGuid::from_str(class).ok().and_then(|g| db.record_by_id(&g)) else { return Map::new() };
                let path = format!("Components[{component}]");
                let mut out = Map::new();
                if let Ok(compiled) = db.compile_path::<Value>(record.struct_id(), &path) {
                    for value in db.query_no_references(&compiled, record).unwrap_or_default() {
                        flatten(&value, "", &mut out);
                    }
                }
                out
            })
            .clone()
    }

    fn placed_json(&mut self, db: &Database<'_>, p: &PlacedAudio, component: &'static str) -> Json {
        let mut params = self.class_defaults(db, p.class_guid.as_deref(), component);
        for (k, v) in &p.params {
            params.insert(k.clone(), scalar(v));
        }
        // The trigger names live in their own fields.
        params.retain(|k, _| !k.ends_with(".audioTrigger"));
        json!({
            "entity_id": p.entity_id,
            "name": p.name,
            "class_guid": p.class_guid,
            "parent_entity_id": p.parent_entity_id,
            "position": p.position,
            "extra_positions": p.extra_positions,
            "play_trigger": p.play_trigger,
            "stop_trigger": p.stop_trigger,
            "room": p.room,
            "links": p.links,
            "params": params,
        })
    }

    pub(crate) fn spots_json(&mut self, db: &Database<'_>, audio: &InteriorAudio) -> Vec<Json> {
        audio.spots.iter().map(|s| self.placed_json(db, s, "AudioTriggerSpotParams")).collect()
    }

    pub(crate) fn areas_json(&mut self, db: &Database<'_>, audio: &InteriorAudio) -> Vec<Json> {
        audio
            .areas
            .iter()
            .map(|a| {
                let mut v = self.placed_json(db, a, "AudioAreaAmbienceComponentParams");
                v["shapes"] = Json::Array(a.shapes.iter().map(|s| s.to_json()).collect());
                v
            })
            .collect()
    }
}

/// Scalar fields of a DataCore component, nested objects flattened with dots (`audioRTPCAreaFadeRtpc.rtpc`).
fn flatten(value: &Value<'_>, prefix: &str, out: &mut Map<String, Json>) {
    let Value::Object { fields, .. } = value else { return };
    for (name, v) in fields {
        let key = if prefix.is_empty() { (*name).to_owned() } else { format!("{prefix}.{name}") };
        let j = match v {
            Value::Bool(b) => json!(b),
            Value::Float(f) => json!(f),
            Value::Double(f) => json!(f),
            Value::Int8(i) => json!(i),
            Value::Int16(i) => json!(i),
            Value::Int32(i) => json!(i),
            Value::Int64(i) => json!(i),
            Value::UInt8(i) => json!(i),
            Value::UInt16(i) => json!(i),
            Value::UInt32(i) => json!(i),
            Value::UInt64(i) => json!(i),
            Value::String(s) | Value::Enum(s) | Value::Locale(s) => json!(s),
            Value::Object { .. } => {
                flatten(v, &key, out);
                continue;
            }
            _ => continue,
        };
        out.insert(key, j);
    }
}

/// An authored attribute as JSON: number, bool or string.
fn scalar(text: &str) -> Json {
    if let Ok(f) = text.trim().parse::<f64>() {
        return json!(f);
    }
    match text.trim() {
        "true" => json!(true),
        "false" => json!(false),
        other => json!(other),
    }
}

/// Volume (m³) of a room-volume JSON record (box, prism or closed trimesh), scaled by its transform.
pub(crate) fn shape_volume(shape: &Json) -> f64 {
    let m: Vec<f64> = shape["transform"].as_array().into_iter().flatten().flat_map(|c| c.as_array().into_iter().flatten().filter_map(Json::as_f64)).collect();
    let det = if m.len() == 16 {
        let a = |c: usize, r: usize| m[c * 4 + r];
        (a(0, 0) * (a(1, 1) * a(2, 2) - a(2, 1) * a(1, 2)) - a(1, 0) * (a(0, 1) * a(2, 2) - a(2, 1) * a(0, 2))
            + a(2, 0) * (a(0, 1) * a(1, 2) - a(1, 1) * a(0, 2)))
        .abs()
    } else {
        1.0
    };
    let p3 = |v: &Json| -> [f64; 3] {
        let a: Vec<f64> = v.as_array().into_iter().flatten().filter_map(Json::as_f64).collect();
        [a.first().copied().unwrap_or(0.0), a.get(1).copied().unwrap_or(0.0), a.get(2).copied().unwrap_or(0.0)]
    };
    let local = match shape["kind"].as_str() {
        Some("box") => {
            let (lo, hi) = (p3(&shape["box_min"]), p3(&shape["box_max"]));
            (hi[0] - lo[0]).abs() * (hi[1] - lo[1]).abs() * (hi[2] - lo[2]).abs()
        }
        Some("prism") => {
            let pts: Vec<[f64; 3]> = shape["points"].as_array().into_iter().flatten().map(p3).collect();
            let area: f64 = (0..pts.len()).map(|i| {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                a[0] * b[1] - b[0] * a[1]
            }).sum::<f64>().abs() / 2.0;
            area * shape["height"].as_f64().unwrap_or(0.0)
        }
        Some("trimesh") => {
            let v: Vec<[f64; 3]> = shape["vertices"].as_array().into_iter().flatten().map(p3).collect();
            v.chunks_exact(3)
                .map(|t| {
                    let (a, b, c) = (t[0], t[1], t[2]);
                    a[0] * (b[1] * c[2] - c[1] * b[2]) - b[0] * (a[1] * c[2] - c[1] * a[2]) + c[0] * (a[1] * b[2] - b[1] * a[2])
                })
                .sum::<f64>()
                .abs()
                / 6.0
        }
        _ => 0.0,
    };
    local * det
}

/// The audio environment a room is given: the `AudioEnvironment` record whose interiority range holds the room's
/// encapsulation and whose size range holds the room's size (cube root of its volume, metres), the nearest size
/// range when the size falls between ranges. `None` when no record's interiority range holds the encapsulation.
pub(crate) fn choose_environment<'a>(
    environments: &'a [(String, starbreaker_wwise::manifest::EnvironmentRecord)],
    size_m: f32,
    interiority: f32,
) -> Option<&'a str> {
    let gap = |r: (f32, f32)| if size_m < r.0 { r.0 - size_m } else if size_m > r.1 { size_m - r.1 } else { 0.0 };
    environments
        .iter()
        .filter(|(_, e)| interiority >= e.interiority.0 && interiority <= e.interiority.1)
        .min_by(|a, b| gap(a.1.size).total_cmp(&gap(b.1.size)).then(a.0.cmp(&b.0)))
        .map(|(name, _)| name.as_str())
}

/// Collect the package's triggers and rooms, build `audio.json` and add it and its media to `files`.
pub(crate) fn add_package_audio(db: &Database<'_>, p4k: &MappedP4k, files: &mut crate::decomposed::OutputFiles, package_name: &str) {
    let scene_path = format!("Packages/{package_name}/scene.json");
    let Some(scene) = files.get(&scene_path).and_then(|b| serde_json::from_slice::<Json>(b).ok()) else { return };
    let mut triggers: BTreeSet<String> = BTreeSet::new();
    fn add(t: Option<&str>, set: &mut BTreeSet<String>) {
        if let Some(t) = t.filter(|t| !t.is_empty()) {
            set.insert(t.to_owned());
        }
    }
    let mut classes: BTreeSet<String> = BTreeSet::new();
    let mut rooms: BTreeMap<String, (f64, Option<String>, Option<String>)> = BTreeMap::new();
    let (mut spot_count, mut area_count) = (0usize, 0usize);
    for interior in scene["interiors"].as_array().into_iter().flatten() {
        for spot in interior["audio_spots"].as_array().into_iter().flatten() {
            spot_count += 1;
            add(spot["play_trigger"].as_str(), &mut triggers);
            add(spot["stop_trigger"].as_str(), &mut triggers);
        }
        for area in interior["audio_areas"].as_array().into_iter().flatten() {
            area_count += 1;
            add(area["play_trigger"].as_str(), &mut triggers);
            add(area["stop_trigger"].as_str(), &mut triggers);
        }
        for list in ["placements", "entities", "lifts"] {
            for e in interior[list].as_array().into_iter().flatten() {
                if let Some(g) = e["entity_class_guid"].as_str() {
                    classes.insert(g.to_ascii_lowercase());
                }
            }
        }
        for volume in interior["room_volumes"].as_array().into_iter().flatten() {
            let Some(room) = volume["room"].as_str() else { continue };
            let entry = rooms.entry(room.to_owned()).or_insert((0.0, None, None));
            entry.0 += shape_volume(volume);
            entry.1 = entry.1.take().or_else(|| volume["room_name"].as_str().map(str::to_owned));
            entry.2 = entry.2.take().or_else(|| volume["room_class"].as_str().map(str::to_owned));
        }
    }
    let clip_prefix = format!("Packages/{package_name}/");
    for (path, bytes) in files.iter() {
        if !path.starts_with(&clip_prefix) || !path.contains("/animations/") || !path.ends_with(".json") {
            continue;
        }
        let Ok(clip) = serde_json::from_slice::<Json>(bytes) else { continue };
        let clips = if clip.is_array() { clip.as_array().cloned().unwrap_or_default() } else { vec![clip] };
        for c in &clips {
            for src in c["sources"].as_array().into_iter().flatten().chain(std::iter::once(c)) {
                for e in src["audio_events"].as_array().into_iter().flatten() {
                    add(e["trigger"].as_str(), &mut triggers);
                }
            }
        }
    }
    let root_class = scene["root_entity"]["entity_name"]
        .as_str()
        .and_then(|name| db.records_by_type_name("EntityClassDefinition").find(|r| db.resolve_string2(r.name_offset) == name))
        .map(|r| r.id.to_string().to_ascii_lowercase());
    if let Some(root) = &root_class {
        classes.insert(root.clone());
    }

    let lib = match AudioLibrary::open(p4k) {
        Ok(lib) => lib,
        Err(e) => {
            log::warn!("[audio] no audio library: {e}");
            return;
        }
    };
    let tags = dce::tag_paths(db);
    let mut entity_classes = Map::new();
    for class in &classes {
        let Some(record) = CigGuid::from_str(class).ok().and_then(|g| db.record_by_id(&g)) else { continue };
        let effects = dce::sound_effects(db, record, &tags);
        let actions: Vec<_> = dce::audio_strings(db, record, &lib.atl)
            .into_iter()
            .filter(|a| HOOKED_ITEM_COMPONENTS.iter().any(|c| a.path.contains(&format!("[{c}]"))))
            .collect();
        if effects.is_empty() && actions.is_empty() {
            continue;
        }
        effects.iter().for_each(|e| add(Some(&e.trigger), &mut triggers));
        actions.iter().for_each(|a| add(Some(&a.trigger), &mut triggers));
        entity_classes.insert(
            class.clone(),
            json!({ "name": db.resolve_string2(record.name_offset), "effects": effects, "actions": actions }),
        );
    }

    let environments = dce::audio_environments(db);
    let mut room_out = Map::new();
    let mut used_envs = BTreeSet::new();
    for (guid, (volume, name, class)) in &rooms {
        let propagation = class
            .as_deref()
            .and_then(|c| CigGuid::from_str(c).ok())
            .and_then(|g| db.record_by_id(&g))
            .and_then(|r| dce::propagation(db, r));
        let interiority = propagation.as_ref().and_then(|p| p.physical_encapsulation);
        let size = volume.cbrt() as f32;
        let explicit = propagation.as_ref().and_then(|p| p.audio_environment.clone());
        let chosen = match &explicit {
            Some(record) => environments.iter().find(|(_, e)| &e.record == record).map(|(n, _)| n.clone()),
            None => interiority.and_then(|i| choose_environment(&environments, size, i)).map(str::to_owned),
        };
        if let Some(env) = &chosen {
            used_envs.insert(env.clone());
        }
        room_out.insert(
            guid.clone(),
            json!({
                "name": name,
                "class": class,
                "volume_m3": volume,
                "size_m": size,
                "interiority": interiority,
                "environment": chosen,
                "rule": if explicit.is_some() { "explicit AudioPropagationParams.audioEnvironment" } else { "size = cbrt(volume); AudioEnvironment interiority and size ranges" },
            }),
        );
    }

    let mut builder = ManifestBuilder::new(&lib, AUDIO_DIR);
    for t in &triggers {
        builder.trigger(t);
    }
    for (name, record) in &environments {
        if used_envs.contains(name) {
            builder.environment(name, record.clone());
        }
    }
    let (manifest, payloads) = builder.finish();
    let Ok(mut value) = serde_json::to_value(&manifest) else { return };
    value["entity_classes"] = Json::Object(entity_classes);
    value["root_entity_class"] = json!(root_class);
    value["rooms"] = Json::Object(room_out);
    value["environment_records"] = json!(environments.iter().map(|(n, e)| json!({ "environment": n, "record": e.record, "size": [e.size.0, e.size.1], "interiority": [e.interiority.0, e.interiority.1] })).collect::<Vec<_>>());
    value["emitters"] = json!({ "spots": spot_count, "areas": area_count });
    let resolved = manifest.triggers.values().filter(|t| t.status == "playable").count();
    log::info!(
        "[audio] {package_name}: {} triggers ({resolved} playable), {} media, {} environments, {spot_count} spots, {area_count} areas",
        manifest.triggers.len(),
        manifest.media.len(),
        manifest.environments.len()
    );
    for p in payloads {
        files.insert_canonical(p.path, p.bytes);
    }
    files.insert_canonical(format!("Packages/{package_name}/audio.json"), serde_json::to_vec_pretty(&value).unwrap_or_default());
}

#[cfg(test)]
#[path = "decomposed_audio_tests.rs"]
mod tests;
