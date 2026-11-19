//! Widget-standard style plumbing (WP-I1 fix pass 2), run around the canvas style
//! cascade of one resolve level:
//!
//! - [`apply_standard_runtime_styles`]: an expanded standard template's own runtime
//!   look, applied to that template's instance subtree only, when the instance is
//!   created (the lowest tier: the canvas styles and the modular-kit sheets still
//!   override it). The look is the selected container's kit sheet (`sharedStyles`
//!   of the template's `brandStyles[]` container for the canvas brand / manufacturer when it links one,
//!   else of its `defaultStyles`: e.g. the large button's `_default` sheet centring
//!   its label), then that container's entries (the canvas rule: the brand
//!   container, else `defaultStyles.entries`). The icon widget standard's
//!   `IconElement` entry centres the glyph in its box.
//! - [`route_styled_component_params`]: a style that sets a component host's
//!   `ParamInputN` (recorded per modifier family by `bb_brand_apply`,
//!   `STYLED_PARAMS_KEY`) feeds the expanded template's parameter of exactly that
//!   family on that slot, as the engine's component-property routing does. The
//!   door shelf's CRUS brand block sets the secondary buttons' boolean
//!   `ParamInput1` (`ShowIcon`): the button's own glyph shows. A declared template
//!   parameter the expansion did not inject gets its synthetic here.
//!
//! The template behind an expanded host is recorded on the host by
//! `expand_widget_standards` ([`STANDARD_TEMPLATE_KEY`]).

use crate::bb_loc::LocFetcher;
use crate::bb_scene::{BbNodeId, BbScene};

/// Raw key on an expanded host naming the standard template record it expanded from.
pub(crate) const STANDARD_TEMPLATE_KEY: &str = "_StandardTemplate_";

/// Apply each expanded standard's runtime style entries to its own instance
/// subtree. `hosts`: the hosts expanded at this level (a host expanded by a child
/// canvas already has its look, possibly overridden by that child's styles);
/// `None`: every expanded host. `preferred_brand`: the canvas's style link, which
/// selects a brand container before the manufacturer does (as for a canvas).
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_standard_runtime_styles(
    scene: &mut BbScene,
    hosts: Option<&[BbNodeId]>,
    preferred_brand: Option<&str>,
    manufacturer_id: Option<&str>,
    class: crate::bb_brand_style::BrandClass,
    fetch_by_path: &dyn Fn(&str) -> Result<serde_json::Value, String>,
    loc_fetcher: Option<&dyn LocFetcher>,
) {
    let mut expanded: Vec<(BbNodeId, String)> = scene
        .nodes
        .iter()
        .filter(|(id, _)| hosts.is_none_or(|hosts| hosts.contains(id)))
        .filter_map(|(id, node)| Some((*id, node.raw.get(STANDARD_TEMPLATE_KEY)?.as_str()?.to_owned())))
        .collect();
    if expanded.is_empty() {
        return;
    }
    expanded.sort();
    let mut templates: std::collections::HashMap<String, Option<serde_json::Value>> = std::collections::HashMap::new();
    let mut sheets: std::collections::HashMap<String, Option<serde_json::Value>> = std::collections::HashMap::new();
    for (host_id, path) in expanded {
        let Some(template) = templates.entry(path.clone()).or_insert_with(|| fetch_by_path(&path).ok()).as_ref() else {
            continue;
        };
        let record_value = template.get("_RecordValue_").unwrap_or(template);
        let brand = crate::bb_brand_style::resolve_brand_identity(
            template,
            preferred_brand,
            manufacturer_id,
            class,
            crate::bb_brand_style::BrandPolicy::Default,
        );
        // The selected container: its kit-sheet link and its entries.
        let default_styles = record_value.get("defaultStyles");
        let (identifier, palette, shared_link, entries): (String, &serde_json::Value, Option<&str>, &[serde_json::Value]) =
            match brand.as_ref() {
                Some(brand) => (
                    brand.identifier.clone(),
                    brand.raw,
                    brand
                        .raw
                        .get("sharedStyles")
                        .and_then(|v| v.as_str())
                        .filter(|p| !p.is_empty())
                        .or_else(|| default_styles.and_then(|d| d.get("sharedStyles")).and_then(|v| v.as_str())),
                    brand.entries,
                ),
                None => (
                    "standard-defaultStyles".to_string(),
                    record_value,
                    default_styles.and_then(|d| d.get("sharedStyles")).and_then(|v| v.as_str()),
                    default_styles.and_then(|d| d.get("entries")).and_then(|e| e.as_array()).map_or(&[], |e| e.as_slice()),
                ),
            };
        if let Some(shared_path) = shared_link.filter(|p| !p.is_empty())
            && let Some(shared) = sheets.entry(shared_path.to_string()).or_insert_with(|| fetch_by_path(shared_path).ok()).as_ref()
        {
            let shared_value = shared.get("_RecordValue_").unwrap_or(shared);
            if let Some(shared_entries) = shared_value.get("entries").and_then(|e| e.as_array()) {
                crate::bb_style_engine::apply(
                    scene,
                    &[crate::bb_style_engine::StyleSheet {
                        tier: crate::bb_style_engine::Tier::StandardModule,
                        identifier: crate::record_name::extract_record_name(shared_path),
                        fills: shared_value,
                        chrome: shared_value,
                        entries: shared_entries.as_slice(),
                        scope: crate::bb_style_engine::SheetScope::Subtree(host_id),
                    }],
                    loc_fetcher,
                );
            }
        }
        if entries.is_empty() {
            continue;
        }
        crate::bb_style_engine::apply(
            scene,
            &[crate::bb_style_engine::StyleSheet {
                tier: crate::bb_style_engine::Tier::StandardModule,
                identifier,
                fills: palette,
                chrome: palette,
                entries,
                scope: crate::bb_style_engine::SheetScope::Subtree(host_id),
            }],
            loc_fetcher,
        );
    }
}

/// The modifier family whose style values feed a synthetic parameter of this type,
/// and the field the resolved value lives in. Each type takes only its own family:
/// a slot number is shared by parameters of different types (the secondary button's
/// Number `Icon Scale` and Integer `IconPosition` are both `ParamInput4`).
fn styled_family(synth_type: &str) -> Option<(&'static str, &'static str)> {
    match synth_type {
        "_SynthBooleanParam_" => Some(("Boolean", "resolvedBool")),
        "_SynthNumberParam_" => Some(("Number", "resolvedNumber")),
        "_SynthIntegerParam_" => Some(("Integer", "resolvedInt")),
        "_SynthStringParam_" => Some(("String", "resolvedString")),
        "_SynthLocalizedParam_" => Some(("Localization", "resolvedLocKey")),
        _ => None,
    }
}

/// The synthetic type standing for a declared template parameter of this type.
fn synth_type_for_parameter(parameter_type: &str) -> Option<&'static str> {
    match parameter_type {
        "BuildingBlocks_BindingsBooleanComponentParameter" => Some("_SynthBooleanParam_"),
        "BuildingBlocks_BindingsNumberComponentParameter" => Some("_SynthNumberParam_"),
        "BuildingBlocks_BindingsIntegerComponentParameter" => Some("_SynthIntegerParam_"),
        "BuildingBlocks_BindingsStringComponentParameter" => Some("_SynthStringParam_"),
        "BuildingBlocks_BindingsLocalizedComponentParameter" => Some("_SynthLocalizedParam_"),
        _ => None,
    }
}

/// A styled value converted to the synthetic's field type (`None`: not convertible).
fn styled_value(field: &str, styled: &serde_json::Value) -> Option<serde_json::Value> {
    match field {
        "resolvedBool" => styled.as_bool().map(serde_json::Value::Bool),
        "resolvedNumber" => styled.as_f64().map(|n| serde_json::json!(n)),
        "resolvedInt" => styled.as_f64().map(|n| serde_json::json!(n.round() as i64)),
        _ => styled.as_str().map(|s| serde_json::Value::String(s.to_owned())),
    }
}

/// Feed each host's style-set parameter slots into its expanded template's
/// parameters of the same family. A slot the host canvas BINDS keeps the binding
/// (`hostBoundInputs`: run-time data stands over the stylesheet, as for a bound
/// `IsActive`). A declared parameter without a synthetic (the expansion injects only
/// the host properties it maps) gets one. Returns the hosts whose parameters
/// changed, sorted. Runs while the expansion's host annotations exist (before
/// `finalize_widget_standard_fields` strips them).
pub(crate) fn route_styled_component_params(scene: &mut BbScene) -> Vec<BbNodeId> {
    let styled_of = |scene: &BbScene, host_id: BbNodeId, family: &str, parameter: &str| -> Option<serde_json::Value> {
        scene
            .nodes
            .get(&host_id)?
            .raw
            .get(crate::bb_brand_apply::STYLED_PARAMS_KEY)?
            .get(format!("{family}:{parameter}"))
            .cloned()
    };
    let synth_pointers: std::collections::HashSet<String> = scene
        .operations
        .iter()
        .filter(|op| op.get("_Type_").and_then(|v| v.as_str()).and_then(styled_family).is_some())
        .filter_map(|op| op.get("_Pointer_").and_then(|v| v.as_str()).map(str::to_owned))
        .collect();
    let mut updates: Vec<(usize, &'static str, serde_json::Value, BbNodeId)> = Vec::new();
    let mut created: Vec<(serde_json::Value, BbNodeId)> = Vec::new();
    for (index, op) in scene.operations.iter().enumerate() {
        let Some(op_type) = op.get("_Type_").and_then(|v| v.as_str()) else { continue };
        let (Some(host_id), Some(parameter)) = (
            op.get("_HostNodeId_").and_then(|v| v.as_u64()).map(|id| id as BbNodeId),
            op.get("parameter").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        if let Some((family, field)) = styled_family(op_type) {
            if op.get("hostBoundInputs").is_some() {
                continue;
            }
            let Some(value) = styled_of(scene, host_id, family, parameter).and_then(|s| styled_value(field, &s)) else {
                continue;
            };
            if op.get(field) != Some(&value) {
                updates.push((index, field, value, host_id));
            }
        } else if let Some(synth_type) = synth_type_for_parameter(op_type)
            && let Some(pointer) = op.get("_Pointer_").and_then(|v| v.as_str())
            && !synth_pointers.contains(pointer)
            && let Some((family, field)) = styled_family(synth_type)
            && let Some(value) = styled_of(scene, host_id, family, parameter).and_then(|s| styled_value(field, &s))
        {
            created.push((
                serde_json::json!({
                    "_Type_": synth_type,
                    "_Pointer_": pointer,
                    "_HostNodeId_": host_id,
                    "parameter": parameter,
                    field: value,
                }),
                host_id,
            ));
        }
    }
    let mut changed: Vec<BbNodeId> = Vec::new();
    for (index, field, value, host_id) in updates {
        if let Some(map) = scene.operations.get_mut(index).and_then(|op| op.as_object_mut()) {
            map.insert(field.to_string(), value);
            changed.push(host_id);
        }
    }
    for (op, host_id) in created {
        scene.operations.push(op);
        changed.push(host_id);
    }
    changed.sort_unstable();
    changed.dedup();
    changed
}

/// The hosts a widget-standard expansion has run on (\`STANDARD_TEMPLATE_KEY\`).
pub(crate) fn expanded_standard_hosts(scene: &BbScene) -> impl Iterator<Item = BbNodeId> + '_ {
    scene.nodes.iter().filter(|(_, n)| n.raw.get(STANDARD_TEMPLATE_KEY).is_some()).map(|(id, _)| *id)
}

/// Whether \`ancestor\` is \`node\` or one of its ancestors.
pub(crate) fn is_ancestor(scene: &BbScene, ancestor: BbNodeId, node: BbNodeId) -> bool {
    let mut current = Some(node);
    while let Some(id) = current {
        if id == ancestor {
            return true;
        }
        current = scene.nodes.get(&id).and_then(|n| n.parent);
    }
    false
}

/// Whether a canvas's own style containers (\`brandStyles[]\`, \`embeddedStyles\`,
/// \`defaultStyles\`, its style link) set a component parameter slot (\`ParamInputN\`).
pub(crate) fn canvas_styles_set_param_slots(record_value: &serde_json::Value, style_link: Option<&serde_json::Value>) -> bool {
    fn sets_slot(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(map) => {
                map.get("field").and_then(|f| f.as_str()).is_some_and(|f| f.starts_with("ParamInput")) || map.values().any(sets_slot)
            }
            serde_json::Value::Array(items) => items.iter().any(sets_slot),
            _ => false,
        }
    }
    ["brandStyles", "embeddedStyles", "defaultStyles"]
        .iter()
        .filter_map(|key| record_value.get(*key))
        .chain(style_link)
        .any(sets_slot)
}

/// Copy the style-set parameter slots a probe cascade recorded onto the same nodes.
pub(crate) fn copy_styled_params(probe: &BbScene, scene: &mut BbScene) {
    for (id, node) in &probe.nodes {
        if let Some(styled) = node.raw.get(crate::bb_brand_apply::STYLED_PARAMS_KEY)
            && let Some(raw) = scene.nodes.get_mut(id).and_then(|n| n.raw.as_object_mut())
        {
            raw.insert(crate::bb_brand_apply::STYLED_PARAMS_KEY.to_string(), styled.clone());
        }
    }
}

#[cfg(test)]
#[path = "standard_styles_tests.rs"]
mod tests;
