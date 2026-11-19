//! Style source selection for pipeline IR rendering.

use std::collections::BTreeMap;

use crate::error::UiError;
use crate::style::{ManufacturerStyle, StyleLoader};
use crate::ui_ir::UiIrDocument;

use super::{CanvasFetcher, PipelineInputs, StyleFetcher, extract_record_name, load_style};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StyleSelectionManifest {
    pub selected_source: Option<String>,
    pub fallback_counters: BTreeMap<String, u32>,
}

pub(super) fn load_style_for_ir(
    ir: &UiIrDocument,
    inputs: &PipelineInputs<'_>,
) -> Result<ManufacturerStyle, UiError> {
    if let Some(style_name) = ir
        .selected_style_source
        .as_deref()
        .and_then(|source| source.strip_prefix("canvas:"))
    {
        let style_record = inputs.canvas_fetcher.fetch_canvas_by_name(style_name)?;
        // The loader's manufacturer only names the parsed style; no brand is invented.
        let loader = StyleLoader::for_manufacturer(inputs.binding.manufacturer_id.unwrap_or(""));
        return loader.parse_buildingblocks_style_record(&style_record);
    }

    if let Some(manufacturer_id) = ir
        .selected_style_source
        .as_deref()
        .and_then(|source| source.strip_prefix("manufacturer:"))
    {
        return Ok(load_style(Some(manufacturer_id), inputs.style_fetcher));
    }

    Ok(load_style(inputs.binding.manufacturer_id, inputs.style_fetcher))
}

pub(super) fn build_style_selection_manifest(
    raw_root_json: &serde_json::Value,
    manufacturer_id: Option<&str>,
    canvas_fetcher: &dyn CanvasFetcher,
    style_fetcher: &dyn StyleFetcher,
) -> StyleSelectionManifest {
    let mut fallback_counters = BTreeMap::new();

    if let Some(style_name) = raw_root_json
        .get("_RecordValue_")
        .and_then(|rv| rv.get("style"))
        .and_then(|v| v.as_str())
        .map(extract_record_name)
    {
        match canvas_fetcher.fetch_canvas_by_name(&style_name) {
            Ok(_) => {
                return StyleSelectionManifest {
                    selected_source: Some(format!("canvas:{style_name}")),
                    fallback_counters,
                };
            }
            Err(e) => {
                log::warn!(
                    "pipeline: failed to fetch canvas-level style record '{}': {}",
                    style_name,
                    e
                );
                fallback_counters.insert("canvas_style_fetch_failed".to_string(), 1);
            }
        }
    }

    // The brand comes from data only: the canvas's own style above, else the
    // binding's manufacturer. Without either, or when that manufacturer has no
    // style record, the canvas renders without a brand (its authored default
    // styles); no manufacturer is invented.
    let Some(manufacturer) = manufacturer_id else {
        fallback_counters.insert("no_brand".to_string(), 1);
        return StyleSelectionManifest { selected_source: None, fallback_counters };
    };
    match style_fetcher.fetch_manufacturer_style(manufacturer) {
        Ok(_) => StyleSelectionManifest {
            selected_source: Some(format!("manufacturer:{manufacturer}")),
            fallback_counters,
        },
        Err(e) => {
            log::warn!("pipeline: manufacturer style fetch failed for '{manufacturer}': {e}; rendering without a brand");
            fallback_counters.insert("manufacturer_style_missing".to_string(), 1);
            StyleSelectionManifest { selected_source: None, fallback_counters }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MissingCanvasStyleFetcher;

    impl crate::pipeline::CanvasFetcher for MissingCanvasStyleFetcher {
        fn fetch_canvas_json(&self, guid: &str) -> Result<serde_json::Value, UiError> {
            Err(UiError::FetchFailed {
                guid: guid.to_string(),
                source: "missing canvas".to_string().into(),
            })
        }
    }

    struct MissingManufacturerStyleFetcher;

    impl crate::pipeline::StyleFetcher for MissingManufacturerStyleFetcher {
        fn fetch_manufacturer_style(&self, manufacturer_id: &str) -> Result<ManufacturerStyle, UiError> {
            Err(UiError::RenderError(format!("missing manufacturer style: {manufacturer_id}")))
        }
    }

    #[test]
    fn style_selection_manifest_counts_manufacturer_fallback() {
        let root = serde_json::json!({
            "_RecordValue_": {
                "style": "file://./foo_style.json"
            }
        });

        let manifest = build_style_selection_manifest(
            &root,
            Some("aegs"),
            &MissingCanvasStyleFetcher,
            &MissingManufacturerStyleFetcher,
        );

        // WP-I1 fix pass: a missing style is not replaced by an invented brand.
        assert_eq!(manifest.selected_source, None);
        assert_eq!(manifest.fallback_counters.get("canvas_style_fetch_failed"), Some(&1));
        assert_eq!(manifest.fallback_counters.get("manufacturer_style_missing"), Some(&1));
    }

    /// WP-I1 fix pass: a binding without a manufacturer (the hangar transit panel: no attachable manufacturer, no
    /// `defaultStyle`, none on its container) and a canvas without a style select no brand. It used to be Drake.
    #[test]
    fn style_selection_without_manufacturer_selects_no_brand() {
        let root = serde_json::json!({"_RecordValue_": {}});
        let manifest = build_style_selection_manifest(&root, None, &MissingCanvasStyleFetcher, &MissingManufacturerStyleFetcher);
        assert_eq!(manifest.selected_source, None);
        assert_eq!(manifest.fallback_counters.get("no_brand"), Some(&1));
    }
}
