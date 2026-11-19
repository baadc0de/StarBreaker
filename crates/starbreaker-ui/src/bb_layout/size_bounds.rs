//! A node's size bounds (WP-I1 fix pass 2): the authored `sizing.minWidth` /
//! `maxWidth` / `minHeight` / `maxHeight` values with their `enable*` flags, and the
//! style fields that override them (`MinSizeX` / `MaxSizeX` / `MinSizeY` / `MaxSizeY`
//! values in the authored behaviour, `EnableMinWidth` / `EnableMaxWidth` /
//! `EnableMinHeight` / `EnableMaxHeight` flags). [`clamp_to_size_bounds`] clamps a
//! resolved extent; `resolve_value_for_node` applies it to every size it resolves.
//! The button standards rely on it: `RootIconPosVertical` enables the kit's
//! `MaxSizeY` 35 on a full-height icon instance, `RootIconLabelHide` its `MinSize` 18.

use crate::bb_scene::BbNode;

/// One bound of one axis: the authored `{value, behavior}` and `enable*` flag, each
/// overridden by its style field when a style set it.
fn bound(node: &BbNode, is_width: bool, is_max: bool, primary_dim: f32, canvas_scale: f32) -> Option<f32> {
    let (authored_key, enable_key, styled_value_key, styled_enable_key) = match (is_width, is_max) {
        (true, false) => ("minWidth", "enableMinWidth", "MinSizeX", "EnableMinWidth"),
        (true, true) => ("maxWidth", "enableMaxWidth", "MaxSizeX", "EnableMaxWidth"),
        (false, false) => ("minHeight", "enableMinHeight", "MinSizeY", "EnableMinHeight"),
        (false, true) => ("maxHeight", "enableMaxHeight", "MaxSizeY", "EnableMaxHeight"),
    };
    let sizing = node.raw.get("sizing");
    let enabled = node
        .raw
        .get(styled_enable_key)
        .or_else(|| sizing.and_then(|s| s.get(enable_key)))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !enabled {
        return None;
    }
    let authored = sizing.and_then(|s| s.get(authored_key));
    let value = node
        .raw
        .get(styled_value_key)
        .and_then(|v| v.as_f64())
        .or_else(|| authored.and_then(|a| a.get("value")).and_then(|v| v.as_f64()))? as f32;
    let behavior = authored.and_then(|a| a.get("behavior")).and_then(|v| v.as_str()).unwrap_or("Fixed");
    Some(if behavior.eq_ignore_ascii_case("Percent") { primary_dim * value } else { value * canvas_scale })
}

/// Clamp a resolved extent to the node's enabled bounds on that axis (the maximum
/// first, then the minimum, so a minimum above the maximum wins).
pub(crate) fn clamp_to_size_bounds(node: &BbNode, extent: f32, primary_dim: f32, canvas_scale: f32, is_width: bool) -> f32 {
    let mut extent = extent;
    if let Some(max) = bound(node, is_width, true, primary_dim, canvas_scale) {
        extent = extent.min(max);
    }
    if let Some(min) = bound(node, is_width, false, primary_dim, canvas_scale) {
        extent = extent.max(min);
    }
    extent
}
