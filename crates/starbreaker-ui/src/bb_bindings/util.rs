use crate::bb_scene::BbNodeId;
use crate::canvas::Value;

pub(super) fn case_modifier_from_raw(node_raw: &serde_json::Value) -> &str {
    node_raw
        .get("caseModifier")
        .and_then(|v| v.as_str())
        .or_else(|| {
            node_raw
                .get("labelProperties")
                .and_then(|lp| lp.get("caseModifier"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
}

/// The host-canvas bindings routed into a synthetic component parameter
/// (`hostBoundInputs`, set by widget-standard expansion when the host canvas
/// binds the parameter's `ParamInputN` slot), in authored order. The resolver
/// evaluates them before the parameter's authored value.
pub(super) fn host_bound_inputs(op: &serde_json::Value) -> Vec<BbNodeId> {
    op.get("hostBoundInputs")
        .and_then(|v| v.as_array())
        .map(|inputs| {
            inputs
                .iter()
                .filter_map(|v| v.as_str().and_then(parse_points_to_or_ptr_str))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a tag record reference (`{_RecordName_: "Tag.<guid>", _RecordId_}`
/// or a bare string) names the tag `value` — an engine tag value as a GUID or
/// as the `Tag.<guid>` record name, compared case-insensitively.
pub(super) fn tag_reference_matches(tag: &serde_json::Value, value: &str) -> bool {
    let bare = |s: &str| s.trim().trim_start_matches("Tag.").to_ascii_lowercase();
    let value = bare(value);
    if value.is_empty() {
        return false;
    }
    [tag.get("_RecordId_"), tag.get("_RecordName_"), Some(tag)]
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .any(|s| bare(s) == value)
}

#[allow(dead_code)]
pub(super) fn derive_label_from_name(name: &str) -> String {
    let trimmed = name.trim();
    let stripped = strip_widget_prefix(trimmed);
    let stripped = strip_widget_suffix(stripped);
    if stripped.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let mut prev_lower = false;
    let mut prev_alpha = false;
    for ch in stripped.chars() {
        if ch == '_' || ch == '-' {
            if !out.ends_with(' ') && !out.is_empty() {
                out.push(' ');
            }
            prev_lower = false;
            prev_alpha = false;
            continue;
        }
        if ch.is_uppercase() && prev_lower {
            out.push(' ');
        } else if ch.is_ascii_digit() && prev_alpha {
            out.push(' ');
        }
        out.push(ch);
        prev_lower = ch.is_lowercase();
        prev_alpha = ch.is_alphabetic();
    }
    out.trim().to_owned()
}

#[allow(dead_code)]
fn strip_widget_prefix(s: &str) -> &str {
    const PREFIXES: &[&str] = &["text_", "txt_", "lbl_", "label_", "Text_", "Label_"];
    for p in PREFIXES {
        if let Some(rest) = s.strip_prefix(p) {
            return rest;
        }
    }
    s
}

#[allow(dead_code)]
fn strip_widget_suffix(s: &str) -> &str {
    const SUFFIXES: &[&str] = &["Text", "Label"];
    for sfx in SUFFIXES {
        if let Some(rest) = s.strip_suffix(sfx)
            && !rest.is_empty()
        {
            return rest;
        }
    }
    s
}

/// Whether a resolved label string is an empty / placeholder sentinel that
/// carries no real content. Used so a component-parameter-driven label whose
/// parameter is unset at rest (e.g. an inactive footer warning message) renders
/// empty rather than surfacing its design-time placeholder.
pub(super) fn is_placeholder_label(s: &str) -> bool {
    let t = s.trim();
    t.is_empty()
        || t.eq_ignore_ascii_case("@LOC_PLACEHOLDER")
        || t.eq_ignore_ascii_case("LOC_PLACEHOLDER")
        || t.eq_ignore_ascii_case("@LOC_EMPTY")
        || t.eq_ignore_ascii_case("LOC_EMPTY")
}

pub(crate) fn apply_case_modifier(s: &str, modifier: &str) -> String {
    match modifier {
        "Upper" | "AllCaps" => s.to_uppercase(),
        "Lower" => s.to_lowercase(),
        _ => s.to_owned(),
    }
}

pub(super) fn value_to_string(v: &Value) -> String {
    match v {
        Value::Str(s) | Value::Guid(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e9 {
                format!("{}", *f as i64)
            } else {
                format!("{f:.2}")
            }
        }
        Value::Bool(b) => {
            if *b {
                "ON".to_owned()
            } else {
                "OFF".to_owned()
            }
        }
    }
}

pub(super) fn number_to_compact_string(n: f64) -> String {
    if (n.fract()).abs() < f64::EPSILON {
        format!("{:.0}", n)
    } else {
        let s = format!("{:.3}", n);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub(super) fn parse_ptr(s: &str) -> Option<BbNodeId> {
    s.strip_prefix("ptr:").and_then(|n| n.parse().ok())
}

fn parse_points_to(s: &str) -> Option<BbNodeId> {
    s.strip_prefix("_PointsTo_:").and_then(parse_ptr)
}

pub(super) fn parse_points_to_or_ptr_str(s: &str) -> Option<BbNodeId> {
    parse_points_to(s).or_else(|| parse_ptr(s))
}

pub(super) fn parse_points_to_or_ptr(value: Option<&serde_json::Value>) -> Option<BbNodeId> {
    match value {
        Some(serde_json::Value::String(s)) => parse_points_to(s).or_else(|| parse_ptr(s)),
        Some(serde_json::Value::Object(obj)) => obj
            .get("_Pointer_")
            .and_then(|v| v.as_str())
            .and_then(parse_ptr),
        _ => None,
    }
}
