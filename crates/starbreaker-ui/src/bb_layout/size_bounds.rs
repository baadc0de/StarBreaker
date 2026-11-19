//! A node's size bounds (WP-I1 fix pass 2): the authored `sizing.minWidth` /
//! `maxWidth` / `minHeight` / `maxHeight` values with their `enable*` flags, and the
//! style fields that override them (`MinSizeX` / `MaxSizeX` / `MinSizeY` / `MaxSizeY`
//! values, `Min/MaxWidthBehavior` / `Min/MaxHeightBehavior`, `EnableMinWidth` /
//! `EnableMaxWidth` / `EnableMinHeight` / `EnableMaxHeight` flags).
//! [`clamp_to_size_bounds`] clamps a resolved extent (`resolve_value_for_node` and the
//! flex allocation); [`scaled_about_pivot`] applies a node's scale to a flex-placed
//! rect; [`scale_fixed_size_bounds`] carries Fixed bounds through a child-canvas merge.
//! The button standards rely on it: `RootIconPosVertical` enables the kit's
//! `MaxSizeY` 35 on a full-height icon instance, `RootIconLabelHide` its `MinSize` 18.

use crate::bb_scene::BbNode;

/// One bound of one axis: the authored `{value, behavior}` and `enable*` flag, each
/// overridden by its style field when a style set it.
fn bound(node: &BbNode, is_width: bool, is_max: bool, primary_dim: f32, canvas_scale: f32) -> Option<f32> {
    let (authored_key, enable_key, styled_value_key, styled_enable_key, styled_behavior_key) = match (is_width, is_max) {
        (true, false) => ("minWidth", "enableMinWidth", "MinSizeX", "EnableMinWidth", "MinWidthBehavior"),
        (true, true) => ("maxWidth", "enableMaxWidth", "MaxSizeX", "EnableMaxWidth", "MaxWidthBehavior"),
        (false, false) => ("minHeight", "enableMinHeight", "MinSizeY", "EnableMinHeight", "MinHeightBehavior"),
        (false, true) => ("maxHeight", "enableMaxHeight", "MaxSizeY", "EnableMaxHeight", "MaxHeightBehavior"),
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
    let behavior = node
        .raw
        .get(styled_behavior_key)
        .or_else(|| authored.and_then(|a| a.get("behavior")))
        .and_then(|v| v.as_str())
        .unwrap_or("Fixed");
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

/// A flex-placed rect with the node's scale (`scale`, styled `ScaleX` / `ScaleY`)
/// applied about its pivot, as the overlay placement does.
pub(crate) fn scaled_about_pivot(node: &BbNode, rect: super::Rect) -> super::Rect {
    let factor = |key: &str| {
        node.raw
            .get("scale")
            .and_then(|s| s.get(key))
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(1.0)
    };
    let (sx, sy) = (factor("x"), factor("y"));
    let (w, h) = (rect.w * sx, rect.h * sy);
    super::Rect { x: rect.x + (rect.w - w) * node.pivot.x, y: rect.y + (rect.h - h) * node.pivot.y, w, h }
}

/// Scale a node's Fixed size bounds (authored and styled) by a child-canvas merge's
/// per-axis factors, as its sizes are; Percent bounds stay fractions.
pub(crate) fn scale_fixed_size_bounds(node: &mut BbNode, sx: f32, sy: f32) {
    for (authored_key, styled_key, behavior_key, factor) in [
        ("minWidth", "MinSizeX", "MinWidthBehavior", sx),
        ("maxWidth", "MaxSizeX", "MaxWidthBehavior", sx),
        ("minHeight", "MinSizeY", "MinHeightBehavior", sy),
        ("maxHeight", "MaxSizeY", "MaxHeightBehavior", sy),
    ] {
        let authored_behavior = node.raw.get("sizing").and_then(|s| s.get(authored_key)).and_then(|a| a.get("behavior")).and_then(|v| v.as_str());
        let is_fixed = node.raw.get(behavior_key).and_then(|v| v.as_str()).or(authored_behavior).is_none_or(|b| !b.eq_ignore_ascii_case("Percent"));
        if !is_fixed {
            continue;
        }
        if let Some(value) = node.raw.get_mut("sizing").and_then(|s| s.get_mut(authored_key)).and_then(|a| a.get_mut("value"))
            && let Some(v) = value.as_f64()
        {
            *value = serde_json::json!(v * factor as f64);
        }
        if let Some(value) = node.raw.get_mut(styled_key)
            && let Some(v) = value.as_f64()
        {
            *value = serde_json::json!(v * factor as f64);
        }
    }
}

/// A flex item's final rect: its extents clamped to its enabled size bounds (Percent
/// bounds against the flex container; the flow's stretch and grow allocations bypass
/// `resolve_value_for_node`), shrunk about its pivot, then its scale applied.
pub(crate) fn flex_item_rect(
    scene: &crate::bb_scene::BbScene,
    id: crate::bb_scene::BbNodeId,
    rect: super::Rect,
    container: super::Rect,
    csx: f32,
    csy: f32,
) -> super::Rect {
    let Some(node) = scene.nodes.get(&id) else { return rect };
    let w = clamp_to_size_bounds(node, rect.w, container.w, csx, true);
    let h = clamp_to_size_bounds(node, rect.h, container.h, csy, false);
    let bounded = super::Rect { x: rect.x + (rect.w - w) * node.pivot.x, y: rect.y + (rect.h - h) * node.pivot.y, w, h };
    scaled_about_pivot(node, bounded)
}
