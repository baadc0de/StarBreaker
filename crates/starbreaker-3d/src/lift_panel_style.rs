//! The style and labels of a lift's floor-select panel (`lifts[].panel_style`, WP-P5 floor panel).
//!
//! The game draws no UI on some lifts' floor panels (the C2's inner panel has a screen face but no canvas); a host that
//! draws its own floor list there takes everything it can from data, in the look of the vehicle's door pads:
//!
//! - [`stop_display_names`]: the names each stop is offered under, the display names of the panel interactions whose
//!   links ask the carriage for that stop (`From Elevator call <N>`); two panels may name one stop differently.
//! - [`is_door_pad`] / [`label_style_from_ir`] / [`style_manufacturer`]: the label style of the door pads' main button:
//!   a binding whose owner mirrors a door (state type `DoorOpenState`), its canvas compiled to IR, the large button's
//!   label (font symbol, size, letter spacing, case, colour role), and the brand style the canvas rendered with.
//! - [`build`]: the brand palette by `BB_ColorStyle` role, the label style, and one white label mask per (stop, name) in
//!   the pads' font ([`starbreaker_ui::text::render_label_mask`]) as PNG files, each independent of the others: a palette
//!   without a font, a font without a palette, a name the font cannot spell left without a mask.
//! - [`attach_masks`]: writes the masks and gives each label the path its own file got.
//!
//! The host chooses which roles colour what and lays the list out: that part is its own UI, not the game's.

use std::collections::{BTreeSet, HashMap};

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

/// The names each stop of `lift` is offered under by a placed panel: `(stop, display name)` of every panel interaction
/// whose link targets the carriage's `From Elevator call <stop>`. Stops no panel names are absent.
pub(crate) fn stop_display_names(lift: &Lift, placements: &[InteriorPlacement]) -> BTreeSet<(usize, String)> {
    let mut names = BTreeSet::new();
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
                    names.insert((stop, name.to_string()));
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

/// The manufacturer whose brand style a compiled canvas rendered with (`selected_style_source` `manufacturer:<id>`);
/// `None` for any other source (a canvas-linked style: its palette is not the manufacturer's).
pub(crate) fn style_manufacturer(ir: &Value) -> Option<&str> {
    ir.get("selected_style_source")?.as_str()?.strip_prefix("manufacturer:").filter(|id| !id.is_empty())
}

/// Where the style came from (provenance in the export).
pub(crate) struct StyleSource<'a> {
    pub entity_id: Option<&'a str>,
    pub binding: Option<&'a UiBinding>,
    /// The brand style record the pad's canvas rendered with and its `colorStyles` by `BB_ColorStyle` index.
    pub palette: Option<(String, Vec<Option<[u8; 4]>>)>,
}

/// Whether every visible character of `text` has a glyph in `font` (the rasteriser skips a missing one silently).
fn font_spells(font: &starbreaker_ui::swf_assets::FontGlyphSet, text: &str) -> bool {
    text.chars().filter(|c| !c.is_whitespace()).all(|c| {
        let code = u16::try_from(u32::from(c)).ok();
        code.is_some() && font.glyphs.iter().any(|g| g.code == code)
    })
}

/// `lifts[].panel_style` for `lift` and the label masks it names, in the order of its `labels[]`: `(json, [(path,
/// png)])`. `path_for` gives the package path of a mask from its file stem. Each part stands alone: the palette without a
/// label style, the label style and masks without a palette, a name the font cannot spell without a mask (logged).
/// `None` when there is neither a palette nor a label style.
pub(crate) fn build(
    lift: &Lift,
    names: &BTreeSet<(usize, String)>,
    label: Option<&LabelStyle>,
    source: &StyleSource<'_>,
    fonts: Option<&SwfAssetLibrary>,
    path_for: impl Fn(&str) -> String,
) -> Option<(Value, Vec<(String, Vec<u8>)>)> {
    if label.is_none() && source.palette.is_none() {
        return None;
    }
    let mut files = Vec::new();
    let mut labels = Vec::new();
    let font = label.zip(fonts).and_then(|(label, fonts)| {
        let font = fonts.lookup_export(&label.font_symbol).and_then(|id| fonts.get_font(id));
        if font.is_none() {
            log::warn!("lift {}: panel label font '{}' is not in the shared font library", lift.entity_id, label.font_symbol);
        }
        font.map(|font| (label, font, fonts.font_edit_text_metrics(&label.font_symbol)))
    });
    if let Some((label, font, metrics)) = font {
        // The pads' tracking, as a fraction of their em, at the mask's em.
        let letter_spacing_px = if label.font_size > 0.0 { label.letter_spacing / label.font_size * LABEL_EM_PX } else { 0.0 };
        for (stop, name) in names {
            let text = if label.upper_case { name.to_uppercase() } else { name.clone() };
            if !font_spells(font, &text) {
                log::warn!("lift {}: stop {stop} label '{text}' has characters the pads' font lacks; no mask", lift.entity_id);
                continue;
            }
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
            "canvas": source.binding.and_then(|b| b.canvas_record_name.as_deref()),
            "style_record": style_record,
        },
        "palette": palette,
        "label": label,
        "labels": labels,
    });
    Some((value, files))
}

/// Write `masks` (in the order of `value["labels"]`, as [`build`] returns them) with `insert`, which returns the path a
/// file was written under (another when the wanted one holds different bytes), and give each label its own file's path.
pub(crate) fn attach_masks(value: &mut Value, masks: Vec<(String, Vec<u8>)>, mut insert: impl FnMut(String, Vec<u8>) -> String) {
    let labels = value.get_mut("labels").and_then(Value::as_array_mut);
    for (entry, (path, png)) in labels.into_iter().flatten().zip(masks) {
        entry["image"] = json!(insert(path, png));
    }
}

#[cfg(test)]
#[path = "lift_panel_style_tests.rs"]
mod tests;
