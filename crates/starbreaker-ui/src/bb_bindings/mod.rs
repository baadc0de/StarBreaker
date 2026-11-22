//! Binding resolver — maps text widget nodes to runtime binding paths.

use std::collections::HashMap;

use crate::bb_scene::BbNodeId;

mod build;
mod eval;
mod eval_bool;
mod eval_numeric;
mod eval_string;
mod param_overrides;
mod resolve_text;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_numeric_ops;
#[cfg(test)]
mod tests_hover;
#[cfg(test)]
mod tests_state_tags;
mod util;

pub(crate) use util::apply_case_modifier;

/// Resolves text content for `WidgetTextField` and `WidgetText` nodes from operations.
pub struct BindingResolver {
    pub(super) widget_to_path: HashMap<BbNodeId, String>,
    pub(super) widget_to_loc_key: HashMap<BbNodeId, String>,
    pub(super) widget_to_input_ptrs: HashMap<BbNodeId, Vec<BbNodeId>>,
    pub(super) widget_field_to_input_ptrs: HashMap<(BbNodeId, String), Vec<BbNodeId>>,
    pub(super) field_name_to_input_ptrs: HashMap<String, Vec<BbNodeId>>,
    pub(super) ptr_to_op: HashMap<BbNodeId, serde_json::Value>,
    pub(super) ptr_to_path: HashMap<BbNodeId, String>,
    pub(super) widget_to_string: HashMap<BbNodeId, String>,
    /// Authored `staticVariables[]` values (lower-cased name → raw value),
    /// carried through merges as `_SynthStaticVariable_` ops. Consulted by
    /// `Bindings*Variable` evaluation after the defaults registry.
    pub(super) static_variable_values: HashMap<String, serde_json::Value>,
}

/// Outcome of [`BindingResolver::resolve_text_detailed`].
pub struct ResolvedText {
    pub text: String,
    pub is_name_derived: bool,
}

/// Resolve `BindingsStringField` state-tag bindings (`PrimaryStateTag` …
/// `QuinaryStateTag`) onto each node's `style_tag_uuids`.
///
/// A node's authored `styleTags` are its at-rest set; the runtime state tags
/// (e.g. the footer's selected-screen `Tag fef243b7`, produced by a
/// `TagFromBoolean` over `bindingid == selectedmfd`) are computed from the scene
/// operations and appended here. Run before style application so style entries
/// gated on those tags (selected/unselected name colours, the footer border
/// colours) match correctly. Tags already present are not duplicated. The
/// registry's hovered node ([`DefaultValueRegistry::hovered`]) gets the hover tag.
///
/// [`DefaultValueRegistry::hovered`]: crate::defaults::DefaultValueRegistry::hovered
pub fn resolve_state_tags_into_scene(
    scene: &mut crate::bb_scene::BbScene,
    defaults: &crate::defaults::DefaultValueRegistry,
) {
    const STATE_TAG_FIELDS: &[&str] = &[
        "PrimaryStateTag",
        "SecondaryStateTag",
        "TertiaryStateTag",
        "QuarternaryStateTag",
        "QuinaryStateTag",
    ];
    let resolver = BindingResolver::from_operations(&scene.operations);
    let node_ids: Vec<BbNodeId> = scene.nodes.keys().copied().collect();
    for node_id in node_ids {
        for field in STATE_TAG_FIELDS {
            let Some(tag_ref) = resolver.resolve_field_text(node_id, field, defaults) else {
                continue;
            };
            let Some(uuid) = state_tag_uuid_from_reference(&tag_ref) else {
                continue;
            };
            if let Some(node) = scene.nodes.get_mut(&node_id) {
                if !node.style_tag_uuids.iter().any(|t| t.eq_ignore_ascii_case(&uuid)) {
                    node.style_tag_uuids.push(uuid);
                }
            }
        }
    }
    // The engine's hover tag on the widget under the cursor (a render made for the player's aim): applied with the
    // state tags so the standards' runtime styles, which run next, see it.
    if let Some((hovered, tag)) = defaults.hovered() {
        for node in scene.nodes.values_mut().filter(|n| n.name == hovered) {            if !node.style_tag_uuids.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                node.style_tag_uuids.push(tag.to_string());
            }
        }
    }
}

/// Re-resolve the state-tag bindings of every node under `roots` after a
/// component parameter changed (a style-set `ShowIcon` turns the instance's
/// `hide-icon` into `show-icon`). Per node, the tags its resolved state fields can
/// produce (their inputs' `TagFromIntegerSwitch` values, `TagFromBoolean` isTrue /
/// isFalse) are rebuilt from what all its fields resolve to now: a candidate no
/// field resolves to is dropped unless the node authors it, and an unresolvable
/// field changes nothing.
pub fn refresh_state_tags_in_subtrees(
    scene: &mut crate::bb_scene::BbScene,
    roots: &[BbNodeId],
    defaults: &crate::defaults::DefaultValueRegistry,
) {
    const STATE_TAG_FIELDS: &[&str] =
        &["PrimaryStateTag", "SecondaryStateTag", "TertiaryStateTag", "QuarternaryStateTag", "QuinaryStateTag"];
    let resolver = BindingResolver::from_operations(&scene.operations);
    let mut stack: Vec<BbNodeId> = roots.to_vec();
    let mut seen = std::collections::HashSet::new();
    while let Some(node_id) = stack.pop() {
        if !seen.insert(node_id) {
            continue;
        }
        let Some(node) = scene.nodes.get(&node_id) else { continue };
        stack.extend(node.children.iter().copied());
        let authored: std::collections::HashSet<String> = node
            .raw
            .get("styleTags")
            .and_then(|t| t.as_array())
            .into_iter()
            .flatten()
            .filter_map(|t| t.get("_RecordId_").and_then(|v| v.as_str()))
            .map(str::to_ascii_lowercase)
            .collect();
        let mut candidates = std::collections::HashSet::new();
        let mut resolved = Vec::new();
        for field in STATE_TAG_FIELDS {
            let Some(input_ptrs) = resolver.widget_field_to_input_ptrs.get(&(node_id, field.to_string())) else {
                continue;
            };
            let Some(uuid) = resolver
                .resolve_field_text(node_id, field, defaults)
                .and_then(|reference| state_tag_uuid_from_reference(&reference))
            else {
                continue;
            };
            for op in input_ptrs.iter().filter_map(|ptr| resolver.ptr_to_op.get(ptr)) {
                let pairs = op.get("values").and_then(|v| v.as_array()).into_iter().flatten().filter_map(|p| p.get("second"));
                for tag in pairs.chain([op.get("isTrue"), op.get("isFalse")].into_iter().flatten()) {
                    if let Some(id) = tag.get("_RecordId_").and_then(|v| v.as_str()) {
                        candidates.insert(id.to_ascii_lowercase());
                    }
                }
            }
            resolved.push(uuid);
        }
        if resolved.is_empty() {
            continue;
        }
        if let Some(node) = scene.nodes.get_mut(&node_id) {
            node.style_tag_uuids.retain(|tag| {
                let tag = tag.to_ascii_lowercase();
                !candidates.contains(&tag) || authored.contains(&tag) || resolved.contains(&tag)
            });
            for uuid in resolved {
                if !node.style_tag_uuids.iter().any(|t| t.eq_ignore_ascii_case(&uuid)) {
                    node.style_tag_uuids.push(uuid);
                }
            }
        }
    }
}

/// The PENDING state tags of a scene: every tag a state-tag binding COULD
/// produce whose producing chain is UNRESOLVABLE right now (an unwired
/// parent-injected parameter — the annunciator chiclet severities at the
/// chiclet canvas's own resolve time). Style entries referencing a pending
/// tag (positively or via `NotTag`) must not be evaluated yet: the tag's
/// truth is unknown, and treating it as absent lit the offline chiclet's
/// "Show Glow in online state" gradient. The candidate tags come from the
/// binding's direct input op (`TagFromIntegerSwitch` values,
/// `TagFromBoolean` isTrue/isFalse).
pub fn pending_state_tags(
    scene: &crate::bb_scene::BbScene,
    defaults: &crate::defaults::DefaultValueRegistry,
) -> std::collections::HashSet<String> {
    const STATE_TAG_FIELDS: &[&str] = &[
        "PrimaryStateTag",
        "SecondaryStateTag",
        "TertiaryStateTag",
        "QuarternaryStateTag",
        "QuinaryStateTag",
    ];
    let resolver = BindingResolver::from_operations(&scene.operations);
    let mut pending = std::collections::HashSet::new();
    for &node_id in scene.nodes.keys() {
        for field in STATE_TAG_FIELDS {
            let Some(input_ptrs) = resolver
                .widget_field_to_input_ptrs
                .get(&(node_id, field.to_string()))
            else {
                continue;
            };
            if resolver.resolve_field_text(node_id, field, defaults).is_some() {
                continue;
            }
            for input_ptr in input_ptrs {
                let Some(op) = resolver.ptr_to_op.get(input_ptr) else {
                    continue;
                };
                let mut collect = |tag: Option<&serde_json::Value>| {
                    if let Some(uuid) = tag
                        .and_then(|t| t.get("_RecordId_"))
                        .and_then(|v| v.as_str())
                    {
                        pending.insert(uuid.to_ascii_lowercase());
                    }
                };
                if let Some(values) = op.get("values").and_then(|v| v.as_array()) {
                    for pair in values {
                        collect(pair.get("second"));
                    }
                }
                collect(op.get("isTrue"));
                collect(op.get("isFalse"));
            }
        }
    }
    pending
}

/// Extract a lower-cased UUID from a tag reference (`"Tag.<uuid>"`, a bare
/// `_RecordId_` UUID, or a record path ending in the UUID). Returns `None` for
/// anything that is not a canonical 8-4-4-4-12 UUID.
fn state_tag_uuid_from_reference(reference: &str) -> Option<String> {
    let candidate = reference.trim().rsplit('.').next().unwrap_or("").trim();
    let is_uuid = candidate.len() == 36
        && candidate.chars().enumerate().all(|(i, ch)| match i {
            8 | 13 | 18 | 23 => ch == '-',
            _ => ch.is_ascii_hexdigit(),
        });
    is_uuid.then(|| candidate.to_ascii_lowercase())
}

/// Apply registry-resolvable `SizeX`/`SizeY` number-field bindings to node
/// sizing, preserving each dimension's authored behaviour (`Percent` stays a
/// parent fraction, `Fixed` stays canvas units). Authored sizing values on
/// bound widgets are editor placeholders — the power pip template authors
/// width 0.5 / height 0.0667 but binds both to the live pip data (full width,
/// `1/MaxPipList` height). Unresolvable bindings leave the authored value.
pub fn resolve_geometry_fields_into_scene(
    scene: &mut crate::bb_scene::BbScene,
    defaults: &crate::defaults::DefaultValueRegistry,
) {
    use crate::bb_scene::BbValue;

    let resolver = BindingResolver::from_operations(&scene.operations);
    let probe = std::env::var("SB_UI_GEOM_PROBE").as_deref() == Ok("1");
    let node_ids: Vec<BbNodeId> = scene.nodes.keys().copied().collect();
    for node_id in node_ids {
        for (field, horizontal) in [("SizeX", true), ("SizeY", false)] {
            if probe
                && let Some(node) = scene.nodes.get(&node_id)
                && let Some(ptrs) = resolver
                    .widget_field_to_input_ptrs
                    .get(&(node_id, field.to_string()))
            {
                let typed: Vec<String> = ptrs
                    .iter()
                    .map(|p| {
                        let ty = resolver
                            .ptr_to_op
                            .get(p)
                            .and_then(|op| op.get("_Type_"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("<none>");
                        format!("ptr:{p}:{ty}")
                    })
                    .collect();
                eprintln!(
                    "geom-probe: node={} '{}' field={} inputs={:?} value={:?}",
                    node_id,
                    node.name,
                    field,
                    typed,
                    resolver.resolve_field_number(node_id, field, defaults),
                );
            }
            let Some(value) = resolver.resolve_field_number(node_id, field, defaults) else {
                continue;
            };
            // A non-positive size is normally a half-resolved chain (an unwired
            // component parameter's default 0, a guarded divide-by-zero): keep
            // the authored sizing rather than collapsing the widget (the
            // widget-standard expansion icons size from ParamInputs; the
            // power-pip from `1/MaxPipList`). The exception is a size driven by
            // a live engine `Variable` (the velocity ball's `SizeY =
            // |linearvelocity/ratio/z|/2`): there a `0` is the genuine at-rest
            // collapse, so the vector bar shrinks to the centre as it does
            // in-engine.
            let engine_driven =
                resolver.field_value_source_is_engine_variable(node_id, field, defaults);
            if !value.is_finite() || (value <= 0.0 && !engine_driven) {
                continue;
            }
            let Some(node) = scene.nodes.get_mut(&node_id) else {
                continue;
            };
            let slot = if horizontal {
                &mut node.sizing.width
            } else {
                &mut node.sizing.height
            };
            *slot = match &*slot {
                BbValue::Fixed(_) => BbValue::Fixed(value as f32),
                BbValue::Percent(_) => BbValue::Percent(value as f32),
                BbValue::Other { behavior, .. } => BbValue::Other {
                    value: value as f32,
                    behavior: behavior.clone(),
                },
            };
        }
        // Bound anchors ride live data in-engine (the heat gauge marker's
        // `AnchorY = 1 - currentTemp/maxTemp`); the authored anchor is an
        // editor rest pose. Zero is a legitimate anchor, so unlike sizes only
        // non-finite results keep the authored value.
        for (field, horizontal) in [("AnchorX", true), ("AnchorY", false)] {
            let Some(value) = resolver.resolve_field_number(node_id, field, defaults) else {
                continue;
            };
            if !value.is_finite() {
                continue;
            }
            let Some(node) = scene.nodes.get_mut(&node_id) else {
                continue;
            };
            if horizontal {
                node.anchor.x = value as f32;
            } else {
                node.anchor.y = value as f32;
            }
        }
    }
}
