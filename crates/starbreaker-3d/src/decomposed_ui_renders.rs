//! Pre-rendered UI screens of a decomposed export: one render per distinct render identity, its other states, and the
//! file stem each identity is written under.
//!
//! - [`UiRenderKey`] / [`ui_render_key`]: what a binding's image depends on (its view fields, its texture aspect, its
//!   owner's user variables, the owner state the render pins, and the button the cursor is on: [`ui_hover_key`]).
//! - [`prerender_ui_bindings`]: renders every distinct default identity once, in parallel, plus the owner's other
//!   states the canvas binds (one state type away from the default: `canvas_state_paths` × `UiBinding::state_types`)
//!   and every pair of such states of two different bound types ([`bound_state_pairs`]; pairs the canvas cannot tell
//!   apart share one render, [`render_class`]), and assigns each identity a file stem ([`UiRenders::stem`]).
//!   Then each written render (the default, the states that differ: [`written_renders`]) with the cursor on each of its
//!   buttons (the engine's hover tag on the node; [`UiRenders::hovers`]).
//! - [`state_image_stem`] / [`states_image_stem`]: the file stem of a state render (a pair's stem names both).
//!   [`hover_image_stem`]: a hover render's.
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
    /// The owner states pinned by the render (empty: the default render).
    states: Vec<(String, String)>,
    /// The button node the cursor is on (`None`: at rest).
    hovered: Option<String>,
}

/// Build the render-identity key for `binding` rendered with `states` pinned (see [`UiRenderKey`]).
pub(super) fn ui_render_key(binding: &UiBinding, states: &[(&str, &str)]) -> UiRenderKey {
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
        states: states.iter().map(|(t, s)| (t.to_string(), s.to_string())).collect(),
        hovered: None,
    }
}

/// The identity of `binding` rendered with `states` pinned and the cursor on button node `node`.
pub(super) fn ui_hover_key(binding: &UiBinding, states: &[(&str, &str)], node: &str) -> UiRenderKey {
    UiRenderKey { hovered: Some(node.to_string()), ..ui_render_key(binding, states) }
}

/// The pre-rendered screens of one export.
#[derive(Default)]
pub(super) struct UiRenders {
    /// Every render (default and state renders) by identity.
    pub(super) renders: HashMap<UiRenderKey, Result<crate::ui_pipeline::UiRender, String>>,
    /// Per default identity: the owner states rendered for it, `(state type, state)` in state-machine order.
    pub(super) states: HashMap<UiRenderKey, Vec<(String, String)>>,
    /// Per default identity: the pairs of owner states (two types) rendered for it, in state-machine order.
    pub(super) pairs: HashMap<UiRenderKey, Vec<[(String, String); 2]>>,
    /// A pair identity the canvas cannot tell from an earlier one (`render_class`) → that one: it shares its render
    /// and file.
    aliases: HashMap<UiRenderKey, UiRenderKey>,
    /// Every identity's file stem (defaults and states, allocated in one namespace).
    stems: StemAllocator,
    /// Per written render (its identity, an alias resolved): the identities of its renders with the cursor on each of
    /// its buttons, by button node.
    hovers: HashMap<UiRenderKey, Vec<(String, UiRenderKey)>>,
}

impl UiRenders {
    /// The render of identity `key` (through its alias, when it shares another's).
    pub(super) fn render(&self, key: &UiRenderKey) -> Option<&Result<crate::ui_pipeline::UiRender, String>> {
        self.renders.get(self.aliases.get(key).unwrap_or(key))
    }

    /// The file stem of default identity `key` (of `binding`): the assigned one, else the binding's asset name with the
    /// key's hash (an identity rendered outside the pre-render).
    pub(super) fn stem(&self, key: &UiRenderKey, binding: &UiBinding) -> String {
        self.stems
            .by_key
            .get(key)
            .cloned()
            .unwrap_or_else(|| format!("{}_{}", super::generated_ui_asset_name(binding), key_hash(key)))
    }

    /// The renders of identity `key` (default or state, through its alias) with the cursor on each of its buttons:
    /// `(button node, the hover render's identity)`, in button order.
    pub(super) fn hovers(&self, key: &UiRenderKey) -> &[(String, UiRenderKey)] {
        self.hovers.get(self.aliases.get(key).unwrap_or(key)).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The file stem allocated to a hover render's identity.
    pub(super) fn hover_stem(&self, hover_key: &UiRenderKey) -> Option<&str> {
        self.stems.by_key.get(hover_key).map(String::as_str)
    }

    /// The file stem of the state render `state_key` of a default stem `stem` pinning `states`: the allocated one, else
    /// derived.
    pub(super) fn state_stem(&self, state_key: &UiRenderKey, stem: &str, states: &[(String, String)]) -> String {
        self.stems
            .by_key
            .get(self.aliases.get(state_key).unwrap_or(state_key))
            .cloned()
            .unwrap_or_else(|| format!("{}_{}", states_image_stem(stem, states), key_hash(state_key)))
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

/// The file stem of `stem`'s render with every state of `states` pinned (one: [`state_image_stem`]).
pub(super) fn states_image_stem(stem: &str, states: &[(String, String)]) -> String {
    states.iter().fold(stem.to_string(), |acc, (state_type, state)| state_image_stem(&acc, state_type, state))
}

/// The file stem of a render of `stem` (a default or state stem) with the cursor on button node `node`.
pub(super) fn hover_image_stem(stem: &str, node: &str) -> String {
    format!("{stem}__hover_{}", super::sanitize_identifier(node))
}

/// The renders a binding writes beside its default (as `generated_ui_binding_record` decides): each single-state
/// render that differs from the default, then each pair render that differs from the default and from every single.
/// `(identity, its pins)` with aliases resolved, in that order, the default first.
fn written_renders(
    out: &UiRenders,
    key: &UiRenderKey,
    binding: &UiBinding,
) -> Vec<(UiRenderKey, Vec<(String, String)>)> {
    let png = |k: &UiRenderKey| out.render(k).and_then(|r| r.as_ref().ok()).map(|r| &r.png);
    let Some(default_png) = png(key) else { return Vec::new() };
    let mut written = vec![(key.clone(), Vec::new())];
    let mut singles = Vec::new();
    for state in out.states.get(key).into_iter().flatten() {
        let state_key = ui_render_key(binding, &[(state.0.as_str(), state.1.as_str())]);
        if let Some(p) = png(&state_key) {
            singles.push(p);
            if p != default_png {
                written.push((state_key, vec![state.clone()]));
            }
        }
    }
    for pair in out.pairs.get(key).into_iter().flatten() {
        let pins: Vec<(&str, &str)> = pair.iter().map(|(t, s)| (t.as_str(), s.as_str())).collect();
        let pair_key = ui_render_key(binding, &pins);
        let resolved = out.aliases.get(&pair_key).cloned().unwrap_or(pair_key);
        if written.iter().any(|(k, _)| *k == resolved) {
            continue;
        }
        if let Some(p) = png(&resolved)
            && p != default_png
            && !singles.contains(&p)
        {
            written.push((resolved, pair.to_vec()));
        }
    }
    written
}

/// The button nodes a hover render can be made for, in button order: the hover tag goes on every node of the name, so a
/// name that two distinct buttons share (two regions) would light both and is left out (logged).
pub(super) fn hoverable_nodes(buttons: &[crate::types::UiButton]) -> Vec<String> {
    let mut nodes: Vec<String> = Vec::new();
    let mut ambiguous: Vec<String> = Vec::new();
    for button in buttons {
        let distinct = buttons.iter().any(|other| other.node == button.node && other.rect_px != button.rect_px);
        if distinct {
            if !ambiguous.contains(&button.node) {
                log::warn!("[ui] button node {:?} names two regions: no hover render (it would light both)", button.node);
                ambiguous.push(button.node.clone());
            }
        } else if !nodes.contains(&button.node) {
            nodes.push(button.node.clone());
        }
    }
    nodes
}

/// The pairs of owner states the combined renders pin: one non-default state of each of two different state types the
/// canvas binds (every such pair, as the single-state renders take every such state), in state-machine order.
pub(super) fn bound_state_pairs(
    binding: &UiBinding,
    bound_paths: &std::collections::BTreeSet<String>,
) -> Vec<[(String, String); 2]> {
    let variants = bound_state_variants(binding, bound_paths);
    let mut pairs = Vec::new();
    for (i, a) in variants.iter().enumerate() {
        for b in &variants[i + 1..] {
            if a.0 != b.0 {
                pairs.push([a.clone(), b.clone()]);
            }
        }
    }
    pairs
}

/// What the canvas sees of a pinned pair: each state type with its state when the canvas reads that state's own
/// `state.<type>.<state>` path, else none (an unread state shows only as its type's default going false, the same for
/// every unread state of the type). Pairs of one class render identically, so one render serves them all.
pub(super) fn render_class(
    pair: &[(String, String); 2],
    bound_paths: &std::collections::BTreeSet<String>,
) -> [(String, Option<String>); 2] {
    pair.clone().map(|(state_type, state)| {
        let read = bound_paths.contains(&format!("state.{}.{}", state_type.to_ascii_lowercase(), state.to_ascii_lowercase()));
        (state_type, read.then_some(state))
    })
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
        let key = ui_render_key(binding, &[]);
        if !seen.insert(key.clone()) {
            continue;
        }
        out.stems.assign(&key, super::generated_ui_asset_name(binding));
        representatives.push((key, binding));
    }
    // The states each representative's canvas binds (one canvas scan per canvas).
    let mut paths_by_canvas: HashMap<(Option<String>, Option<String>), std::collections::BTreeSet<String>> = HashMap::new();
    let mut jobs: Vec<(UiRenderKey, &UiBinding, Vec<(String, String)>)> = Vec::new();
    for (key, binding) in &representatives {
        jobs.push((key.clone(), binding, Vec::new()));
        if binding.state_types.is_empty() {
            continue;
        }
        let paths = paths_by_canvas
            .entry((binding.canvas_guid.clone(), binding.content_canvas_guid.clone()))
            .or_insert_with(|| crate::ui_pipeline::canvas_state_paths(binding, db));
        let variants = bound_state_variants(binding, paths);
        let stem = out.stems.by_key.get(key).cloned().unwrap_or_default();
        for (state_type, state) in &variants {
            let state_key = ui_render_key(binding, &[(state_type, state)]);
            out.stems.assign(&state_key, state_image_stem(&stem, state_type, state));
            jobs.push((state_key, binding, vec![(state_type.clone(), state.clone())]));
        }
        if !variants.is_empty() {
            out.states.insert(key.clone(), variants);
        }
    }
    let hover_tag = crate::ui_pipeline::hover_tag_id(db);
    let render = |jobs: &[(UiRenderKey, &UiBinding, Vec<(String, String)>)]| -> Vec<_> {
        jobs.par_iter()
            .map(|(key, binding, states)| {
                let pins: Vec<(&str, &str)> = states.iter().map(|(t, s)| (t.as_str(), s.as_str())).collect();
                let hovered = key.hovered.as_deref().zip(hover_tag.as_deref());
                let result = crate::ui_pipeline::render_ui_binding(
                    binding,
                    &pins,
                    hovered,
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
            .collect()
    };
    let defaults = jobs.iter().filter(|(_, _, s)| s.is_empty()).count();
    let singles = jobs.len() - defaults;
    out.renders = render(&jobs).into_iter().collect();
    // Then every pair of states of two bound types; the pairs the canvas cannot tell apart share one render.
    let mut pair_jobs: Vec<(UiRenderKey, &UiBinding, Vec<(String, String)>)> = Vec::new();
    let mut pair_count = 0;
    for (key, binding) in &representatives {
        let Some(paths) = paths_by_canvas.get(&(binding.canvas_guid.clone(), binding.content_canvas_guid.clone())) else {
            continue;
        };
        let pairs = bound_state_pairs(binding, paths);
        let stem = out.stems.by_key.get(key).cloned().unwrap_or_default();
        let mut by_class: HashMap<[(String, Option<String>); 2], UiRenderKey> = HashMap::new();
        for pair in &pairs {
            let pins: Vec<(&str, &str)> = pair.iter().map(|(t, s)| (t.as_str(), s.as_str())).collect();
            let pair_key = ui_render_key(binding, &pins);
            match by_class.get(&render_class(pair, paths)) {
                Some(shared) => {
                    out.aliases.insert(pair_key, shared.clone());
                }
                None => {
                    by_class.insert(render_class(pair, paths), pair_key.clone());
                    out.stems.assign(&pair_key, states_image_stem(&stem, pair));
                    pair_jobs.push((pair_key, binding, pair.to_vec()));
                }
            }
        }
        pair_count += pairs.len();
        if !pairs.is_empty() {
            out.pairs.insert(key.clone(), pairs);
        }
    }
    log::info!(
        "[ui] renders: {defaults} default, {singles} single-state, {} state-pair (for {pair_count} pairs)",
        pair_jobs.len()
    );
    out.renders.extend(render(&pair_jobs));
    // Then each written render with the cursor on each of its buttons (the engine's hover tag on that node).
    if hover_tag.is_none() {
        log::warn!("[ui] the tag database has no UI.Style.__Internal.hover tag: no hover renders");
        return out;
    }
    let mut hover_jobs: Vec<(UiRenderKey, &UiBinding, Vec<(String, String)>)> = Vec::new();
    // Only the panels a player operates (the consumer shows hover there; a cockpit MFD's dozens of pips would multiply
    // the renders for nothing).
    for (key, binding) in representatives.iter().filter(|(_, b)| crate::pipeline::ui_owner_values::is_operated_panel(b)) {
        for (written_key, pins) in written_renders(&out, key, binding) {
            let Some(Ok(render)) = out.render(&written_key) else { continue };
            let stem = out.stems.by_key.get(&written_key).cloned().unwrap_or_default();
            let pin_refs: Vec<(&str, &str)> = pins.iter().map(|(t, s)| (t.as_str(), s.as_str())).collect();
            let nodes = hoverable_nodes(&render.buttons);
            let mut hovers = Vec::new();
            for node in nodes {
                let hover_key = ui_hover_key(binding, &pin_refs, &node);
                out.stems.assign(&hover_key, hover_image_stem(&stem, &node));
                hovers.push((node, hover_key.clone()));
                hover_jobs.push((hover_key, binding, pins.clone()));
            }
            out.hovers.insert(written_key, hovers);
        }
    }
    log::info!("[ui] renders: {} hover", hover_jobs.len());
    out.renders.extend(render(&hover_jobs));
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
                UiStateType { name: "TypeA".into(), states: vec!["A0".into(), "A1".into(), "A2".into()], default_state: "A0".into(), ..Default::default() },
                UiStateType { name: "TypeB".into(), states: vec!["B0".into(), "B1".into()], default_state: "B1".into(), ..Default::default() },
                UiStateType { name: "Unbound".into(), states: vec!["U0".into(), "U1".into()], default_state: "U0".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        let paths: std::collections::BTreeSet<String> =
            ["state.typea.a1", "state.typeb.b1"].into_iter().map(String::from).collect();
        let variants = bound_state_variants(&binding, &paths);
        let pairs: Vec<(&str, &str)> = variants.iter().map(|(t, s)| (t.as_str(), s.as_str())).collect();
        assert_eq!(pairs, [("TypeA", "A1"), ("TypeA", "A2"), ("TypeB", "B0")]);
    }

    /// scviewer C2 fixes review: the combined renders cover every non-default state of each of two different bound
    /// types (TypeC, its own non-default state unread, included); the pairs of states the canvas does not tell apart
    /// (unread states of one type) share one render.
    #[test]
    fn state_pairs_cover_every_non_default_state_of_two_bound_types() {
        let ty = |name: &str, states: &[&str]| UiStateType {
            name: name.into(),
            states: states.iter().map(|s| s.to_string()).collect(),
            default_state: states[0].into(),
            ..Default::default()
        };
        let binding = UiBinding {
            state_types: vec![
                ty("TypeA", &["A0", "A1", "A2", "A3"]),
                ty("TypeB", &["B0", "B1"]),
                ty("TypeC", &["C0", "C1"]),
                ty("Unbound", &["U0", "U1"]),
            ],
            ..Default::default()
        };
        // TypeA reads A1 (A2, A3 unread), TypeB reads B1, TypeC reads only its default; Unbound is not read at all.
        let paths: std::collections::BTreeSet<String> =
            ["state.typea.a1", "state.typeb.b1", "state.typec.c0"].into_iter().map(String::from).collect();
        let owned = bound_state_pairs(&binding, &paths);
        let pairs: Vec<String> = owned.iter().map(|[a, b]| format!("{}+{}", a.1, b.1)).collect();
        assert_eq!(pairs, ["A1+B1", "A1+C1", "A2+B1", "A2+C1", "A3+B1", "A3+C1", "B1+C1"]);
        let class = |p: &[(String, String); 2]| render_class(p, &paths);
        assert_eq!(class(&owned[2]), class(&owned[4]), "A2 and A3 are unread: one render");
        assert_ne!(class(&owned[0]), class(&owned[2]), "A1 is read");
        let pair = [("TypeA".to_string(), "A1".to_string()), ("TypeB".to_string(), "B1".to_string())];
        assert_eq!(states_image_stem("pad", &pair), "pad__typea_a1__typeb_b1");
        assert_ne!(ui_render_key(&binding, &[("TypeA", "A1"), ("TypeB", "B1")]), ui_render_key(&binding, &[("TypeA", "A1")]));
    }

    /// A node per button region: one node that two regions share would light both and gets no hover render.
    #[test]
    fn hoverable_nodes_skip_a_name_shared_by_two_regions() {
        let b = |node: &str, x: f32| crate::types::UiButton { node: node.into(), interaction: "I".into(), rect_px: [x, 0.0, 10.0, 10.0], label: None };
        let buttons = [b("Door", 0.0), b("Door", 0.0), b("Pip", 0.0), b("Pip", 20.0), b("Lock", 40.0)];
        assert_eq!(hoverable_nodes(&buttons), ["Door", "Lock"]);
    }

    /// The cursor's button is part of the identity, and a hover render's stem names its button after its image's stem.
    #[test]
    fn hover_identities_name_their_button() {
        let a = UiBinding { canvas_guid: Some("g".into()), ..Default::default() };
        assert_ne!(ui_render_key(&a, &[]), ui_hover_key(&a, &[], "DoorButton"));
        assert_ne!(ui_hover_key(&a, &[], "DoorButton"), ui_hover_key(&a, &[], "LockButton"));
        assert_ne!(ui_hover_key(&a, &[], "DoorButton"), ui_hover_key(&a, &[("T", "S")], "DoorButton"));
        assert_eq!(hover_image_stem("door__t_s", "DoorButton"), "door__t_s__hover_doorbutton");
    }

    /// The renders a binding writes beside its default, which get hover renders: the single states that differ from the
    /// default, and the pairs that differ from the default and from every single.
    #[test]
    fn written_renders_skip_states_that_look_like_the_default_or_a_single() {
        let ty = |name: &str, states: &[&str]| UiStateType {
            name: name.into(),
            states: states.iter().map(|s| s.to_string()).collect(),
            default_state: states[0].into(),
            ..Default::default()
        };
        let binding = UiBinding { canvas_guid: Some("g".into()), state_types: vec![ty("A", &["A0", "A1", "A2"]), ty("B", &["B0", "B1"])], ..Default::default() };
        let render = |png: u8| Ok(crate::ui_pipeline::UiRender { png: vec![png], size_px: [1, 1], buttons: Vec::new() });
        let key = ui_render_key(&binding, &[]);
        let mut out = UiRenders::default();
        out.renders.insert(key.clone(), render(0));
        out.renders.insert(ui_render_key(&binding, &[("A", "A1")]), render(1));
        out.renders.insert(ui_render_key(&binding, &[("A", "A2")]), render(0)); // looks like the default
        out.renders.insert(ui_render_key(&binding, &[("B", "B1")]), render(2));
        out.renders.insert(ui_render_key(&binding, &[("A", "A1"), ("B", "B1")]), render(3));
        out.renders.insert(ui_render_key(&binding, &[("A", "A2"), ("B", "B1")]), render(2)); // looks like B1 alone
        let pin = |t: &str, s: &str| (t.to_string(), s.to_string());
        out.states.insert(key.clone(), vec![pin("A", "A1"), pin("A", "A2"), pin("B", "B1")]);
        out.pairs.insert(key.clone(), vec![[pin("A", "A1"), pin("B", "B1")], [pin("A", "A2"), pin("B", "B1")]]);
        let written: Vec<Vec<(String, String)>> = written_renders(&out, &key, &binding).into_iter().map(|(_, pins)| pins).collect();
        assert_eq!(written, [vec![], vec![pin("A", "A1")], vec![pin("B", "B1")], vec![pin("A", "A1"), pin("B", "B1")]]);
    }

    /// Identities differ by texture aspect, user variables and state; the state stem names its state.
    #[test]
    fn render_keys_separate_aspect_variables_and_state() {
        let mut a = UiBinding { canvas_guid: Some("g".into()), ..Default::default() };
        let base = ui_render_key(&a, &[]);
        assert_ne!(base, ui_render_key(&a, &[("T", "S")]));
        a.ui_texture_aspect_w_over_h = Some(0.5625);
        assert_ne!(base, ui_render_key(&a, &[]));
        a.user_variables.push(crate::types::UiUserVariable { name: "v".into(), value: crate::types::UiUserValue::Bool(true) });
        let with_vars = ui_render_key(&a, &[]);
        a.user_variables[0].value = crate::types::UiUserValue::Bool(false);
        assert_ne!(with_vars, ui_render_key(&a, &[]));
        assert_eq!(state_image_stem("door", "DoorOpenState", "DoorOpened"), "door__dooropenstate_dooropened");
    }

    /// Review fix: the owner's state machine is part of the identity (two pads alike but for their states render
    /// and list states differently), and default and state renders share one stem namespace: an identity whose plain
    /// name another already holds (a default named like another's state image) gets its digest, never the same file.
    #[test]
    fn identities_include_the_state_machine_and_stems_never_collide() {
        let a = UiBinding { canvas_guid: Some("g".into()), ..Default::default() };
        let mut b = a.clone();
        b.state_types.push(UiStateType { name: "T".into(), states: vec!["S0".into(), "S1".into()], default_state: "S0".into(), ..Default::default() });
        assert_ne!(ui_render_key(&a, &[]), ui_render_key(&b, &[]));
        let mut stems = StemAllocator::default();
        let door = stems.assign(&ui_render_key(&a, &[]), "door".into());
        let state = stems.assign(&ui_render_key(&a, &[("T", "S1")]), state_image_stem(&door, "T", "S1"));
        let clash = stems.assign(&ui_render_key(&b, &[]), state.clone());
        assert_eq!(door, "door");
        assert_ne!(clash, state, "a second identity never takes a held stem");
        assert_eq!(stems.assign(&ui_render_key(&a, &[]), "other".into()), "door", "an identity keeps its stem");
    }
}
