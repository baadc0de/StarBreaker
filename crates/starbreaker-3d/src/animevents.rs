//! `.animevents` (a rig's `$AnimEventDatabase`): the events each animation fires at a normalised time.
//!
//! ```xml
//! <anim_event_list>
//!   <animation name="animations/.../door_open.caf">
//!     <event name="audio_trigger" time="0.0001" endTime="0.0001" parameter="Play_Door_Open" bone="Door" offset="0,0,0" dir="0,0,0" />
//!     <event name="audio_trigger_reverse" time="0.9999" parameter="Play_Door_Close" bone="Door" />
//!   </animation>
//! </anim_event_list>
//! ```
//!
//! [`parse_animevents`] reads every event; [`audio_events`] keeps the audio ones as the clip-source contract's
//! `audio_events[]`: `audio_trigger` fires when the clip plays forward over `time`, `audio_trigger_reverse` when it plays
//! backward over it, `audio_trigger_bothdir` either way.

use std::collections::HashMap;

use serde::Serialize;
use starbreaker_cryxml::{CryXml, CryXmlNode};

use crate::error::Error;

/// One authored animation event.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimEvent {
    pub name: String,
    pub time: f32,
    pub end_time: f32,
    pub parameter: String,
    pub bone: String,
    pub offset: [f32; 3],
}

/// An audio event of a clip source (`audio_events[]`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AudioEvent {
    /// Normalised clip time.
    pub time: f32,
    /// `forward`, `reverse` or `both`.
    pub dir: &'static str,
    pub trigger: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bone: String,
    pub offset: [f32; 3],
}

/// Comparison key of an animation path (lower case, forward slashes).
pub fn animation_key(path: &str) -> String {
    path.trim().replace('\\', "/").to_ascii_lowercase()
}

fn attr<'a>(xml: &'a CryXml, node: &CryXmlNode, name: &str) -> Option<&'a str> {
    xml.node_attributes(node).find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v)
}

fn vec3(text: Option<&str>) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (slot, v) in out.iter_mut().zip(text.unwrap_or("").split(',')) {
        *slot = v.trim().parse().unwrap_or(0.0);
    }
    out
}

/// Every event of an `.animevents` file, by [`animation_key`] of its animation.
pub fn parse_animevents(data: &[u8]) -> Result<HashMap<String, Vec<AnimEvent>>, Error> {
    let xml = starbreaker_cryxml::from_bytes(data).map_err(|e| Error::Other(format!("animevents: {e:?}")))?;
    let mut out: HashMap<String, Vec<AnimEvent>> = HashMap::new();
    for animation in xml.node_children(xml.root()) {
        if !xml.node_tag(animation).eq_ignore_ascii_case("animation") {
            continue;
        }
        let Some(name) = attr(&xml, animation, "name") else { continue };
        let events = out.entry(animation_key(name)).or_default();
        for event in xml.node_children(animation) {
            if !xml.node_tag(event).eq_ignore_ascii_case("event") {
                continue;
            }
            let num = |k| attr(&xml, event, k).and_then(|v| v.trim().parse::<f32>().ok());
            let time = num("time").unwrap_or(0.0);
            events.push(AnimEvent {
                name: attr(&xml, event, "name").unwrap_or_default().to_owned(),
                time,
                end_time: num("endTime").unwrap_or(time),
                parameter: attr(&xml, event, "parameter").unwrap_or_default().to_owned(),
                bone: attr(&xml, event, "bone").unwrap_or_default().to_owned(),
                offset: vec3(attr(&xml, event, "offset")),
            });
        }
    }
    Ok(out)
}

/// The audio events among `events` (an empty trigger is no event).
pub fn audio_events(events: &[AnimEvent]) -> Vec<AudioEvent> {
    events
        .iter()
        .filter_map(|e| {
            let dir = match e.name.to_ascii_lowercase().as_str() {
                "audio_trigger" => "forward",
                "audio_trigger_reverse" => "reverse",
                "audio_trigger_bothdir" => "both",
                _ => return None,
            };
            (!e.parameter.trim().is_empty()).then(|| AudioEvent {
                time: e.time,
                dir,
                trigger: e.parameter.trim().to_owned(),
                bone: e.bone.clone(),
                offset: e.offset,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<anim_event_list>
  <animation name="Animations/Synthetic/door_open.caf">
    <event name="audio_trigger" time="9.9999997e-05" endTime="9.9999997e-05" parameter="Play_Synthetic_Open" bone="Door" offset="0,0.5,0" dir="0,0,0" model="" seekTime="0" />
    <event name="audio_trigger_reverse" time="0.99989998" endTime="0.99989998" parameter="Play_Synthetic_Close" bone="Door" offset="0,0,0" />
    <event name="audio_trigger_bothdir" time="0.5" parameter="Play_Synthetic_Mid" bone="" />
    <event name="footstep" time="0.25" parameter="" bone="LeftFoot" />
    <event name="audio_trigger" time="0.75" parameter="" />
  </animation>
</anim_event_list>"#;

    #[test]
    fn audio_events_carry_time_direction_trigger_and_bone() {
        let all = parse_animevents(&crate::test_xml::encode(DOC)).unwrap();
        let events = &all[&animation_key("animations/synthetic/DOOR_OPEN.caf")];
        assert_eq!(events.len(), 5);
        let audio = audio_events(events);
        assert_eq!(audio.len(), 3, "footsteps and empty triggers are not audio events");
        assert_eq!((audio[0].dir, audio[0].trigger.as_str(), audio[0].bone.as_str()), ("forward", "Play_Synthetic_Open", "Door"));
        assert_eq!(audio[0].offset, [0.0, 0.5, 0.0]);
        assert!((audio[0].time - 0.0001).abs() < 1e-6);
        assert_eq!((audio[1].dir, audio[1].trigger.as_str()), ("reverse", "Play_Synthetic_Close"));
        assert_eq!(audio[2].dir, "both");
    }
}
