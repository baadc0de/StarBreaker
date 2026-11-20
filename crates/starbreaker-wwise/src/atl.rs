//! ATL (Audio Translation Layer) XML parser for Star Citizen's GameAudio files.
//!
//! Parses `Data\Libs\GameAudio\*.xml` from the P4k archive to build a lookup
//! index from trigger names to their associated Wwise events and bank files.
//!
//! The documents are read by [`crate::game_audio`] (CryXmlB or text XML);
//! [`AtlIndex::add_document`] indexes one, [`AtlIndex::from_p4k`] all of them
//! ([`crate::game_audio::load`] builds this and the external-source index in
//! one scan).

use std::collections::HashMap;

use quick_xml::Reader;
use quick_xml::events::Event;
use serde::Serialize;
use starbreaker_cryxml::{CryXml, CryXmlNode};
use starbreaker_p4k::MappedP4k;

use crate::error::BnkError;
use crate::game_audio::{GameAudioDoc, cryxml_attr, for_each_document, xml_attr as get_attr};

/// A single ATL trigger entry, linking a trigger name to its Wwise event and bank file.
#[derive(Debug, Clone, Serialize)]
pub struct AtlTrigger {
    /// The ATL trigger name (e.g., `"Play_SSTP_AEGS_Gladius_Start_Run"`).
    pub trigger_name: String,
    /// The Wwise event name (usually matches trigger_name).
    pub wwise_event_name: String,
    /// The bank file name (e.g., `"SSTP_AEGS_Gladius_01.bnk"`).
    pub bank_name: String,
    /// Duration type: `"OneShot"` or `"Infinite"`.
    pub duration_type: String,
    /// Maximum attenuation radius, if specified.
    pub radius_max: Option<f32>,
    /// `wwise_duration_min` / `wwise_duration_max` (seconds), if specified.
    pub duration_min: Option<f32>,
    pub duration_max: Option<f32>,
    /// `RTPCDependees`: the ATL RTPC names the trigger's sounds depend on.
    pub rtpc_dependees: Vec<String>,
}

/// An ATL environment (`ATLEnvironment`): the Wwise auxiliary bus the game sends to for it.
#[derive(Debug, Clone, Serialize)]
pub struct AtlEnvironment {
    pub atl_name: String,
    /// `WwiseAuxBus wwise_name`.
    pub aux_bus: String,
}

/// An ATL RTPC (`ATLRtpc` -> `WwiseRtpc`): the Wwise game parameter, its range and its default.
#[derive(Debug, Clone, Serialize)]
pub struct AtlRtpc {
    pub atl_name: String,
    pub wwise_name: String,
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub default: Option<f32>,
}

/// Index of ATL triggers built from all GameAudio XML files in the P4k.
#[derive(Default)]
pub struct AtlIndex {
    /// trigger_name -> AtlTrigger
    triggers: HashMap<String, AtlTrigger>,
    /// bank_name -> list of trigger_names
    bank_triggers: HashMap<String, Vec<String>>,
    /// atl_name -> environment
    environments: HashMap<String, AtlEnvironment>,
    /// atl_name -> RTPC
    rtpcs: HashMap<String, AtlRtpc>,
}

impl AtlIndex {
    /// Build the ATL index by scanning all `Data\Libs\GameAudio\*.xml` files in the P4k.
    pub fn from_p4k(p4k: &MappedP4k) -> Result<Self, BnkError> {
        let mut index = Self::default();
        for_each_document(p4k, |name, doc| {
            if let Err(e) = index.add_document(doc) {
                eprintln!("warning: failed to parse ATL XML {name}: {e}");
            }
        });
        Ok(index)
    }

    /// Index the triggers of one GameAudio document.
    pub fn add_document(&mut self, doc: &GameAudioDoc<'_>) -> Result<(), BnkError> {
        let parsed = match doc {
            GameAudioDoc::Binary(xml) => parse_atl_cryxml(xml),
            GameAudioDoc::Text(xml) => parse_atl_xml(xml)?,
        };
        for environment in parsed.environments {
            self.environments.insert(environment.atl_name.clone(), environment);
        }
        for rtpc in parsed.rtpcs {
            self.rtpcs.insert(rtpc.atl_name.clone(), rtpc);
        }
        for trigger in parsed.triggers {
            self.bank_triggers
                .entry(trigger.bank_name.clone())
                .or_default()
                .push(trigger.trigger_name.clone());
            self.triggers.insert(trigger.trigger_name.clone(), trigger);
        }
        Ok(())
    }

    /// Look up a trigger by exact name.
    pub fn get_trigger(&self, name: &str) -> Option<&AtlTrigger> {
        self.triggers.get(name)
    }

    /// Search for triggers whose names contain the query (case-insensitive).
    pub fn search(&self, query: &str) -> Vec<&AtlTrigger> {
        let query_lower = query.to_ascii_lowercase();
        self.triggers
            .values()
            .filter(|t| t.trigger_name.to_ascii_lowercase().contains(&query_lower))
            .collect()
    }

    /// Get all trigger names associated with a given bank file.
    pub fn triggers_for_bank(&self, bank_name: &str) -> Vec<&str> {
        self.bank_triggers
            .get(bank_name)
            .map(|names| names.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Total number of triggers in the index.
    pub fn len(&self) -> usize {
        self.triggers.len()
    }

    /// Whether the index contains no triggers.
    pub fn is_empty(&self) -> bool {
        self.triggers.is_empty()
    }

    /// Iterate over all triggers in the index.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &AtlTrigger)> {
        self.triggers.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// An ATL environment by name.
    pub fn environment(&self, atl_name: &str) -> Option<&AtlEnvironment> {
        self.environments.get(atl_name)
    }

    /// Every ATL environment.
    pub fn environments(&self) -> impl Iterator<Item = &AtlEnvironment> {
        self.environments.values()
    }

    /// An ATL RTPC by name.
    pub fn rtpc(&self, atl_name: &str) -> Option<&AtlRtpc> {
        self.rtpcs.get(atl_name)
    }

    /// Every ATL RTPC.
    pub fn rtpcs(&self) -> impl Iterator<Item = &AtlRtpc> {
        self.rtpcs.values()
    }

    /// Get all unique bank names referenced by triggers.
    pub fn bank_names(&self) -> Vec<&str> {
        self.bank_triggers.keys().map(|s| s.as_str()).collect()
    }
}

// ── CryXmlB parser ──────────────────────────────────────────────────────────

/// Recursively search for a WwiseFile node under a given parent.
fn find_wwise_file_recursive(xml: &CryXml<'_>, node: &CryXmlNode) -> Option<String> {
    for child in xml.node_children(node) {
        if xml.node_tag(child) == "WwiseFile" {
            return cryxml_attr(xml, child, "wwise_name").map(|s| s.to_owned());
        }
        // Recurse into children (e.g., ATLConfigGroup)
        if let Some(found) = find_wwise_file_recursive(xml, child) {
            return Some(found);
        }
    }
    None
}

/// Intermediate trigger data collected during parsing.
#[derive(Default)]
struct RawTrigger {
    trigger_name: String,
    wwise_event_name: String,
    duration_type: String,
    radius_max: Option<f32>,
    duration_min: Option<f32>,
    duration_max: Option<f32>,
    rtpc_dependees: Vec<String>,
    preload_name: String,
}

impl RawTrigger {
    fn named(trigger_name: String) -> Self {
        Self { trigger_name, ..Self::default() }
    }

    /// Read one `WwiseEvent` attribute.
    fn event_attr(&mut self, key: &str, value: &str) {
        match key {
            "wwise_name" => self.wwise_event_name = value.to_owned(),
            "wwise_duration_type" => self.duration_type = value.to_owned(),
            "wwise_radius_max" => self.radius_max = value.parse::<f32>().ok(),
            "wwise_duration_min" => self.duration_min = value.parse::<f32>().ok(),
            "wwise_duration_max" => self.duration_max = value.parse::<f32>().ok(),
            _ => {}
        }
    }

    fn into_trigger(self, preload_banks: &HashMap<String, String>) -> AtlTrigger {
        AtlTrigger {
            bank_name: preload_banks.get(&self.preload_name).cloned().unwrap_or_default(),
            trigger_name: self.trigger_name,
            wwise_event_name: self.wwise_event_name,
            duration_type: self.duration_type,
            radius_max: self.radius_max,
            duration_min: self.duration_min,
            duration_max: self.duration_max,
            rtpc_dependees: self.rtpc_dependees,
        }
    }
}

/// What one ATL document declares.
#[derive(Default)]
struct ParsedAtl {
    triggers: Vec<AtlTrigger>,
    environments: Vec<AtlEnvironment>,
    rtpcs: Vec<AtlRtpc>,
}

fn parse_f32(value: Option<&str>) -> Option<f32> {
    value.and_then(|v| v.trim().parse::<f32>().ok())
}

/// Parse a CryXmlB ATL document.
fn parse_atl_cryxml(xml: &CryXml<'_>) -> ParsedAtl {
    let root = xml.root();
    let mut raw_triggers: Vec<RawTrigger> = Vec::new();
    let mut preload_banks: HashMap<String, String> = HashMap::new();
    let mut parsed = ParsedAtl::default();

    for section in xml.node_children(root) {
        match xml.node_tag(section) {
            "AudioTriggers" => {
                for trigger_node in xml.node_children(section) {
                    if xml.node_tag(trigger_node) != "ATLTrigger" {
                        continue;
                    }
                    let trigger_name = match cryxml_attr(xml, trigger_node, "atl_name") {
                        Some(n) => n.to_owned(),
                        None => continue,
                    };

                    let mut raw = RawTrigger::named(trigger_name);

                    for child in xml.node_children(trigger_node) {
                        match xml.node_tag(child) {
                            "WwiseEvent" => {
                                for (k, v) in xml.node_attributes(child) {
                                    raw.event_attr(k, v);
                                }
                            }
                            "RTPCDependees" => {
                                for dep in xml.node_children(child) {
                                    if let Some(v) = cryxml_attr(xml, dep, "atl_name") {
                                        raw.rtpc_dependees.push(v.to_owned());
                                    }
                                }
                            }
                            "ATLPreload" => {
                                if let Some(v) = cryxml_attr(xml, child, "atl_name") {
                                    raw.preload_name = v.to_owned();
                                }
                            }
                            _ => {}
                        }
                    }

                    raw_triggers.push(raw);
                }
            }
            "AudioPreloads" | "ATLPreloads" => {
                for preload_node in xml.node_children(section) {
                    if xml.node_tag(preload_node) != "ATLPreloadRequest" {
                        continue;
                    }
                    let preload_name = match cryxml_attr(xml, preload_node, "atl_name") {
                        Some(n) => n.to_owned(),
                        None => continue,
                    };

                    // WwiseFile can be nested under ATLConfigGroup or directly
                    if let Some(bank) = find_wwise_file_recursive(xml, preload_node) {
                        preload_banks.insert(preload_name.clone(), bank);
                    }
                }
            }
            "AudioEnvironments" => {
                for env in xml.node_children(section) {
                    if xml.node_tag(env) != "ATLEnvironment" {
                        continue;
                    }
                    let Some(atl_name) = cryxml_attr(xml, env, "atl_name") else { continue };
                    let aux = xml
                        .node_children(env)
                        .find(|c| xml.node_tag(c) == "WwiseAuxBus")
                        .and_then(|c| cryxml_attr(xml, c, "wwise_name"));
                    if let Some(aux) = aux {
                        parsed.environments.push(AtlEnvironment {
                            atl_name: atl_name.to_owned(),
                            aux_bus: aux.to_owned(),
                        });
                    }
                }
            }
            "AudioRTPCs" => {
                for rtpc in xml.node_children(section) {
                    if xml.node_tag(rtpc) != "ATLRtpc" {
                        continue;
                    }
                    let Some(atl_name) = cryxml_attr(xml, rtpc, "atl_name") else { continue };
                    if let Some(w) = xml.node_children(rtpc).find(|c| xml.node_tag(c) == "WwiseRtpc") {
                        parsed.rtpcs.push(AtlRtpc {
                            atl_name: atl_name.to_owned(),
                            wwise_name: cryxml_attr(xml, w, "wwise_name").unwrap_or(atl_name).to_owned(),
                            min: parse_f32(cryxml_attr(xml, w, "min")),
                            max: parse_f32(cryxml_attr(xml, w, "max")),
                            default: parse_f32(cryxml_attr(xml, w, "default")),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    parsed.triggers = raw_triggers.into_iter().map(|raw| raw.into_trigger(&preload_banks)).collect();
    parsed
}

// ── Plain XML fallback (quick-xml) ──────────────────────────────────────────

/// Parse a single plain-text ATL XML file.
fn parse_atl_xml(xml: &str) -> Result<ParsedAtl, BnkError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut raw_triggers: Vec<RawTrigger> = Vec::new();
    let mut preload_banks: HashMap<String, String> = HashMap::new();

    let mut inside_triggers = false;
    let mut current_trigger: Option<RawTrigger> = None;
    let mut current_preload_name: Option<String> = None;
    let mut inside_preloads = false;
    let mut text = TextState::default();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                text.open(e);
                handle_open_tag(
                    e,
                    false,
                    &mut inside_triggers,
                    &mut inside_preloads,
                    &mut current_trigger,
                    &mut current_preload_name,
                    &mut preload_banks,
                    &mut raw_triggers,
                );
            }
            Ok(Event::Empty(ref e)) => {
                text.open(e);
                text.close(e.local_name().as_ref());
                handle_open_tag(
                    e,
                    true,
                    &mut inside_triggers,
                    &mut inside_preloads,
                    &mut current_trigger,
                    &mut current_preload_name,
                    &mut preload_banks,
                    &mut raw_triggers,
                );
            }
            Ok(Event::End(ref e)) => {
                text.close(e.local_name().as_ref());
                let tag = e.local_name();
                match tag.as_ref() {
                    "AudioTriggers" => inside_triggers = false,
                    "ATLTrigger" => {
                        if let Some(trigger) = current_trigger.take() {
                            if !trigger.trigger_name.is_empty() {
                                raw_triggers.push(trigger);
                            }
                        }
                    }
                    "AudioPreloads" | "ATLPreloads" => inside_preloads = false,
                    "ATLPreloadRequest" => {
                        current_preload_name = None;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(BnkError::Xml(format!("{e}"))),
            _ => {}
        }
    }

    Ok(ParsedAtl {
        triggers: raw_triggers.into_iter().map(|raw| raw.into_trigger(&preload_banks)).collect(),
        environments: text.environments,
        rtpcs: text.rtpcs,
    })
}

/// Environments and RTPCs of a text document (the trigger state machine is `handle_open_tag`).
#[derive(Default)]
struct TextState {
    environment: Option<String>,
    rtpc: Option<String>,
    environments: Vec<AtlEnvironment>,
    rtpcs: Vec<AtlRtpc>,
}

impl TextState {
    fn open(&mut self, e: &quick_xml::events::BytesStart<'_>) {
        match e.local_name().as_ref() {
            "ATLEnvironment" => self.environment = get_attr(e, "atl_name"),
            "WwiseAuxBus" => {
                if let (Some(env), Some(aux)) = (&self.environment, get_attr(e, "wwise_name")) {
                    self.environments.push(AtlEnvironment { atl_name: env.clone(), aux_bus: aux });
                }
            }
            "ATLRtpc" => self.rtpc = get_attr(e, "atl_name"),
            "WwiseRtpc" => {
                if let Some(atl_name) = &self.rtpc {
                    self.rtpcs.push(AtlRtpc {
                        atl_name: atl_name.clone(),
                        wwise_name: get_attr(e, "wwise_name").unwrap_or_else(|| atl_name.clone()),
                        min: parse_f32(get_attr(e, "min").as_deref()),
                        max: parse_f32(get_attr(e, "max").as_deref()),
                        default: parse_f32(get_attr(e, "default").as_deref()),
                    });
                }
            }
            _ => {}
        }
    }

    fn close(&mut self, tag: &str) {
        match tag {
            "ATLEnvironment" => self.environment = None,
            "ATLRtpc" => self.rtpc = None,
            _ => {}
        }
    }
}

/// Process an opening or self-closing XML tag.
#[allow(clippy::too_many_arguments)]
fn handle_open_tag(
    e: &quick_xml::events::BytesStart<'_>,
    is_empty: bool,
    inside_triggers: &mut bool,
    inside_preloads: &mut bool,
    current_trigger: &mut Option<RawTrigger>,
    current_preload_name: &mut Option<String>,
    preload_banks: &mut HashMap<String, String>,
    raw_triggers: &mut Vec<RawTrigger>,
) {
    let tag = e.local_name();
    match tag.as_ref() {
        "AudioTriggers" => {
            *inside_triggers = true;
        }
        "ATLTrigger" => {
            if *inside_triggers {
                if let Some(name) = get_attr(e, "atl_name") {
                    let trigger = RawTrigger::named(name);
                    // If self-closing, push immediately; otherwise set as current.
                    if is_empty {
                        raw_triggers.push(trigger);
                    } else {
                        *current_trigger = Some(trigger);
                    }
                }
            }
        }
        "WwiseEvent" => {
            if let Some(trigger) = current_trigger {
                for attr in e.attributes().flatten() {
                    let (key, value): (&str, &str) = (attr.key.as_ref(), &attr.value);
                    trigger.event_attr(key, value);
                }
            }
        }
        "ATLRTPCDependee" => {
            if let (Some(trigger), Some(name)) = (current_trigger.as_mut(), get_attr(e, "atl_name")) {
                trigger.rtpc_dependees.push(name);
            }
        }
        "ATLPreload" => {
            if let Some(trigger) = current_trigger {
                if let Some(preload_name) = get_attr(e, "atl_name") {
                    trigger.preload_name = preload_name;
                }
            }
        }
        "AudioPreloads" | "ATLPreloads" => {
            *inside_preloads = true;
        }
        "ATLPreloadRequest" => {
            if *inside_preloads {
                *current_preload_name = get_attr(e, "atl_name");
            }
        }
        "WwiseFile" => {
            if let Some(preload_name) = current_preload_name {
                if let Some(wwise_name) = get_attr(e, "wwise_name") {
                    preload_banks.insert(preload_name.clone(), wwise_name);
                }
            }
        }
        _ => {}
    }
}
