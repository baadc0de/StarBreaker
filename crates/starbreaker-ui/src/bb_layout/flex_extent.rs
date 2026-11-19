//! Content extent of `Auto`-sized flex items that flow beside a growing sibling
//! (`auto_content_main_extent`): padding plus the extent of the item's
//! layout-affecting children along the flow axis. Split from `engine_01` by
//! responsibility (line-cap guard; scviewer WP-I1 fix pass).

#[allow(unused_imports)]
use super::*;
use crate::bb_scene::{BbScene, BbValue};

/// Content extent along one axis of an `Auto`-sized flow item beside growing
/// siblings: its padding plus the extent of its active, layout-affecting
/// children — summed (with spacing) when the node itself flows them along that
/// axis, else the largest. A child counts with a `Fixed` size or, recursively,
/// an `Auto` one; a `Percent` child depends on this size and adds nothing. A
/// node with no such content is empty (0).
pub(crate) fn auto_content_main_extent(node: &crate::bb_scene::BbNode, scene: &BbScene, is_row: bool, scale: f32) -> f32 {
    let padding = if is_row { node.padding.left + node.padding.right } else { node.padding.top + node.padding.bottom } * scale;
    let flow = node.raw.get("layoutPolicy").filter(|p| {
        p.get("_Type_").and_then(|t| t.as_str()).is_some_and(|t| t.contains("FlexContainer"))
            && p.get("direction")
                .and_then(|d| d.as_str())
                .is_some_and(|d| d.to_ascii_lowercase().starts_with("column") != is_row)
    });
    let extents: Vec<f32> = node
        .children
        .iter()
        .filter_map(|id| scene.nodes.get(id))
        .filter(|child| child.is_active && child.raw.get("affectsLayout").and_then(|v| v.as_bool()) != Some(false))
        .map(|child| match if is_row { &child.sizing.width } else { &child.sizing.height } {
            BbValue::Fixed(px) => px * scale,
            BbValue::Other { behavior, .. } if behavior == "Auto" => auto_content_main_extent(child, scene, is_row, scale),
            _ => 0.0,
        })
        .collect();
    let content = match flow {
        Some(policy) => {
            let key = if is_row { "columnSpacing" } else { "rowSpacing" };
            let spacing = policy.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32 * scale;
            extents.iter().sum::<f32>() + spacing * (extents.len() as f32 - 1.0).max(0.0)
        }
        None => extents.iter().copied().fold(0.0, f32::max),
    };
    padding + content
}
