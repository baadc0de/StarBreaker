//! An entity class's interaction graph, read from its record JSON (scviewer C2 fixes): which shared interactions it
//! sends and receives, which states they set, and what a light group's states light.
//!
//! The record JSON names weak-pointer targets (`_Pointer_: ptr:N`) and pointers (`_PointsTo_:ptr:N`), so a state is
//! known with its state type (two types may author a state of the same name) — the materialized DataCore value does
//! not keep that identity.
//!
//! - [`RecordGraph`]: the record's components and pointer targets.
//! - [`sendable_interactions`]: `SSharedInteractionParams` with `Sendable`, with their `LinkingTag` names.
//! - [`light_group_interactions`]: a light group's `Linkable` interactions and the light state each ends in, through
//!   the state machine (`SInteractionStateMachineParams`: transitions, then `AggregateStateModifier`s) and
//!   `SLightGroupComponentParams.lightStateOverrides`.
//! - [`state_type_rules`]: a state type's interaction transitions and linked-state tag rules (UI `state_types[]`).
//!
//! Tags are named by [`starbreaker_datacore::Database::tag_path`]. Nothing here consults a class, interaction or
//! state name: the graph is followed as authored.

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use starbreaker_datacore::database::Database;
use starbreaker_datacore::types::{CigGuid, Record};

use crate::types::{SendableInteraction, UiLinkedState, UiStateTransition};

/// One interaction a light group receives and the light state it ends in.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct LightGroupInteraction {
    /// The shared interaction's `Name` (`Emergency Override`).
    pub name: String,
    /// Its `LinkingTag`, by full name (`/Interaction/LightGroup/SetEmergency`): a linked entity's sendable interaction
    /// with the same tag delivers it. `None` when unauthored.
    pub linking_tag: Option<String>,
    /// `SLightStateOverride.lightState` of the state the interaction leads to (`Emergency`); `None` when the state
    /// machine leads to no overridden state.
    pub light_state: Option<String>,
}

/// A light group entity class's facts (`SLightGroupComponentParams`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct LightGroupFacts {
    pub interactions: Vec<LightGroupInteraction>,
}

/// The record JSON of `record` (`None` when it does not export).
pub(crate) fn record_json(db: &Database, record: &Record) -> Option<Value> {
    let bytes = starbreaker_datacore::export::to_json_compact(db, record).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The full name of the tag a JSON tag reference (`{_RecordId_: …}`) names; its id when the database does not name
/// it.
pub(crate) fn tag_name(db: &Database, reference: &Value) -> Option<String> {
    let id = reference.get("_RecordId_").and_then(Value::as_str)?;
    let path = id.parse::<CigGuid>().ok().and_then(|guid| db.tag_path(&guid)).map(str::to_string);
    Some(path.unwrap_or_else(|| id.to_string()))
}

/// A record's components and the targets of its pointers.
pub(crate) struct RecordGraph<'j> {
    components: &'j [Value],
    pointers: HashMap<&'j str, &'j Value>,
}

impl<'j> RecordGraph<'j> {
    pub(crate) fn new(record: &'j Value) -> Self {
        fn walk<'j>(value: &'j Value, out: &mut HashMap<&'j str, &'j Value>) {
            match value {
                Value::Object(map) => {
                    if let Some(id) = map.get("_Pointer_").and_then(Value::as_str) {
                        out.insert(id, value);
                    }
                    map.values().for_each(|v| walk(v, out));
                }
                Value::Array(items) => items.iter().for_each(|v| walk(v, out)),
                _ => {}
            }
        }
        let mut pointers = HashMap::new();
        walk(record, &mut pointers);
        let components = record
            .get("_RecordValue_")
            .unwrap_or(record)
            .get("Components")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        Self { components, pointers }
    }

    /// The first component of `_Type_` `type_name`.
    pub(crate) fn component(&self, type_name: &str) -> Option<&'j Value> {
        self.components.iter().find(|c| c.get("_Type_").and_then(Value::as_str) == Some(type_name))
    }

    /// The pointer id a `_PointsTo_:ptr:N` field names (`ptr:N`).
    fn pointer_id(value: Option<&Value>) -> Option<&str> {
        value?.as_str()?.strip_prefix("_PointsTo_:")
    }

    /// The `SSharedInteractionParams` of `SEntityInteractableParams.Interactable`, in authored order.
    fn shared_interactions(&self) -> &'j [Value] {
        self.component("SEntityInteractableParams")
            .and_then(|c| c.get("Interactable"))
            .and_then(|i| i.get("SharedInteractions"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The `StateTypes` of `SInteractionStateMachineParams`.
    fn state_types(&self) -> &'j [Value] {
        self.component("SInteractionStateMachineParams")
            .and_then(|c| c.get("StateTypes"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The `StateName` of the state a pointer field targets.
    fn state_name(&self, pointer: Option<&Value>) -> Option<String> {
        let target = self.pointers.get(Self::pointer_id(pointer)?)?;
        target.get("StateName").and_then(Value::as_str).map(str::to_string)
    }

    /// The `Name` of the shared interaction a pointer field targets.
    fn interaction_name(&self, pointer: Option<&Value>) -> Option<String> {
        let target = self.pointers.get(Self::pointer_id(pointer)?)?;
        target.get("Name").and_then(Value::as_str).filter(|n| !n.is_empty()).map(str::to_string)
    }
}

fn flag(value: &Value, name: &str) -> bool {
    value.get(name).and_then(Value::as_bool).unwrap_or(false)
}

/// The class's sendable shared interactions with their linking tags, in authored order.
pub(crate) fn sendable_interactions(graph: &RecordGraph, tag: &dyn Fn(&Value) -> Option<String>) -> Vec<SendableInteraction> {
    graph
        .shared_interactions()
        .iter()
        .filter(|i| flag(i, "Sendable"))
        .filter_map(|i| {
            Some(SendableInteraction {
                name: i.get("Name").and_then(Value::as_str).filter(|n| !n.is_empty())?.to_string(),
                linking_tag: i.get("LinkingTag").and_then(tag),
            })
        })
        .collect()
}

/// The states (pointer ids) the state machine is in once `start` is: `start`, then every aggregate state whose
/// `aggregateStates` are all reached, to a fixpoint.
fn aggregate_closure<'j>(graph: &RecordGraph<'j>, start: &HashSet<&'j str>) -> HashSet<&'j str> {
    let mut reached = start.clone();
    loop {
        let before = reached.len();
        for state_type in graph.state_types() {
            for modifier in state_type.get("StateModifiers").and_then(Value::as_array).into_iter().flatten() {
                for params in modifier.get("aggregateStateParams").and_then(Value::as_array).into_iter().flatten() {
                    let conditions: Vec<&str> = params
                        .get("aggregateStates")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|s| RecordGraph::pointer_id(Some(s)))
                        .collect();
                    if conditions.is_empty() || !conditions.iter().all(|c| reached.contains(c)) {
                        continue;
                    }
                    if let Some(state) = RecordGraph::pointer_id(params.get("state")) {
                        reached.insert(state);
                    }
                }
            }
        }
        if reached.len() == before {
            return reached;
        }
    }
}

/// The light group's linkable interactions, each with the light state its transitions lead to.
pub(crate) fn light_group_interactions(graph: &RecordGraph, tag: &dyn Fn(&Value) -> Option<String>) -> Vec<LightGroupInteraction> {
    let overrides: Vec<(&str, &str)> = graph
        .component("SLightGroupComponentParams")
        .and_then(|c| c.get("lightStateOverrides"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|o| {
            Some((
                RecordGraph::pointer_id(o.get("interactionState"))?,
                o.get("lightState").and_then(Value::as_str).filter(|s| !s.is_empty())?,
            ))
        })
        .collect();
    graph
        .shared_interactions()
        .iter()
        .filter(|i| flag(i, "Linkable"))
        .filter_map(|interaction| {
            let name = interaction.get("Name").and_then(Value::as_str).filter(|n| !n.is_empty())?;
            let own = interaction.get("_Pointer_").and_then(Value::as_str);
            // The states its transitions set, from any state.
            let next: HashSet<&str> = graph
                .state_types()
                .iter()
                .flat_map(|t| t.get("States").and_then(Value::as_array).into_iter().flatten())
                .flat_map(|s| s.get("Interactions").and_then(Value::as_array).into_iter().flatten())
                .filter(|t| own.is_some() && RecordGraph::pointer_id(t.get("Interaction")) == own)
                .filter_map(|t| RecordGraph::pointer_id(t.get("NextState")))
                .collect();
            let reached = aggregate_closure(graph, &next);
            let states: Vec<&str> =
                overrides.iter().filter(|(state, _)| reached.contains(state)).map(|(_, light)| *light).collect();
            if states.iter().collect::<HashSet<_>>().len() > 1 {
                log::warn!("light group interaction {name:?} leads to several light states {states:?}; the first is exported");
            }
            Some(LightGroupInteraction {
                name: name.to_string(),
                linking_tag: interaction.get("LinkingTag").and_then(tag),
                light_state: states.first().map(|s| s.to_string()),
            })
        })
        .collect()
}

/// A state type's interaction transitions (distinct `Interaction` → `NextState` pairs, authored order) and its linked
/// state rules (`SLinkedInteractionStateChangedStateModifier.onChangeStateTagStates[]`, one per tag).
pub(crate) fn state_type_rules(
    graph: &RecordGraph,
    state_type: &Value,
    tag: &dyn Fn(&Value) -> Option<String>,
) -> (Vec<UiStateTransition>, Vec<UiLinkedState>) {
    let mut transitions: Vec<UiStateTransition> = Vec::new();
    for transition in state_type
        .get("States")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|s| s.get("Interactions").and_then(Value::as_array).into_iter().flatten())
    {
        let (Some(interaction), Some(state)) =
            (graph.interaction_name(transition.get("Interaction")), graph.state_name(transition.get("NextState")))
        else {
            continue;
        };
        let pair = UiStateTransition { interaction, state };
        if !transitions.contains(&pair) {
            transitions.push(pair);
        }
    }
    let mut linked = Vec::new();
    for modifier in state_type.get("StateModifiers").and_then(Value::as_array).into_iter().flatten() {
        for rule in modifier.get("onChangeStateTagStates").and_then(Value::as_array).into_iter().flatten() {
            let Some(state) = graph.state_name(rule.get("state")) else { continue };
            for t in rule.get("stateTags").and_then(|s| s.get("tags")).and_then(Value::as_array).into_iter().flatten() {
                if let Some(tag) = tag(t) {
                    linked.push(UiLinkedState { tag, state: state.clone() });
                }
            }
        }
    }
    (transitions, linked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A synthetic light group record (the structure of `SLightGroupComponentParams` and its state machine; made-up
    /// names): `Go` sets `Ia.Go`, which aggregates to `Agg.On`, which the light-state override lights `Lit`; `Stay`
    /// has no transition; `Send` is not linkable.
    fn record() -> Value {
        json!({"_RecordValue_": {"Components": [
            {"_Type_": "SEntityInteractableParams", "Interactable": {"SharedInteractions": [
                {"_Pointer_": "ptr:1", "Name": "Go", "Linkable": true, "LinkingTag": {"_RecordId_": "00000000-0000-0000-0000-0000000000aa"}},
                {"_Pointer_": "ptr:2", "Name": "Stay", "Linkable": true},
                {"Name": "Send", "Sendable": true, "LinkingTag": {"_RecordId_": "00000000-0000-0000-0000-0000000000bb"}}
            ]}},
            {"_Type_": "SInteractionStateMachineParams", "StateTypes": [
                {"StateTypeName": "Ia", "States": [
                    {"_Pointer_": "ptr:10", "StateName": "Idle", "Interactions": [{"Interaction": "_PointsTo_:ptr:1", "NextState": "_PointsTo_:ptr:11"}]},
                    {"_Pointer_": "ptr:11", "StateName": "Go", "Interactions": [{"Interaction": "_PointsTo_:ptr:1", "NextState": "_PointsTo_:ptr:11"}]}
                ]},
                {"StateTypeName": "Agg", "States": [{"_Pointer_": "ptr:20", "StateName": "On"}, {"_Pointer_": "ptr:21", "StateName": "Off"}],
                 "StateModifiers": [
                    {"aggregateStateParams": [
                        {"aggregateStates": ["_PointsTo_:ptr:11"], "state": "_PointsTo_:ptr:20"},
                        {"aggregateStates": ["_PointsTo_:ptr:11", "_PointsTo_:ptr:10"], "state": "_PointsTo_:ptr:21"}
                    ]},
                    {"onChangeStateTagStates": [{"stateTags": {"tags": [{"_RecordId_": "00000000-0000-0000-0000-0000000000cc"}]}, "state": "_PointsTo_:ptr:21"}]}
                 ]}
            ]},
            {"_Type_": "SLightGroupComponentParams", "lightStateOverrides": [
                {"interactionState": "_PointsTo_:ptr:21", "lightState": "Dark"},
                {"interactionState": "_PointsTo_:ptr:20", "lightState": "Lit"}
            ]}
        ]}})
    }

    fn tag(value: &Value) -> Option<String> {
        value.get("_RecordId_").and_then(Value::as_str).map(|id| format!("/T/{}", &id[34..]))
    }

    #[test]
    fn a_linkable_interaction_lights_the_state_its_transition_aggregates_to() {
        let json = record();
        let graph = RecordGraph::new(&json);
        let interactions = light_group_interactions(&graph, &tag);
        assert_eq!(interactions.len(), 2, "only linkable interactions");
        assert_eq!(interactions[0].name, "Go");
        assert_eq!(interactions[0].linking_tag.as_deref(), Some("/T/aa"));
        assert_eq!(interactions[0].light_state.as_deref(), Some("Lit"), "a conjunction not met does not apply");
        assert_eq!(interactions[1].light_state, None, "no transition: no light state");
    }

    #[test]
    fn sendable_interactions_carry_their_linking_tags() {
        let json = record();
        let graph = RecordGraph::new(&json);
        let sendable = sendable_interactions(&graph, &tag);
        assert_eq!(sendable, [SendableInteraction { name: "Send".into(), linking_tag: Some("/T/bb".into()) }]);
    }

    #[test]
    fn state_type_rules_list_distinct_transitions_and_linked_tags() {
        let json = record();
        let graph = RecordGraph::new(&json);
        let types = graph.state_types();
        let (transitions, linked) = state_type_rules(&graph, &types[0], &tag);
        assert_eq!(transitions, [UiStateTransition { interaction: "Go".into(), state: "Go".into() }], "deduplicated");
        assert!(linked.is_empty());
        let (transitions, linked) = state_type_rules(&graph, &types[1], &tag);
        assert!(transitions.is_empty());
        assert_eq!(linked, [UiLinkedState { tag: "/T/cc".into(), state: "Off".into() }]);
    }
}
