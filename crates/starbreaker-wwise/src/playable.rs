//! Trigger → playable tree: what the game's Wwise data does when an ATL trigger is posted, as data a host player runs.
//!
//! [`Resolver::trigger`] follows ATL trigger → Wwise event (FNV-1 of the lower-case name) → its actions. A Play action's
//! target becomes a tree of [`PlayNode`]s: random and sequence containers (weights, avoid-repeat, loop count, step or
//! continuous play, transition mode and time), blend containers (every child; RTPC crossfade layers kept as curves),
//! switch containers (the default branch) and sound leaves (media id, loop count, source plugin). Each node keeps its
//! **own** properties (volume, pitch, filters, aux-send and make-up gains, ranged random modifiers, RTPC curves): Wwise
//! sums them down the tree, and so must the player. What Wwise inherits by override instead — the output bus,
//! positioning and its attenuation, the aux-send routing — is resolved on every node by walking `direct_parent_id` up
//! to the nearest node that overrides it ([`Effective`]). The target's ancestors above the tree contribute through the
//! action's [`PlayAction::context`].
//!
//! Node ids are global in a Wwise project; objects are looked up in the event's bank, then `Init.bnk`.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde::Serialize;

use crate::hirc::action::ActionSpecificParams;
use crate::hirc::node_base::NodeBaseParams;
use crate::hirc::props::prop_id;
use crate::hirc::rtpc::InitialRtpc;
use crate::hirc::HircObject;
use crate::library::{AudioLibrary, LoadedBank};

/// One RTPC curve on a node or bus: game parameter → property.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RtpcCurve {
    /// The game parameter's ATL name when known, else its id in hex.
    pub rtpc: String,
    pub rtpc_id: u32,
    /// 0 game parameter, 1 MIDI, 2 switch, 3 state, 4 modulator (`AkGameSyncType`).
    pub rtpc_type: u8,
    /// The driven property (`prop_id::rtpc_param_name`), or `param_<id>`.
    pub param: String,
    /// `AkCurveScaling`: 0 none, 2 dB, 3 log, 4 dB-to-linear.
    pub scaling: u8,
    /// `(x, y, interpolation)`; `AkCurveInterpolation`: 0 log3, 1 sine, 2 log1, 3 inv-S, 4 linear, 5 S, 6 exp1,
    /// 7 sine-recip, 8 exp3, 9 constant.
    pub points: Vec<(f32, f32, u32)>,
}

/// A node's own property values (Wwise adds them along the tree).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Props {
    #[serde(skip_serializing_if = "is_zero")]
    pub volume_db: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub pitch_cents: f32,
    /// Wwise low-pass / high-pass amount, 0..100 (not Hz).
    #[serde(skip_serializing_if = "is_zero")]
    pub lpf: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub hpf: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub make_up_gain_db: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub game_aux_send_db: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub output_bus_volume_db: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub initial_delay_s: f32,
    /// Ranged modifiers: `(property, min, max)` added to the base value at each play.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub random: Vec<(String, f32, f32)>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rtpc: Vec<RtpcCurve>,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

/// What a node inherits by override (nearest node up the chain that sets it).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Effective {
    /// Output bus id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bus: Option<u32>,
    /// 3D (listener-relative) positioning.
    pub spatial: bool,
    /// Attenuation object id (when attenuation is enabled).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attenuation: Option<u32>,
    /// Game-defined aux sends (the game's environment sends) are used.
    pub use_game_aux: bool,
    /// User-defined aux buses.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub user_aux: Vec<u32>,
}

/// The kind of a play node and its own playback rules.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeKind {
    Sound {
        #[serde(skip_serializing_if = "Option::is_none")]
        media: Option<u32>,
        /// Source plugin of a non-media sound (`silence`, `sine`, `tone`, or the id in hex).
        #[serde(skip_serializing_if = "Option::is_none")]
        source_plugin: Option<String>,
        /// Plays this many times; 0 loops forever.
        loop_count: u32,
        language_specific: bool,
    },
    Random {
        weights: Vec<f32>,
        avoid_repeat: u16,
        shuffle: bool,
        #[serde(flatten)]
        play: ContainerPlay,
    },
    Sequence {
        restart_backward: bool,
        #[serde(flatten)]
        play: ContainerPlay,
    },
    Blend {
        /// RTPC crossfade layers: per layer, the RTPC and each child's volume curve over it.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        layers: Vec<BlendLayer>,
    },
    /// The default branch of a switch / state group plays (the game's switch values are not modelled).
    Switch { group_type: u8, group_id: u32, default_switch: u32 },
    /// An actor-mixer reached as a target (plays every child, like a blend).
    Mixer,
}

/// Random / sequence container play rules.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ContainerPlay {
    /// Continuous: the container plays its items one after another (with `transition`); step: one item per play.
    pub continuous: bool,
    /// Loops of the continuous playlist; 0 loops forever.
    pub loop_count: u16,
    pub reset_each_play: bool,
    /// `AkTransitionMode`: disabled, crossfade_amp, crossfade_power, delay, sample_accurate, trigger_rate.
    pub transition: &'static str,
    pub transition_s: f32,
    pub transition_random_s: (f32, f32),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BlendLayer {
    pub rtpc: String,
    /// `(child index into the node's children, volume curve points)`.
    pub children: Vec<(usize, Vec<(f32, f32, u32)>)>,
}

/// A node of a playable tree.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlayNode {
    pub id: u32,
    #[serde(flatten)]
    pub kind: NodeKind,
    /// Indices into [`Playables::nodes`], in playlist order for random / sequence containers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<usize>,
    #[serde(flatten)]
    pub props: Props,
    #[serde(flatten)]
    pub effective: Effective,
}

/// A Play action: the tree root and what the target's ancestors add.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlayAction {
    pub root: usize,
    pub delay_s: f32,
    pub fade_in_s: f32,
    /// The ancestors of the root (actor-mixers above the target) summed: volume, pitch, filters and gains add; their
    /// random ranges and RTPC curves are listed.
    pub context: Props,
}

/// A Stop (or other non-play) action.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OtherAction {
    /// `action_type` (0x01xx stop, 0x02xx pause, 0x12xx set state, 0x19xx set switch, …).
    pub action_type: u16,
    pub target: u32,
    pub delay_s: f32,
    pub fade_s: f32,
}

/// One trigger, resolved.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TriggerOut {
    pub event: String,
    pub bank: String,
    pub duration_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_s: Option<(f32, f32)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius_max: Option<f32>,
    /// `playable`, `no_media` (only stops, source plugins or missing media), `event_missing`, `bank_missing`,
    /// `not_in_atl`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub play: Vec<PlayAction>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub other: Vec<OtherAction>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rtpc_dependees: Vec<String>,
}

/// Triggers resolved into one shared node table.
#[derive(Debug, Default, Serialize)]
pub struct Playables {
    pub triggers: BTreeMap<String, TriggerOut>,
    pub nodes: Vec<PlayNode>,
    /// Every media leaf: media id → the bank whose HIRC references it (to find embedded media).
    #[serde(skip)]
    pub media_homes: BTreeMap<u32, (String, bool)>,
    /// Attenuation and bus ids met (for the manifest's tables).
    #[serde(skip)]
    pub attenuations: BTreeMap<u32, String>,
    #[serde(skip)]
    pub buses: BTreeMap<u32, String>,
}

/// Resolves triggers against an [`AudioLibrary`].
pub struct Resolver<'l, 'p> {
    pub lib: &'l AudioLibrary<'p>,
    pub out: Playables,
    /// FNV-1 hash of a Wwise game-parameter name → its ATL name.
    rtpc_names: HashMap<u32, String>,
    node_index: HashMap<u32, usize>,
}

const MAX_DEPTH: usize = 32;

impl<'l, 'p> Resolver<'l, 'p> {
    pub fn new(lib: &'l AudioLibrary<'p>) -> Self {
        let rtpc_names = lib.atl.rtpcs().map(|r| (crate::fnv1_hash(&r.wwise_name), r.atl_name.clone())).collect();
        Self { lib, out: Playables::default(), rtpc_names, node_index: HashMap::new() }
    }

    /// The ATL name of a game parameter hash, or its id in hex.
    pub fn rtpc_name(&self, id: u32) -> String {
        self.rtpc_names.get(&id).cloned().unwrap_or_else(|| format!("{id:#010x}"))
    }

    /// Resolve `name` (once) into [`Playables::triggers`].
    pub fn trigger(&mut self, name: &str) -> &TriggerOut {
        if !self.out.triggers.contains_key(name) {
            let resolved = self.resolve(name);
            self.out.triggers.insert(name.to_owned(), resolved);
        }
        &self.out.triggers[name]
    }

    fn resolve(&mut self, name: &str) -> TriggerOut {
        let Some(atl) = self.lib.atl.get_trigger(name).cloned() else {
            return TriggerOut {
                event: String::new(),
                bank: String::new(),
                duration_type: String::new(),
                duration_s: None,
                radius_max: None,
                status: "not_in_atl",
                reason: Some("no ATLTrigger of this name".into()),
                play: Vec::new(),
                other: Vec::new(),
                rtpc_dependees: Vec::new(),
            };
        };
        let mut out = TriggerOut {
            event: atl.wwise_event_name.clone(),
            bank: atl.bank_name.clone(),
            duration_type: atl.duration_type.clone(),
            duration_s: atl.duration_min.zip(atl.duration_max),
            radius_max: atl.radius_max,
            status: "playable",
            reason: None,
            play: Vec::new(),
            other: Vec::new(),
            rtpc_dependees: atl.rtpc_dependees.clone(),
        };
        let Some(bank) = self.lib.bank(&atl.bank_name) else {
            out.status = "bank_missing";
            out.reason = Some(format!("bank {} not in the archive", atl.bank_name));
            return out;
        };
        let mut banks = vec![bank.clone()];
        if let Some(init) = self.lib.init_bank() {
            banks.push(init);
        }
        let event_name = if atl.wwise_event_name.is_empty() { name } else { &atl.wwise_event_name };
        let Some(HircObject::Event(event)) = self.lib.object(&banks, crate::fnv1_hash(event_name)) else {
            out.status = "event_missing";
            out.reason = Some(format!("event {event_name} not in {}", atl.bank_name));
            return out;
        };
        for &action_id in &event.action_ids {
            let Some(HircObject::Action(action)) = self.lib.object(&banks, action_id) else { continue };
            let ms = |id: u8| action.properties.u32(id).map(|v| v as f32 / 1000.0).unwrap_or(0.0);
            if let ActionSpecificParams::Play { .. } = action.specific_params {
                if let Some(root) = self.node(&banks, &bank, action.target_id, 0) {
                    let context = self.context(&banks, action.target_id);
                    out.play.push(PlayAction {
                        root,
                        delay_s: ms(prop_id::DELAY_TIME),
                        fade_in_s: ms(prop_id::TRANSITION_TIME),
                        context,
                    });
                }
            } else {
                out.other.push(OtherAction {
                    action_type: action.action_type,
                    target: action.target_id,
                    delay_s: ms(prop_id::DELAY_TIME),
                    fade_s: ms(prop_id::TRANSITION_TIME),
                });
            }
        }
        let has_media = out.play.iter().any(|a| self.subtree_has_media(a.root));
        if !has_media {
            out.status = "no_media";
            out.reason = Some(if out.play.is_empty() {
                "no play action with a resolvable target".into()
            } else {
                "the played tree holds no media (source plugins or media missing from the archive)".into()
            });
        }
        out
    }

    fn subtree_has_media(&self, index: usize) -> bool {
        let node = &self.out.nodes[index];
        matches!(node.kind, NodeKind::Sound { media: Some(_), .. })
            || node.children.iter().any(|&c| self.subtree_has_media(c))
    }

    /// The node table index of object `id` (resolving its subtree on first use).
    fn node(&mut self, banks: &[Arc<LoadedBank>], home: &Arc<LoadedBank>, id: u32, depth: usize) -> Option<usize> {
        if let Some(&index) = self.node_index.get(&id) {
            return Some(index);
        }
        if depth > MAX_DEPTH {
            return None;
        }
        let object = self.lib.object(banks, id)?.clone();
        let base = node_base(&object)?;
        let (kind, child_ids) = match &object {
            HircObject::Sound(s) => {
                let plugin_type = s.plugin_id & 0x0F;
                let media = (plugin_type == 1 && self.lib.media_exists(s.media_id, Some(home))).then_some(s.media_id);
                if let Some(m) = media {
                    self.out.media_homes.entry(m).or_insert_with(|| (home.name.clone(), s.is_language_specific()));
                }
                let source_plugin = (plugin_type != 1).then(|| source_plugin_name(s.plugin_id));
                let loop_count = s.node_base.properties.u32(prop_id::LOOP).unwrap_or(1);
                let language_specific = s.is_language_specific();
                (NodeKind::Sound { media, source_plugin, loop_count, language_specific }, Vec::new())
            }
            HircObject::RanSeqContainer(c) => {
                let ids: Vec<u32> = if c.playlist.is_empty() {
                    c.children.clone()
                } else {
                    c.playlist.iter().map(|p| p.play_id).collect()
                };
                let play = ContainerPlay {
                    continuous: c.flags & 0x08 != 0,
                    loop_count: c.loop_count,
                    reset_each_play: c.flags & 0x02 != 0,
                    transition: transition_name(c.transition_mode),
                    transition_s: c.transition_time / 1000.0,
                    transition_random_s: (c.transition_time_mod_min / 1000.0, c.transition_time_mod_max / 1000.0),
                };
                let kind = if c.mode == 1 {
                    NodeKind::Sequence { restart_backward: c.flags & 0x04 != 0, play }
                } else {
                    let weights = if c.playlist.is_empty() {
                        vec![1.0; ids.len()]
                    } else {
                        c.playlist.iter().map(|p| p.weight as f32 / 1000.0).collect()
                    };
                    NodeKind::Random { weights, avoid_repeat: c.avoid_repeat_count, shuffle: c.random_mode == 1, play }
                };
                (kind, ids)
            }
            HircObject::BlendContainer(b) => {
                let layers = b
                    .layers
                    .iter()
                    .map(|l| BlendLayer {
                        rtpc: self.rtpc_name(l.rtpc_id),
                        children: l
                            .associations
                            .iter()
                            .filter_map(|a| {
                                let i = b.children.iter().position(|&c| c == a.child_id)?;
                                Some((i, a.curve.iter().map(|p| (p.from, p.to, p.interp)).collect()))
                            })
                            .collect(),
                    })
                    .collect();
                (NodeKind::Blend { layers }, b.children.clone())
            }
            HircObject::SwitchContainer(s) => {
                let ids = s
                    .switch_groups
                    .iter()
                    .find(|g| g.switch_id == s.default_switch)
                    .map(|g| g.node_ids.clone())
                    .unwrap_or_default();
                (NodeKind::Switch { group_type: s.group_type, group_id: s.group_id, default_switch: s.default_switch }, ids)
            }
            HircObject::ActorMixer(m) => (NodeKind::Mixer, m.children.clone()),
            _ => return None,
        };
        let props = self.props(&base.clone());
        let effective = self.effective(banks, id);
        if let Some(a) = effective.attenuation {
            self.out.attenuations.entry(a).or_insert_with(|| home.name.clone());
        }
        if let Some(b) = effective.bus {
            self.out.buses.entry(b).or_insert_with(|| home.name.clone());
        }
        let index = self.out.nodes.len();
        self.node_index.insert(id, index);
        self.out.nodes.push(PlayNode { id, kind, children: Vec::new(), props, effective });
        let children: Vec<usize> = child_ids.iter().filter_map(|&c| self.node(banks, home, c, depth + 1)).collect();
        self.out.nodes[index].children = children;
        Some(index)
    }

    /// A node's own properties.
    fn props(&self, base: &NodeBaseParams) -> Props {
        let p = &base.properties;
        let f = |id| p.f32(id).unwrap_or(0.0);
        let random = base
            .ranged_modifiers
            .modifiers
            .iter()
            .map(|m| (prop_id::rtpc_param_name(u32::from(m.id)).map_or_else(|| format!("prop_{:#04x}", m.id), str::to_owned), m.min_f32(), m.max_f32()))
            .collect();
        Props {
            volume_db: f(prop_id::VOLUME),
            pitch_cents: f(prop_id::PITCH),
            lpf: f(prop_id::LPF),
            hpf: f(prop_id::HPF),
            make_up_gain_db: f(prop_id::MAKE_UP_GAIN),
            game_aux_send_db: f(prop_id::GAME_AUX_SEND_VOLUME),
            output_bus_volume_db: f(prop_id::OUTPUT_BUS_VOLUME),
            initial_delay_s: f(prop_id::INITIAL_DELAY),
            random,
            rtpc: self.curves(&base.rtpc),
        }
    }

    pub fn curves(&self, rtpc: &InitialRtpc) -> Vec<RtpcCurve> {
        rtpc.curves
            .iter()
            .map(|c| RtpcCurve {
                rtpc: self.rtpc_name(c.rtpc_id),
                rtpc_id: c.rtpc_id,
                rtpc_type: c.rtpc_type,
                param: prop_id::rtpc_param_name(c.param_id).map_or_else(|| format!("param_{}", c.param_id), str::to_owned),
                scaling: c.scaling,
                points: c.points.iter().map(|p| (p.from, p.to, p.interp)).collect(),
            })
            .collect()
    }

    /// The parent chain of `id` (excluding it), nearest first.
    fn ancestors<'b>(&self, banks: &'b [Arc<LoadedBank>], id: u32) -> Vec<&'b HircObject> {
        let mut chain = Vec::new();
        let mut current = self.lib.object(banks, id).and_then(node_base).map(|b| b.direct_parent_id);
        while let Some(parent) = current.filter(|&p| p != 0) {
            let Some(object) = self.lib.object(banks, parent) else { break };
            chain.push(object);
            if chain.len() > MAX_DEPTH {
                break;
            }
            current = node_base(object).map(|b| b.direct_parent_id);
        }
        chain
    }

    /// Sum of the ancestors' own properties (the target's context).
    fn context(&self, banks: &[Arc<LoadedBank>], id: u32) -> Props {
        let mut sum = Props::default();
        for object in self.ancestors(banks, id) {
            let Some(base) = node_base(object) else { continue };
            let p = self.props(base);
            sum.volume_db += p.volume_db;
            sum.pitch_cents += p.pitch_cents;
            sum.lpf += p.lpf;
            sum.hpf += p.hpf;
            sum.make_up_gain_db += p.make_up_gain_db;
            sum.game_aux_send_db += p.game_aux_send_db;
            sum.output_bus_volume_db += p.output_bus_volume_db;
            sum.initial_delay_s += p.initial_delay_s;
            sum.random.extend(p.random);
            sum.rtpc.extend(p.rtpc);
        }
        sum
    }

    /// The inherited-by-override settings of `id`: bus, positioning and attenuation, aux routing.
    fn effective(&self, banks: &[Arc<LoadedBank>], id: u32) -> Effective {
        let mut chain: Vec<&HircObject> = self.lib.object(banks, id).into_iter().collect();
        chain.extend(self.ancestors(banks, id));
        let bases: Vec<&NodeBaseParams> = chain.iter().filter_map(|o| node_base(o)).collect();
        let mut out = Effective {
            bus: bases.iter().map(|b| b.override_bus_id).find(|&b| b != 0),
            ..Effective::default()
        };
        if let Some(pos) = bases.iter().find(|b| b.positioning.bits_positioning & 0x01 != 0) {
            out.spatial = pos.positioning.bits_positioning & 0x02 != 0;
            let attenuation_enabled = pos.positioning.bits_3d.is_none_or(|bits| bits & 0x08 != 0);
            if out.spatial && attenuation_enabled {
                out.attenuation = bases.iter().find_map(|b| b.properties.u32(prop_id::ATTENUATION_ID)).filter(|&a| a != 0);
            }
        }
        if let Some(aux) = bases.iter().find(|b| b.aux_params.flags & 0x01 != 0) {
            out.use_game_aux = aux.aux_params.flags & 0x02 != 0;
        }
        if let Some(aux) = bases.iter().find(|b| b.aux_params.flags & 0x04 != 0) {
            out.user_aux = aux.aux_params.aux_bus_ids.iter().copied().filter(|&a| a != 0).collect();
        }
        out
    }
}

/// The node base of a container or sound.
pub(crate) fn node_base(object: &HircObject) -> Option<&NodeBaseParams> {
    match object {
        HircObject::Sound(s) => Some(&s.node_base),
        HircObject::RanSeqContainer(c) => Some(&c.node_base),
        HircObject::SwitchContainer(c) => Some(&c.node_base),
        HircObject::ActorMixer(c) => Some(&c.node_base),
        HircObject::BlendContainer(c) => Some(&c.node_base),
        _ => None,
    }
}

fn transition_name(mode: u8) -> &'static str {
    match mode {
        0 => "disabled",
        1 => "crossfade_amp",
        2 => "crossfade_power",
        3 => "delay",
        4 => "sample_accurate",
        5 => "trigger_rate",
        _ => "unknown",
    }
}

/// Audiokinetic source plugins (company 0, type 2): 100 silence, 101 sine, 102 tone generator.
fn source_plugin_name(plugin_id: u32) -> String {
    match plugin_id {
        0x0064_0002 => "silence".into(),
        0x0065_0002 => "sine".into(),
        0x0066_0002 => "tone".into(),
        other => format!("{other:#010x}"),
    }
}

#[cfg(test)]
#[path = "playable_tests.rs"]
mod tests;
