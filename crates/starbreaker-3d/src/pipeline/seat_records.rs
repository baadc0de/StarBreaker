//! Structured seat and access data for decomposed loadout children. Fields are
//! read from the child's DataCore components and the owner's port controls.
use super::interior_entities::{field, number};
use super::interior_interactions::{InteractionResolver, boolean, interaction_name, query, text};
use super::seat_controls::{self, SeatRoute};
use crate::types::{EntityOverrides, EntityPayload, InteractionPoint};
use serde_json::{Value as Json, json};
use starbreaker_datacore::{Database, loadout::EntityIndex, query::value::Value, types::Record};
use starbreaker_p4k::MappedP4k;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub(crate) struct ChildSeatRecords {
    pub interaction_points: Vec<InteractionPoint>,
    pub seat: Option<Json>,
    pub access: Option<Json>,
}

fn string<'a>(value: &Value<'a>, key: &str) -> Option<&'a str> {
    field(value, key).and_then(text).filter(|s| !s.is_empty())
}

fn array<'v, 'a>(value: &'v Value<'a>, key: &str) -> &'v [Value<'a>] {
    match field(value, key) {
        Some(Value::Array(array)) => array,
        _ => &[],
    }
}

fn record_name<'a>(db: &'a Database<'a>, value: &Value) -> Option<&'a str> {
    let id = match value {
        Value::Guid(id)
        | Value::Object {
            record_id: Some(id),
            ..
        } => id,
        _ => return None,
    };
    db.record_by_id(id)
        .map(|record| db.resolve_string2(record.name_offset))
}

fn attachment(value: &Value) -> Json {
    json!({"type": string(value, "type"), "bone": string(value, "boneName"), "stance": string(value, "stance")})
}

fn camera(db: &Database, camera: &Value) -> Option<Json> {
    let config = field(camera, "baseConfig")?;
    let Value::Object { type_name, .. } = config else {
        return None;
    };
    let settings = field(config, "baseSettings");
    Some(json!({
        "record": record_name(db, camera),
        "config_type": type_name,
        "attachment_name": settings.and_then(|s| field(s, "attachmentName")).and_then(text),
        "enter_exit_camera": settings.and_then(|s| field(s, "enterExitCamera")).and_then(boolean),
        "near_plane": field(config, "FOVConfig").and_then(|f| field(f, "nearPlane")).and_then(number),
        "blend_time": field(config, "blendConfig").and_then(|b| field(b, "blendingToTime")).and_then(number),
        "fov_scale": field(config, "fovScale").and_then(number),
    }))
}

fn seat_json(db: &Database, record: &Record, seat: &Value, routing: Option<&SeatRoute>) -> Json {
    let class_tag = query(
        db,
        record,
        "Components[SAttachableComponentParams].AttachDef.mannequinTags.mannequinClassTag",
    );
    let controller = query(
        db,
        record,
        "Components[SAnimationControllerParams].AnimationController",
    );
    let iks: Vec<_> = query(
        db,
        record,
        "Components[SAnimationControllerParams].AnimationControlActorIKs",
    )
    .into_iter()
    .flat_map(|value| match value {
        Value::Array(items) => items,
        _ => Vec::new(),
    })
    .filter_map(|value| {
        Some(json!({"limb": string(&value, "limb")?, "bone": string(&value, "bone")?}))
    })
    .collect();
    let usable = query(db, record, "Components[SActorUsableParams]").map(|value| {
        let exit = array(&value, "useSlots").iter()
            .flat_map(|slot| array(slot, "useChannelInstances"))
            .find(|channel| field(channel, "useChannelArchetype").and_then(|record| record_name(db, record))
                .is_some_and(|name| name.eq_ignore_ascii_case("UseChannelArchetype.Operate")))
            .and_then(|channel| field(channel, "shouldValidateExitAnimations")).and_then(boolean);
        json!({"archetype": field(&value, "archetype").and_then(|record| record_name(db, record)), "validate_exit": exit})
    });
    json!({
        "class_tag": class_tag.as_ref().and_then(text),
        "transition_template": string(seat, "transitionTemplate"),
        "control_template": string(seat, "controlTemplate"),
        "seat_type": string(seat, "seatType"),
        "attachment": field(seat, "actorAttachment").map(attachment),
        "min_yaw": field(seat, "minYaw").and_then(number),
        "max_yaw": field(seat, "maxYaw").and_then(number),
        "min_pitch": field(seat, "minPitch").and_then(number),
        "max_pitch": field(seat, "maxPitch").and_then(number),
        "set_limits": field(seat, "setYawPitchLimits").and_then(boolean),
        "animation_based_transition": field(seat, "useAnimationBasedTransition").and_then(boolean),
        "user_scope_context": string(seat, "userScopeContext"),
        "user_adb": string(seat, "userAnimationDatabase"),
        "dashboard": {
            "adb": string(seat, "dashboardAnimationDatabase"),
            "scope_context": string(seat, "dashboardScopeContext"),
            "port": routing.and_then(|route| route.dashboard_port.as_deref()),
            "controller": controller.as_ref().and_then(text),
        },
        "actor_iks": iks,
        "camera": array(seat, "views").first().and_then(|view| camera(db, view)),
        "usable": usable,
        "can_drive": routing.and_then(|route| route.can_drive),
    })
}

fn conditions(value: &Value) -> Json {
    json!({
        "idle": string(value, "idle"),
        "landed": string(value, "landed"),
        "docked": string(value, "docked"),
        "zero_g": string(value, "zeroG"),
        "landing_gear_down": string(value, "landingGearDown"),
        "landing_gear_up": string(value, "landingGearUp"),
        "in_quantum": string(value, "inQuantum"),
    })
}

fn interaction_state(value: &Value) -> Json {
    json!({
        "interaction": interaction_name(field(value, "interaction")),
        "fragment_tags": string(value, "fragmentTags"),
        "deploy_retract_fragment_tags": string(value, "deployRetractFragmentTags"),
        "skip_state": string(value, "skipState"),
        "interior": field(value, "interiorEntrance").and_then(boolean),
        "attachment": field(value, "attachment").map(attachment),
    })
}

fn sequence(value: &Value) -> Json {
    json!({
        "dependency": string(value, "dependencyType"),
        "enter_delay": field(value, "enterSequenceDelay").and_then(number),
        "exit_delay": field(value, "exitSequenceDelay").and_then(number),
    })
}

fn entrance_json(
    entrance: &Value,
    source_port: &[String],
    children: &BTreeMap<Vec<String>, &str>,
) -> Json {
    let port = field(entrance, "seatConnection")
        .and_then(|c| field(c, "seatItemPort"))
        .and_then(|p| string(p, "itemPortName"));
    let target = source_port
        .split_last()
        .zip(port)
        .map(|((_, parent), port)| {
            let mut path = parent.to_vec();
            path.push(port.to_ascii_lowercase());
            path
        });
    let target_record = target.as_ref().and_then(|path| children.get(path));
    let links: Vec<_> = array(entrance, "linkedGateways")
        .iter()
        .map(|link| {
            json!({
                "gateway": field(link, "gateway").and_then(|gateway| string(gateway, "name")),
                "sync_to_entrance": field(link, "syncToEntranceState").and_then(boolean),
                "one_way": field(link, "oneWaySequencer").and_then(boolean),
                "ignore_close_on_enter": field(link, "ignoreCloseOnEnter").and_then(boolean),
                "ignore_close_on_exit": field(link, "ignoreCloseOnExit").and_then(boolean),
                "open": field(link, "openSequenceParams").map(sequence),
                "close": field(link, "closeSequenceParams").map(sequence),
            })
        })
        .collect();
    json!({
        "name": string(entrance, "name"),
        "conditions": field(entrance, "conditions").map(conditions),
        "seat_port": port,
        "seat_child": target_record,
        "seat_port_path": target.filter(|_| target_record.is_some()),
        "dashboard_port": field(entrance, "dashboardConnection").and_then(|d| string(d, "itemPort")),
        "enter": field(entrance, "enterState").map(interaction_state),
        "exit": field(entrance, "exitState").map(interaction_state),
        "linked_gateways": links,
    })
}

fn access_json(access: &Value, port: &[String], children: &BTreeMap<Vec<String>, &str>) -> Json {
    let gateways: Vec<_> = array(access, "gateways")
        .iter()
        .map(|gateway| {
            json!({
                "name": string(gateway, "name"),
                "conditions": field(gateway, "conditions").map(conditions),
                "open": field(gateway, "openState").map(interaction_state),
                "close": field(gateway, "closeState").map(interaction_state),
                "audio_canopy": field(gateway, "audioCanopyTreatment").and_then(boolean),
                "vehicle_fragment": string(gateway, "fragmentOverride"),
            })
        })
        .collect();
    let entrances: Vec<_> = array(access, "entrances")
        .iter()
        .map(|e| entrance_json(e, port, children))
        .collect();
    json!({"gateways": gateways, "entrances": entrances})
}

/// DataCore child interaction points derive from the same pipeline as
/// interior placements. Seats and access components add their own records.
pub(crate) fn loadout_children(
    db: &Database,
    p4k: &MappedP4k,
    root: &str,
    children: &[EntityPayload],
    include_animations: bool,
) -> Vec<ChildSeatRecords> {
    let addresses: BTreeMap<_, _> = children
        .iter()
        .filter_map(|child| {
            child.port_path.as_ref().map(|path| {
                (
                    path.iter()
                        .map(|s| s.to_ascii_lowercase())
                        .collect::<Vec<_>>(),
                    child.entity_name.as_str(),
                )
            })
        })
        .collect();
    if addresses.is_empty() {
        return vec![ChildSeatRecords::default(); children.len()];
    }
    let index = EntityIndex::new(db);
    let root_name = root.rsplit('.').next().unwrap_or(root);
    let routing = index
        .find_record(root_name)
        .map(|record| seat_controls::load(db, p4k, record))
        .unwrap_or_default();
    let mut interactions = InteractionResolver::new(db, Some(p4k));
    let mut records: Vec<_> = children
        .iter()
        .map(|child| {
            let Some((port, record)) = child
                .port_path
                .as_ref()
                .zip(index.find_record(&child.entity_name))
            else {
                return ChildSeatRecords::default();
            };
            let guid = record.id.to_string();
            let mut points = interactions.points(Some(&guid), &EntityOverrides::default(), None);
            for point in &mut points {
                point.helper_transform = super::resolve_nmc_helper_transform(
                    child.nmc.as_ref(),
                    Some(&point.helper_name),
                )
                .map(|transform| transform.to_cols_array_2d());
            }
            let routed = (port.len() == 1)
                .then(|| port[0].to_ascii_lowercase())
                .and_then(|name| routing.get(&name));
            let seat = query(db, record, "Components[SCItemSeatParams]")
                .map(|seat| seat_json(db, record, &seat, routed));
            let access = query(db, record, "Components[SCItemSeatAccessParams]").map(|access| {
                let port = port
                    .iter()
                    .map(|p| p.to_ascii_lowercase())
                    .collect::<Vec<_>>();
                access_json(&access, &port, &addresses)
            });
            ChildSeatRecords {
                interaction_points: points,
                seat,
                access,
            }
        })
        .collect();
    if include_animations && records.iter().any(|record| record.seat.is_some()) {
        let mut entrances = BTreeMap::<Vec<String>, Vec<Json>>::new();
        for entrance in records
            .iter()
            .filter_map(|record| record.access.as_ref())
            .flat_map(|access| access["entrances"].as_array().into_iter().flatten())
        {
            if let Some(path) = entrance["seat_port_path"].as_array() {
                let path = path
                    .iter()
                    .filter_map(|segment| segment.as_str().map(str::to_owned))
                    .collect();
                entrances.entry(path).or_default().push(entrance.clone());
            }
        }
        match crate::character::player::SeatMotion::new(p4k, db, &index) {
            Ok(mut motion) => {
                for (child, record) in children.iter().zip(&mut records) {
                    if let Some((seat, port)) = record.seat.as_mut().zip(child.port_path.as_ref()) {
                        let port: Vec<_> =
                            port.iter().map(|name| name.to_ascii_lowercase()).collect();
                        let use_entrances = entrances
                            .get(&port)
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>();
                        match motion.resolve(seat, &use_entrances) {
                            Ok(fragments) => seat["user_fragments"] = json!(fragments),
                            Err(error) => {
                                log::warn!("[seat] {} player fragments: {error}", child.entity_name)
                            }
                        }
                    }
                }
            }
            Err(error) => log::warn!("[seat] Player animation database unavailable: {error}"),
        }
    }
    records
}

#[cfg(test)]
#[path = "seat_records_tests.rs"]
mod tests;
