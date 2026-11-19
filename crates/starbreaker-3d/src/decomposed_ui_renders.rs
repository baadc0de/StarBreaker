//! Pre-rendered UI screens of a decomposed export: one render per distinct render identity, its other states, and the
//! file stem each identity is written under.
//!
//! - [`UiRenderKey`] / [`ui_render_key`]: what a binding's image depends on (its view fields, its texture aspect, its
//!   owner's user variables, and the owner state the render pins).
//! - [`prerender_ui_bindings`]: renders every distinct default identity once, in parallel, plus the owner's other
//!   states the canvas binds (one state type away from the default: `canvas_state_paths` × `UiBinding::state_types`),
//!   and assigns each identity a file stem ([`UiRenders::stem`]).
//! - [`state_image_stem`]: the file stem of a state render.
//!
//! Stems: the first identity with a given asset name (in binding order: children, then interior placements) keeps the
//! plain name, so packages whose screens all have distinct names keep their paths; any other identity with that name
//! gets a hash of its key. Shared generated paths were first-written-wins before (contract "Generated UI").

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use rayon::prelude::*;
use starbreaker_datacore::Database;
use starbreaker_p4k::MappedP4k;

use crate::types::UiBinding;

/// Identity of a UI binding's rendered image. Two bindings with equal `UiRenderKey` produce byte-identical PNGs within
/// one export, because the render depends only on these fields; every other input (localization, ship data,
/// manufacturer, root entity) is constant per export. `f32` aspects are stored as raw bits so the key is `Hash + Eq`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct UiRenderKey {
    canvas_guid: Option<String>,
    content_canvas_guid: Option<String>,
    binding_kind: String,
    helper_name: Option<String>,
    dashboard_view_index: Option<u32>,
    dashboard_screen_slot: Option<u32>,
    screen_name_loc_key: Option<String>,
    /// Per-floor transit location: two lift-call consoles on different floors render different headings, so the loc
    /// key is part of render identity.
    transit_location_loc_key: Option<String>,
    owner_source_file: Option<String>,
    screen_aspect_bits: Option<u32>,
    texture_aspect_bits: Option<u32>,
    /// The owner's user variables, serialized (two door pads hiding different controls render differently).
    user_variables: String,
    /// The owner's state machine, serialized: every render pins each type's default state.
    state_types: String,
    /// The owner state pinned by the render (`None`: the default render).
    state: Option<(String, String)>,
}

/// Build the render-identity key for `binding` rendered in `state` (see [`UiRenderKey`]).
pub(super) fn ui_render_key(binding: &UiBinding, state: Option<(&str, &str)>) -> UiRenderKey {
    UiRenderKey {
        canvas_guid: binding.canvas_guid.clone(),
        content_canvas_guid: binding.content_canvas_guid.clone(),
        binding_kind: binding.binding_kind.clone(),
        helper_name: binding.helper_name.clone(),
        dashboard_view_index: binding.dashboard_view_index,
        dashboard_screen_slot: binding.dashboard_screen_slot,
        screen_name_loc_key: binding.screen_name_loc_key.clone(),
        transit_location_loc_key: binding.transit_location_loc_key.clone(),
        owner_source_file: binding.owner_source_file.clone(),
        screen_aspect_bits: binding.ui_screen_aspect_w_over_h.map(f32::to_bits),
        texture_aspect_bits: binding.ui_texture_aspect_w_over_h.map(f32::to_bits),
        user_variables: serde_json::to_string(&binding.user_variables).unwrap_or_default(),
        state_types: serde_json::to_string(&binding.state_types).unwrap_or_default(),
        state: state.map(|(t, s)| (t.to_string(), s.to_string())),
    }
}

/// The pre-rendered screens of one export.
#[derive(Default)]
pub(super) struct UiRenders {
    /// Every render (default and state renders) by identity.
    pub(super) renders: HashMap<UiRenderKey, Result<Vec<u8>, String>>,
    /// Per default identity: the owner states rendered for it, `(state type, state)` in state-machine order.
    pub(super) states: HashMap<UiRenderKey, Vec<(String, String)>>,
    /// Every identity's file stem (defaults and states, allocated in one namespace).
    stems: StemAllocator,
}

impl UiRenders {
    /// The file stem of default identity `key` (of `binding`): the assigned one, else the binding's asset name with the
    /// key's hash (an identity rendered outside the pre-render).
    pub(super) fn stem(&self, key: &UiRenderKey, binding: &UiBinding) -> String {
        self.stems
            .by_key
            .get(key)
            .cloned()
            .unwrap_or_else(|| format!("{}_{}", super::generated_ui_asset_name(binding), key_hash(key)))
    }

    /// The file stem of the state render `state_key` of a default stem `stem`: the allocated one, else derived.
    pub(super) fn state_stem(&self, state_key: &UiRenderKey, stem: &str, state_type: &str, state: &str) -> String {
        self.stems
            .by_key
            .get(state_key)
            .cloned()
            .unwrap_or_else(|| format!("{}_{}", state_image_stem(stem, state_type, state), key_hash(state_key)))
    }
}

/// A stable 64-bit digest of `key` (`DefaultHasher::new` is unkeyed: the same in every run).
fn key_hash(key: &UiRenderKey) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// File stems of render identities, one namespace for default and state renders: an identity takes its wanted name
/// when no other identity holds it, else the name with its key digest (and a counter in the unlikely event that is
/// held too). Two different identities never share a stem.
#[derive(Default)]
struct StemAllocator {
    by_key: HashMap<UiRenderKey, String>,
    by_stem: HashMap<String, UiRenderKey>,
}

impl StemAllocator {
    fn assign(&mut self, key: &UiRenderKey, wanted: String) -> String {
        if let Some(stem) = self.by_key.get(key) {
            return stem.clone();
        }
        let hashed = format!("{wanted}_{}", key_hash(key));
        let stem = std::iter::once(wanted)
            .chain(std::iter::once(hashed.clone()))
            .chain((2u32..).map(|n| format!("{hashed}_{n}")))
            .find(|candidate| !self.by_stem.contains_key(candidate))
            .expect("an unbounded candidate sequence");
        self.by_stem.insert(stem.clone(), key.clone());
        self.by_key.insert(key.clone(), stem.clone());
        stem
    }
}

/// The file stem of `stem`'s render in state `state` of type `state_type`.
pub(super) fn state_image_stem(stem: &str, state_type: &str, state: &str) -> String {
    format!(
        "{stem}__{}_{}",
        super::sanitize_identifier(state_type),
        super::sanitize_identifier(state)
    )
}

/// The owner states `binding`'s canvas binds, other than each type's default: the state renders it gets.
fn bound_state_variants(binding: &UiBinding, bound_paths: &std::collections::BTreeSet<String>) -> Vec<(String, String)> {
    let mut variants = Vec::new();
    for state_type in &binding.state_types {
        let prefix = format!("state.{}.", state_type.name.to_ascii_lowercase());
        if !bound_paths.iter().any(|path| path.starts_with(&prefix)) {
            continue;
        }
        for state in state_type.states.iter().filter(|s| **s != state_type.default_state) {
            variants.push((state_type.name.clone(), state.clone()));
        }
    }
    variants
}

/// Render each DISTINCT UI binding once, in parallel, with its owner's other bound states, and assign file stems.
/// Bindings that share a [`UiRenderKey`] render identically, so a representative is rendered once and every instance
/// reuses the result: the per-instance render work (hundreds of screens, mostly duplicates) collapses to one render
/// per unique screen and state.
#[allow(clippy::too_many_arguments)]
pub(super) fn prerender_ui_bindings(
    bindings: &[&UiBinding],
    db: &Database<'_>,
    p4k: &MappedP4k,
    texture_mip: u32,
    root_entity_name: &str,
    root_manufacturer_id: Option<&str>,
    loc_data: &crate::ui_pipeline::UiLocData,
    defaults_registry: &starbreaker_ui::DefaultValueRegistry,
    ship_data: &crate::ui_pipeline::UiShipData,
) -> UiRenders {
    let mut out = UiRenders::default();
    // One representative binding per unique key (first wins; any is equivalent), and stems in binding order.
    let mut representatives: Vec<(UiRenderKey, &UiBinding)> = Vec::new();
    let mut seen: HashSet<UiRenderKey> = HashSet::new();
    for binding in bindings {
        let key = ui_render_key(binding, None);
        if !seen.insert(key.clone()) {
            continue;
        }
        out.stems.assign(&key, super::generated_ui_asset_name(binding));
        representatives.push((key, binding));
    }
    // The states each representative's canvas binds (one canvas scan per canvas).
    let mut paths_by_canvas: HashMap<(Option<String>, Option<String>), std::collections::BTreeSet<String>> = HashMap::new();
    let mut jobs: Vec<(UiRenderKey, &UiBinding, Option<(String, String)>)> = Vec::new();
    for (key, binding) in &representatives {
        jobs.push((key.clone(), binding, None));
        if binding.state_types.is_empty() {
            continue;
        }
        let paths = paths_by_canvas
            .entry((binding.canvas_guid.clone(), binding.content_canvas_guid.clone()))
            .or_insert_with(|| crate::ui_pipeline::canvas_state_paths(binding, db));
        let variants = bound_state_variants(binding, paths);
        let stem = out.stems.by_key.get(key).cloned().unwrap_or_default();
        for (state_type, state) in &variants {
            let state_key = ui_render_key(binding, Some((state_type, state)));
            out.stems.assign(&state_key, state_image_stem(&stem, state_type, state));
            jobs.push((
                state_key,
                binding,
                Some((state_type.clone(), state.clone())),
            ));
        }
        if !variants.is_empty() {
            out.states.insert(key.clone(), variants);
        }
    }
    out.renders = jobs
        .par_iter()
        .map(|(key, binding, state)| {
            let result = crate::ui_pipeline::render_ui_binding_png(
                binding,
                state.as_ref().map(|(t, s)| (t.as_str(), s.as_str())),
                db,
                p4k,
                texture_mip,
                root_manufacturer_id,
                loc_data,
                ship_data,
                Some(defaults_registry),
                Some(root_entity_name),
            );
            (key.clone(), result)
        })
        .collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::UiStateType;

    /// I1-3: the state renders are the owner's non-default states of the state types the canvas binds.
    #[test]
    fn state_variants_cover_the_bound_types_non_default_states() {
        let binding = UiBinding {
            state_types: vec![
                UiStateType { name: "TypeA".into(), states: vec!["A0".into(), "A1".into(), "A2".into()], default_state: "A0".into() },
                UiStateType { name: "TypeB".into(), states: vec!["B0".into(), "B1".into()], default_state: "B1".into() },
                UiStateType { name: "Unbound".into(), states: vec!["U0".into(), "U1".into()], default_state: "U0".into() },
            ],
            ..Default::default()
        };
        let paths: std::collections::BTreeSet<String> =
            ["state.typea.a1", "state.typeb.b1"].into_iter().map(String::from).collect();
        let variants = bound_state_variants(&binding, &paths);
        let pairs: Vec<(&str, &str)> = variants.iter().map(|(t, s)| (t.as_str(), s.as_str())).collect();
        assert_eq!(pairs, [("TypeA", "A1"), ("TypeA", "A2"), ("TypeB", "B0")]);
    }

    /// Identities differ by texture aspect, user variables and state; the state stem names its state.
    #[test]
    fn render_keys_separate_aspect_variables_and_state() {
        let mut a = UiBinding { canvas_guid: Some("g".into()), ..Default::default() };
        let base = ui_render_key(&a, None);
        assert_ne!(base, ui_render_key(&a, Some(("T", "S"))));
        a.ui_texture_aspect_w_over_h = Some(0.5625);
        assert_ne!(base, ui_render_key(&a, None));
        a.user_variables.push(crate::types::UiUserVariable { name: "v".into(), value: crate::types::UiUserValue::Bool(true) });
        let with_vars = ui_render_key(&a, None);
        a.user_variables[0].value = crate::types::UiUserValue::Bool(false);
        assert_ne!(with_vars, ui_render_key(&a, None));
        assert_eq!(state_image_stem("door", "DoorOpenState", "DoorOpened"), "door__dooropenstate_dooropened");
    }

    /// Review fix: the owner's state machine is part of the identity (two pads alike but for their states render
    /// and list states differently), and default and state renders share one stem namespace: an identity whose plain
    /// name another already holds (a default named like another's state image) gets its digest, never the same file.
    #[test]
    fn identities_include_the_state_machine_and_stems_never_collide() {
        let a = UiBinding { canvas_guid: Some("g".into()), ..Default::default() };
        let mut b = a.clone();
        b.state_types.push(UiStateType { name: "T".into(), states: vec!["S0".into(), "S1".into()], default_state: "S0".into() });
        assert_ne!(ui_render_key(&a, None), ui_render_key(&b, None));
        let mut stems = StemAllocator::default();
        let door = stems.assign(&ui_render_key(&a, None), "door".into());
        let state = stems.assign(&ui_render_key(&a, Some(("T", "S1"))), state_image_stem(&door, "T", "S1"));
        let clash = stems.assign(&ui_render_key(&b, None), state.clone());
        assert_eq!(door, "door");
        assert_ne!(clash, state, "a second identity never takes a held stem");
        assert_eq!(stems.assign(&ui_render_key(&a, None), "other".into()), "door", "an identity keeps its stem");
    }
}
