//! Widget-standard style plumbing (WP-I1 fix pass 2), run around the canvas style
//! cascade of one resolve level:
//!
//! - [`apply_standard_runtime_styles`]: an expanded standard template's own runtime
//!   look — its `brandStyles[]` container for the manufacturer, else its
//!   `defaultStyles.entries` (the same rule the canvas cascade uses for a canvas) —
//!   applied to that template's instance subtree only, before the canvas cascade,
//!   so the canvas styles and the modular-kit sheets still override it. The icon
//!   widget standard's `IconElement` entry centres the glyph in its box; the button
//!   standards' entries space and place the icon and label. The template's base
//!   modular-kit sheet (`defaultStyles.sharedStyles`) applies first.
//! - [`route_styled_component_params`]: a style that sets a component host's
//!   `ParamInputN` (recorded per modifier family by `bb_brand_apply`,
//!   `STYLED_PARAMS_KEY`) feeds the expanded template's parameter of the same
//!   family on that slot, as the engine's component-property routing does. The
//!   door shelf's CRUS brand block sets the secondary buttons' boolean
//!   `ParamInput1` (`ShowIcon`): the button's own glyph shows.
//!
//! The template behind an expanded host is recorded on the host by
//! `expand_widget_standards` ([`STANDARD_TEMPLATE_KEY`]).

use crate::bb_loc::LocFetcher;
use crate::bb_scene::{BbNodeId, BbScene};

/// Raw key on an expanded host naming the standard template record it expanded from.
pub(crate) const STANDARD_TEMPLATE_KEY: &str = "_StandardTemplate_";

/// Apply each expanded standard's runtime style entries to its own instance
/// subtree (`hosts`: only these hosts; `None`: every expanded host).
pub(crate) fn apply_standard_runtime_styles(
    scene: &mut BbScene,
    hosts: Option<&[BbNodeId]>,
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
    expanded.sort();
    let mut templates: std::collections::HashMap<String, Option<serde_json::Value>> = std::collections::HashMap::new();
    for (host_id, path) in expanded {
        let Some(template) = templates.entry(path.clone()).or_insert_with(|| fetch_by_path(&path).ok()).clone() else {
            continue;
        };
        let record_value = template.get("_RecordValue_").unwrap_or(&template);
        // The standard's base sheet (`defaultStyles.sharedStyles`: its `_default` modular-kit
        // sheet, e.g. the large button's vertically centred label) first, like the
        // canvas cascade's shared pass; a brand's own kit sheet overrides it later.
        if let Some(shared_path) = record_value
            .get("defaultStyles")
            .and_then(|d| d.get("sharedStyles"))
            .and_then(|v| v.as_str())
            .filter(|p| !p.is_empty())
        {
            let shared = templates.entry(shared_path.to_string()).or_insert_with(|| fetch_by_path(shared_path).ok()).clone();
            if let Some(shared) = shared {
                let shared_value = shared.get("_RecordValue_").unwrap_or(&shared);
                if let Some(entries) = shared_value.get("entries").and_then(|e| e.as_array()) {
                    crate::bb_style_engine::apply(
                        scene,
                        &[crate::bb_style_engine::StyleSheet {
                            tier: crate::bb_style_engine::Tier::StandardModule,
                            identifier: crate::record_name::extract_record_name(shared_path),
                            fills: shared_value,
                            chrome: shared_value,
                            entries: entries.as_slice(),
                            scope: crate::bb_style_engine::SheetScope::Subtree(host_id),
                        }],
                        loc_fetcher,
                    );
                }
            }
        }
        let brand = crate::bb_brand_style::resolve_brand_identity(
            &template,
            None,
            manufacturer_id,
            class,
            crate::bb_brand_style::BrandPolicy::Default,
        );
        let (identifier, palette, entries): (String, &serde_json::Value, &[serde_json::Value]) = match brand.as_ref() {
            Some(brand) => (brand.identifier.clone(), brand.raw, brand.entries),
            None => match record_value
                .get("defaultStyles")
                .and_then(|d| d.get("entries"))
                .and_then(|e| e.as_array())
            {
                Some(entries) => ("standard-defaultStyles".to_string(), record_value, entries.as_slice()),
                None => continue,
            },
        };
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
/// and the field the resolved value lives in.
fn styled_family(synth_type: &str) -> Option<(&'static str, &'static str)> {
    match synth_type {
        "_SynthBooleanParam_" => Some(("Boolean", "resolvedBool")),
        "_SynthNumberParam_" => Some(("Number", "resolvedNumber")),
        "_SynthIntegerParam_" => Some(("Number", "resolvedInt")),
        "_SynthStringParam_" => Some(("String", "resolvedString")),
        "_SynthLocalizedParam_" => Some(("Localization", "resolvedLocKey")),
        _ => None,
    }
}

/// Feed each host's style-set parameter slots into its expanded template's
/// synthetic parameters of the same family. A slot the host canvas BINDS keeps the
/// binding (`hostBoundInputs`: run-time data stands over the stylesheet, as for a
/// bound `IsActive`). Returns the hosts whose parameters changed, sorted. Runs after
/// the style cascade and before `finalize_widget_standard_fields` (which strips the
/// host annotations).
pub(crate) fn route_styled_component_params(scene: &mut BbScene) -> Vec<BbNodeId> {
    let mut updates: Vec<(usize, &'static str, serde_json::Value, BbNodeId)> = Vec::new();
    for (index, op) in scene.operations.iter().enumerate() {
        let Some((family, field)) = op.get("_Type_").and_then(|v| v.as_str()).and_then(styled_family) else {
            continue;
        };
        if op.get("hostBoundInputs").is_some() {
            continue;
        }
        let (Some(host_id), Some(parameter)) = (
            op.get("_HostNodeId_").and_then(|v| v.as_u64()).map(|id| id as BbNodeId),
            op.get("parameter").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        let Some(styled) = scene
            .nodes
            .get(&host_id)
            .and_then(|host| host.raw.get(crate::bb_brand_apply::STYLED_PARAMS_KEY))
            .and_then(|styled| styled.get(format!("{family}:{parameter}")))
        else {
            continue;
        };
        let value = match field {
            "resolvedInt" => match styled.as_f64() {
                Some(number) => serde_json::json!(number.round() as i64),
                None => continue,
            },
            _ => styled.clone(),
        };
        if op.get(field) != Some(&value) {
            updates.push((index, field, value, host_id));
        }
    }
    let mut changed: Vec<BbNodeId> = Vec::new();
    for (index, field, value, host_id) in updates {
        if let Some(map) = scene.operations.get_mut(index).and_then(|op| op.as_object_mut()) {
            map.insert(field.to_string(), value);
            changed.push(host_id);
        }
    }
    changed.sort_unstable();
    changed.dedup();
    changed
}

#[cfg(test)]
#[path = "standard_styles_tests.rs"]
mod tests;
