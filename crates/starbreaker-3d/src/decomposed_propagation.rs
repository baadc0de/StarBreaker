//! Package audio propagation inputs pulled from DataCore placement classes and the archive's surface table.

use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use quick_xml::{Reader, XmlVersion};
use quick_xml::events::Event;
use serde::Serialize;
use starbreaker_datacore::{Database, types::CigGuid};
use starbreaker_p4k::MappedP4k;

use crate::types::InteriorMesh;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct RoomConnector {
    pub entity_id: String,
    pub name: Option<String>,
    /// Container-local transform, like its scene placement.
    pub transform: [[f32; 4]; 4],
    pub bounds: Option<[f32; 3]>,
    pub offset: Option<[f32; 3]>,
    pub door_entity_id: Option<String>,
    pub physical_occlusion_when_closed: Option<f32>,
}

pub(crate) fn room_connectors(db: &Database<'_>, meshes: &[InteriorMesh]) -> Vec<RoomConnector> {
    let mut classes = HashMap::new();
    meshes.iter().filter_map(|mesh| {
        let class = mesh.entity_class_guid.as_deref()?;
        let entity = &mesh.entity;
        let entity_id = entity.entity_id.clone()?;
        let facts = classes.entry(class).or_insert_with(|| {
            let id = CigGuid::from_str(class).ok()?;
            let record = db.record_by_id(&id)?;
            starbreaker_wwise::datacore_effects::room_connector(db, record)
        }).as_ref()?;
        Some(RoomConnector {
            door_entity_id: facts.is_door.then(|| entity_id.clone()),
            entity_id,
            name: entity.entity_name.clone(),
            transform: mesh.transform,
            bounds: entity.overrides.room_connector_bounds.or(facts.bounds),
            offset: entity.overrides.room_connector_offset.or(facts.offset),
            physical_occlusion_when_closed: entity.overrides.room_connector_closed_occlusion.or(facts.physical_occlusion_when_closed),
        })
    }).collect()
}

pub(crate) fn surface_obstructions(p4k: &MappedP4k) -> Option<BTreeMap<String, f32>> {
    let entry = p4k.entry_case_insensitive(r"Data\Libs\MaterialEffects\SurfaceTypes.xml")?;
    let data = p4k.read(entry).ok()?;
    let xml = if starbreaker_cryxml::is_cryxmlb(&data) {
        std::borrow::Cow::Owned(starbreaker_cryxml::from_bytes(&data).ok()?.to_string())
    } else {
        std::borrow::Cow::Borrowed(std::str::from_utf8(&data).ok()?)
    };
    match parse_surface_obstructions(&xml) {
        Ok(table) => Some(table),
        Err(error) => {
            log::warn!("[audio] could not parse SurfaceTypes.xml: {error}");
            None
        }
    }
}

fn parse_surface_obstructions(xml: &str) -> Result<BTreeMap<String, f32>, quick_xml::Error> {
    let mut reader = Reader::from_str(xml);
    let mut surfaces = BTreeMap::new();
    let mut surface = None;
    loop {
        match reader.read_event()? {
            Event::Start(ref tag) if tag.name().as_ref() == "SurfaceType" => {
                surface = tag.try_get_attribute("name")?
                    .map(|a| a.normalized_value(XmlVersion::Implicit1_0).map(|v| v.into_owned()))
                    .transpose()?;
            }
            Event::Start(ref tag) | Event::Empty(ref tag) if tag.name().as_ref() == "Physics" => {
                if let (Some(name), Some(value)) = (&surface, tag.try_get_attribute("sound_obstruction")?) {
                    let value = value.normalized_value(XmlVersion::Implicit1_0)?;
                    if let Some(value) = value.parse::<f32>().ok().filter(|v| v.is_finite()) {
                        surfaces.insert(name.clone(), value);
                    }
                }
            }
            Event::End(tag) if tag.name().as_ref() == "SurfaceType" => surface = None,
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(surfaces)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_surface_physics_with_explicit_obstruction_is_exported() {
        let xml = r#"<SurfaceTypes>
            <SurfaceType name="synthetic_metal"><Physics sound_obstruction="0.5"/></SurfaceType>
            <SurfaceType name="omitted"><Physics/></SurfaceType>
            <Physics sound_obstruction="1.0"/>
        </SurfaceTypes>"#;
        assert_eq!(parse_surface_obstructions(xml).unwrap(),
            BTreeMap::from([("synthetic_metal".to_owned(), 0.5)]));
    }
}
