//! Resolve the player's SeatState fragments from the same actor database and
//! ordered tag rules used by locomotion. Save authorship and clips for runtime.
use super::{
    motion, ports,
    xml::{Node, Result, error},
};
use serde_json::{Value, json};
use starbreaker_datacore::{Database, loadout::EntityIndex};
use starbreaker_p4k::MappedP4k;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

const SEAT_FRAGMENT: &str = "SeatState";
const LOOK_SCOPE: &str = "scope_LookAimPose";

pub(crate) struct SeatMotion<'a> {
    p4k: &'a MappedP4k,
    database: String,
    docs: BTreeMap<String, Arc<Node>>,
}

impl<'a> SeatMotion<'a> {
    pub(crate) fn new(p4k: &'a MappedP4k, db: &Database, index: &EntityIndex) -> Result<Self> {
        let actor = index
            .find_record(super::super::body::PLAYER_ACTOR)
            .ok_or_else(|| error("Player record missing for seat fragments"))?;
        let config = ports::query(db, actor, "Components[SAnimatedCharacterComponentParams]")?;
        let database = config["animDatabase3P"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| error("Player actor has no third-person animation database"))?
            .to_owned();
        Ok(Self {
            p4k,
            database,
            docs: BTreeMap::new(),
        })
    }

    pub(crate) fn resolve(&mut self, seat: &Value, entrances: &[&Value]) -> Result<Vec<Value>> {
        let class_tag = seat["class_tag"].as_str().filter(|value| !value.is_empty());
        let tags: Vec<_> = class_tag.into_iter().map(str::to_owned).collect();
        let mut query_tags = tags.clone();
        query_tags.push(LOOK_SCOPE.into());
        let mut fragments = Vec::new();
        motion::adb(
            self.p4k,
            &self.database,
            &BTreeSet::from([SEAT_FRAGMENT.into()]),
            &query_tags,
            &mut BTreeSet::new(),
            &mut fragments,
            &mut self.docs,
        )?;

        let mut requests = BTreeSet::new();
        if let Some(template) = seat["transition_template"].as_str() {
            requests.extend(transition_queries(template, entrances));
        }
        if let Some(template) = seat["control_template"].as_str() {
            for state in ["Deploy", "Retract", "Select", "Deselect", "Idle"] {
                let mut query = vec![template.to_owned()];
                if state != "Idle" {
                    query.push(state.to_owned());
                }
                requests.insert((state, query));
            }
        }
        let mut out = Vec::new();
        for (state, request) in requests {
            for (look, query) in [(false, &tags), (true, &query_tags)] {
                if let Some(chosen) = select(&fragments, query, &request, look) {
                    let mut chosen = chosen.clone();
                    chosen["state"] = json!(state);
                    chosen["look_layer"] = json!(look);
                    chosen["query_tags"] = json!(query);
                    chosen["query_frag_tags"] = json!(request);
                    out.push(chosen);
                }
            }
        }
        Ok(out)
    }
}

fn transition_queries(
    template: &str,
    entrances: &[&Value],
) -> BTreeSet<(&'static str, Vec<String>)> {
    entrances
        .iter()
        .flat_map(|entrance| {
            [("enter", "Enter"), ("exit", "Exit")]
                .into_iter()
                .filter_map(move |(field, state)| {
                    let step = &entrance[field];
                    step["interaction"]
                        .as_str()
                        .filter(|name| !name.is_empty())?;
                    let mut query = vec![template.to_owned(), state.to_owned()];
                    if let Some(tags) = step["fragment_tags"].as_str() {
                        query.extend(ports::words(tags));
                    }
                    Some((state, query))
                })
        })
        .collect()
}

fn select<'a>(
    fragments: &'a [Value],
    tags: &[String],
    request: &[String],
    look: bool,
) -> Option<&'a Value> {
    fragments.iter().find(|fragment| {
        let is_look = fragment["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|tag| {
                tag.as_str()
                    .is_some_and(|tag| tag.eq_ignore_ascii_case(LOOK_SCOPE))
            });
        is_look == look && motion::matches_query(fragment, SEAT_FRAGMENT, tags, request)
    })
}

#[cfg(test)]
#[path = "seat_motion_tests.rs"]
mod tests;
