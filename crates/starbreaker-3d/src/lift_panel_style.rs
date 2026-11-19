//! The style and labels of a lift's floor-select panel (`lifts[].panel_style`, WP-P5 floor panel).
//!
//! The game draws no UI on some lifts' floor panels (the C2's inner panel has a screen face but no canvas); a host that
//! draws its own floor list there takes everything it can from data, in the look of the vehicle's door pads:
//!
//! - [`stop_display_names`]: each stop's name, the display name of the panel interaction whose link asks the carriage
//!   for that stop (`From Elevator call <N>`).
//! - [`is_door_pad`] / [`label_style_from_ir`]: the label style of the door pads' main button: a binding whose owner
//!   mirrors a door (state type `DoorOpenState`), its canvas compiled to IR, the large button's label (font symbol, size,
//!   letter spacing, case, colour role).
//! - [`build`]: the brand palette by `BB_ColorStyle` role, the label style, and one white label mask per named stop in
//!   the pads' font ([`starbreaker_ui::text::render_label_mask`]) as PNG files.
//!
//! The host chooses which roles colour what and lays the list out: that part is its own UI, not the game's.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Value, json};
use starbreaker_ui::style::colour_roles::BB_COLOUR_STYLE_ROLES;
use starbreaker_ui::swf_assets::SwfAssetLibrary;

use crate::pipeline::lifts::Lift;
use crate::types::{InteriorPlacement, UiBinding};

/// The carriage input a floor panel's interaction links to (`From Elevator call <stop>`; game vocabulary).
const CALL_SLOT_PREFIX: &str = "From Elevator call ";
/// The owner state type of a door pad: the pad's state machine mirrors its door (game vocabulary).
const DOOR_OPEN_STATE: &str = "DoorOpenState";
/// The IR node type of the door pads' main button (`BuildingBlocks_ComponentGeneralButtonLarge`).
const LARGE_BUTTON: &str = "component_general_button_large";
/// Raster size of a label mask's em. A host resolution, not a game value: the host scales the mask to its row.
pub(crate) const LABEL_EM_PX: f32 = 128.0;

/// The name of each stop of `lift` that a placed panel requests: the display name of the panel's interaction whose
/// link targets the carriage's `From Elevator call <stop>`. Stops no panel names are absent.
pub(crate) fn stop_display_names(lift: &Lift, placements: &[InteriorPlacement]) -> BTreeMap<usize, String> {
    let mut names = BTreeMap::new();
    for placement in placements {
        let identity = &placement.identity;
        for link in identity.interaction_links.iter().filter(|l| l.target_entity_id == lift.entity_id) {
            let Some(stop) = link.target_slot.strip_prefix(CALL_SLOT_PREFIX).and_then(|n| n.parse::<usize>().ok()) else {
                continue;
            };
            if stop >= lift.stops.len() {
                continue;
            }
            for interaction in identity.interaction_points.iter().flat_map(|p| &p.interactions) {
                if interaction.name == link.source_slot
                    && let Some(name) = interaction.display_name.as_deref().filter(|n| !n.is_empty())
                {
                    names.insert(stop, name.to_string());
                }
            }
        }
    }
    names
}

/// A panel binding whose owner mirrors a door: a door pad.
pub(crate) fn is_door_pad(binding: &UiBinding) -> bool {
    binding.state_types.iter().any(|t| t.name == DOOR_OPEN_STATE)
}

/// The label style of a canvas's large button, read from its compiled IR.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct LabelStyle {
    /// The resolved `BuildingBlocks_FontStyle` record name.
    pub font_style: Option<String>,
    /// The font's export symbol in the shared font library (`font` of the font style).
    pub font_symbol: String,
    /// Font size and letter spacing in the canvas's target pixels.
    pub font_size: f32,
    pub letter_spacing: f32,
    /// The label renders in capitals (its resolved text is all upper case).
    pub upper_case: bool,
    /// The `BB_ColorStyle` role of the label's colour, when the IR names one.
    pub colour_token: Option<String>,
}

/// The first active, resolved label inside a large button of `ir` (a `UiIrDocument` as JSON) and its text style.
pub(crate) fn label_style_from_ir(ir: &Value) -> Option<LabelStyle> {
    let nodes = ir.get("nodes")?.as_array()?;
    let id = |n: &Value| n.get("id").and_then(Value::as_u64);
    let by_id: HashMap<u64, &Value> = nodes.iter().filter_map(|n| Some((id(n)?, n))).collect();
    let active = |n: &Value| n.get("is_active").and_then(Value::as_bool).unwrap_or(false);
    // The node and its ancestors: all active, one of them a large button.
    let in_active_large_button = |node: &Value| {
        let mut large = false;
        let mut current = Some(node);
        let mut steps = 0;
        while let Some(n) = current {
            if !active(n) || steps > nodes.len() {
                return false;
            }
            large |= n.get("node_type").and_then(Value::as_str) == Some(LARGE_BUTTON);
            current = n.get("parent_id").and_then(Value::as_u64).and_then(|p| by_id.get(&p).copied());
            steps += 1;
        }
        large
    };
    for node in nodes {
        let payload = node.get("text_payload");
        let Some(text) = payload
            .filter(|p| p.get("state").and_then(Value::as_str) == Some("resolved"))
            .and_then(|p| p.get("text"))
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
        else {
            continue;
        };
        let Some(style) = node.get("text_style") else { continue };
        let font = style.get("resolved_font_record");
        let Some(symbol) = font
            .and_then(|f| f.get("_RecordValue_"))
            .and_then(|v| v.get("font"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(font_size) = style.get("font_size").and_then(|s| s.get("value")).and_then(Value::as_f64) else {
            continue;
        };
        if !in_active_large_button(node) {
            continue;
        }
        return Some(LabelStyle {
            font_style: font.and_then(|f| f.get("_RecordName_")).and_then(Value::as_str).map(str::to_string),
            font_symbol: symbol.to_string(),
            font_size: font_size as f32,
            letter_spacing: style.get("letter_spacing").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            upper_case: text.chars().any(char::is_alphabetic) && text == text.to_uppercase(),
            colour_token: style.get("colour_token").and_then(Value::as_str).map(str::to_string),
        });
    }
    None
}

/// Where the style came from (provenance in the export).
pub(crate) struct StyleSource<'a> {
    pub entity_id: Option<&'a str>,
    pub binding: &'a UiBinding,
    /// The brand style record and its `colorStyles` by `BB_ColorStyle` index.
    pub palette: Option<(String, Vec<Option<[u8; 4]>>)>,
}

/// `lifts[].panel_style` for `lift` and the label masks it names: `(json, [(path, png)])`. `path_for` gives the package
/// path of a mask from its file stem. `None` when no stop is named or the font is not in `fonts`.
pub(crate) fn build(
    lift: &Lift,
    names: &BTreeMap<usize, String>,
    label: &LabelStyle,
    source: &StyleSource<'_>,
    fonts: &SwfAssetLibrary,
    path_for: impl Fn(&str) -> String,
) -> Option<(Value, Vec<(String, Vec<u8>)>)> {
    if names.is_empty() {
        return None;
    }
    let Some(font) = fonts.lookup_export(&label.font_symbol).and_then(|id| fonts.get_font(id)) else {
        log::warn!("lift {}: panel label font '{}' is not in the shared font library", lift.entity_id, label.font_symbol);
        return None;
    };
    let metrics = fonts.font_edit_text_metrics(&label.font_symbol);
    // The pads' tracking, as a fraction of their em, at the mask's em.
    let letter_spacing_px = if label.font_size > 0.0 { label.letter_spacing / label.font_size * LABEL_EM_PX } else { 0.0 };
    let mut files = Vec::new();
    let mut labels = Vec::new();
    for (&stop, name) in names {
        let text = if label.upper_case { name.to_uppercase() } else { name.clone() };
        let Some(mask) = starbreaker_ui::text::render_label_mask(font, metrics, &text, LABEL_EM_PX, letter_spacing_px) else {
            log::warn!("lift {}: stop {stop} label '{text}' did not render", lift.entity_id);
            continue;
        };
        let Ok(png) = starbreaker_ui::encode_png(&mask.image) else { continue };
        let path = path_for(&format!("lift_label_{}_{}", label.font_symbol, text));
        labels.push(json!({
            "stop": stop,
            "text": text,
            "image": path,
            "width": mask.image.width(),
            "height": mask.image.height(),
            "em_px": mask.em_px,
        }));
        files.push((path, png));
    }
    if labels.is_empty() {
        return None;
    }
    let (style_record, palette) = match &source.palette {
        Some((name, slots)) => (
            Some(name.clone()),
            slots
                .iter()
                .zip(BB_COLOUR_STYLE_ROLES)
                .filter_map(|(slot, role)| slot.map(|c| json!({ "role": role, "srgba": c })))
                .collect::<Vec<_>>(),
        ),
        None => (None, Vec::new()),
    };
    let value = json!({
        "source": {
            "entity_id": source.entity_id,
            "canvas": source.binding.canvas_record_name,
            "style_record": style_record,
        },
        "palette": palette,
        "label": label,
        "labels": labels,
    });
    Some((value, files))
}

#[cfg(test)]
#[path = "lift_panel_style_tests.rs"]
mod tests;
