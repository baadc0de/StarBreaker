//! Socpak interior animation export: promotes animated interior entities to
//! decomposed `children` so their skeletal `.caf` clips are extracted.
//!
//! Hangar doors/elevators are `.cga` (animated geometry) with a sibling
//! `.chrparams` referencing `.caf` clips — the same mechanism ships use. The
//! socpak interior path emits them as static placements, so their animations are
//! never extracted. [`extract_animated_interior_children`] detects them, loads
//! them as [`EntityPayload`] children (carrying `skeleton_source_path`), and
//! removes the static duplicates so the existing decomposed animation pipeline
//! produces their clips.

use starbreaker_datacore::database::Database;
use starbreaker_datacore::types::{Record, StringId, StringId2};
use starbreaker_p4k::MappedP4k;

use crate::types::{EntityPayload, ResolvedNode};

use super::{
    datacore_path_to_p4k, load_child_payload_asset, ExportOptions, LoadedChildPayload,
    LoadedInteriors,
};

/// The sibling `.chrparams` path for an animated `.cga` geometry, or `None` for
/// static `.cgf` (or non-geometry) paths. Case of the stem is preserved.
pub(crate) fn chrparams_path_for_cga(cgf_path: &str) -> Option<String> {
    if cgf_path.to_ascii_lowercase().ends_with(".cga") {
        let stem = &cgf_path[..cgf_path.len() - ".cga".len()];
        Some(format!("{stem}.chrparams"))
    } else {
        None
    }
}

/// Synthetic placeholder DataCore record for an interior geometry that has no
/// entity record (socpak `IncludedObjects` are raw geometry placements).
/// `load_child_payload_asset` only falls back to the record when `geometry_path`
/// fails to load, so an empty record is sufficient.
fn placeholder_record() -> Record {
    Record {
        name_offset: StringId2(-1),
        file_name_offset: StringId(0),
        tag_offset: StringId2(-1),
        struct_index: 0,
        id: starbreaker_common::CigGuid::EMPTY,
        instance_index: 0,
        struct_size: 0,
    }
}

/// The Mannequin controller of the entity class `guid` names (`None` without one).
fn class_animation_controller(db: &Database, guid: &str) -> Option<crate::animation::AnimationControllerSource> {
    let guid = <starbreaker_common::CigGuid as std::str::FromStr>::from_str(guid).ok()?;
    let record = db.record_by_id(&guid)?;
    super::query_animation_controller_source(db, record).filter(|source| !source.animation_database.is_empty())
}

/// Within each group of equal names, names whose controllers differ get the
/// controller's database and controller file stems appended (`<name>__<adb>_<controller>`;
/// `<name>__none` without one); a group with one controller keeps its name.
fn namespace_by_controller(names: &mut [String], controllers: &[Option<crate::animation::AnimationControllerSource>]) {
    let key = |c: &Option<crate::animation::AnimationControllerSource>| {
        c.as_ref().map(|c| (c.animation_database.clone(), c.animation_controller.clone()))
    };
    let stem = |path: &str| {
        let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
        base.rsplit_once('.').map_or(base, |(s, _)| s).to_string()
    };
    let mut distinct: std::collections::HashMap<String, std::collections::BTreeSet<Option<(String, String)>>> =
        std::collections::HashMap::new();
    for (name, controller) in names.iter().zip(controllers) {
        distinct.entry(name.clone()).or_default().insert(key(controller));
    }
    for (name, controller) in names.iter_mut().zip(controllers) {
        if distinct.get(name.as_str()).is_none_or(|set| set.len() < 2) {
            continue;
        }
        *name = match key(controller) {
            Some((adb, ctl)) => format!("{name}__{}_{}", stem(&adb), stem(&ctl)),
            None => format!("{name}__none"),
        };
    }
}

fn cga_entity_name(cgf_path: &str) -> String {
    let base = cgf_path.rsplit(['/', '\\']).next().unwrap_or(cgf_path);
    base.get(..base.len().saturating_sub(".cga".len())).unwrap_or(base).to_string()
}

/// Detect animated `.cga` interior entities (those with a sibling `.chrparams`
/// in the P4k), load each as an `EntityPayload` child with its
/// `skeleton_source_path` set, and remove the static placements that became
/// children (so geometry isn't duplicated). Returns the animated children to add
/// to the decomposed `DecomposedInput.children`.
pub(crate) fn extract_animated_interior_children(
    db: &Database,
    p4k: &MappedP4k,
    interiors: &mut LoadedInteriors,
    opts: &ExportOptions,
) -> Vec<EntityPayload> {
    use rayon::prelude::*;

    // Snapshot per-CGF info so the container loop can mutate placements while
    // still reading the shared unique-CGF table.
    let cgf_info: Vec<(String, Option<String>)> = interiors
        .unique_cgfs
        .iter()
        .map(|e| (e.cgf_path.clone(), e.material_path.clone()))
        .collect();
    let cgf_controllers: Vec<Option<crate::animation::AnimationControllerSource>> =
        interiors.unique_cgfs.iter().map(|e| e.animation_controller.clone()).collect();

    // Phase 1: detect animated placements and remove them from the static
    // interior placements (de-dup geometry). Records each placement's world
    // transform + per-object palette and an index into `unique_keys`; geometry,
    // materials and textures are loaded once per unique (geometry, material) pair
    // in phase 2 rather than once per placement (a hangar repeats the same door /
    // elevator dozens of times, and TEX0 texture decode dominates the load).
    struct AnimPlacement {
        unique_index: usize,
        offset_position: [f32; 3],
        offset_rotation: [f32; 3],
        palette: Option<crate::mtl::TintPalette>,
        entity_name: String,
        // The placement's identity, kept by the child (a hangar door stays that door entity).
        container_index: usize,
        transform: [[f32; 4]; 4],
        identity: crate::types::PlacementIdentity,
        ui_bindings: Vec<crate::types::UiBinding>,
        controller: Option<crate::animation::AnimationControllerSource>,
    }
    let mut unique_keys: Vec<(String, Option<String>)> = Vec::new();
    let mut key_index: std::collections::HashMap<(String, Option<String>), usize> =
        std::collections::HashMap::new();
    let mut anim_placements: Vec<AnimPlacement> = Vec::new();

    for (container_index, container) in interiors.containers.iter_mut().enumerate() {
        let container_mat = glam::Mat4::from_cols_array_2d(&container.container_transform);
        let mut kept = Vec::with_capacity(container.placements.len());
        for placement in std::mem::take(&mut container.placements) {
            let Some((cgf_path, material_path)) = cgf_info.get(placement.mesh_index).cloned() else {
                kept.push(placement);
                continue;
            };
            let Some(chrparams) = chrparams_path_for_cga(&cgf_path) else {
                kept.push(placement);
                continue;
            };
            if p4k
                .entry_case_insensitive(&datacore_path_to_p4k(&chrparams))
                .is_none()
            {
                kept.push(placement);
                continue;
            }

            // Animated: place at the placement's world transform decomposed into
            // CryEngine-style offset position + Euler-degree rotation.
            let world = container_mat * glam::Mat4::from_cols_array_2d(&placement.transform);
            let (_scale, rot, trans) = world.to_scale_rotation_translation();
            let (rx, ry, rz) = rot.to_euler(glam::EulerRot::XYZ);

            let key = (cgf_path.clone(), material_path.clone());
            let unique_index = *key_index.entry(key.clone()).or_insert_with(|| {
                let i = unique_keys.len();
                unique_keys.push(key);
                i
            });

            anim_placements.push(AnimPlacement {
                unique_index,
                offset_position: [trans.x, trans.y, trans.z],
                offset_rotation: [rx.to_degrees(), ry.to_degrees(), rz.to_degrees()],
                palette: placement.palette.clone(),
                entity_name: cga_entity_name(&cgf_path),
                container_index,
                // The placement's own class's controller; a class-less geometry
                // placement takes the one its geometry was first spawned with.
                controller: placement
                    .identity
                    .entity_class_guid
                    .as_deref()
                    .and_then(|guid| class_animation_controller(db, guid))
                    .or_else(|| cgf_controllers.get(placement.mesh_index).cloned().flatten()),
                transform: placement.transform,
                identity: placement.identity,
                ui_bindings: placement.ui_bindings,
            });
        }
        container.placements = kept;
    }

    if anim_placements.is_empty() {
        return Vec::new();
    }
    // A child's name is its clips' namespace: placements of one geometry whose
    // classes have different Mannequin controllers get their own (each clip
    // carries its own class's fragments).
    let mut names: Vec<String> = anim_placements.iter().map(|ap| ap.entity_name.clone()).collect();
    let controllers: Vec<Option<crate::animation::AnimationControllerSource>> =
        anim_placements.iter().map(|ap| ap.controller.clone()).collect();
    namespace_by_controller(&mut names, &controllers);
    for (ap, name) in anim_placements.iter_mut().zip(names) {
        ap.entity_name = name;
    }

    // Phase 2: load each unique geometry once, in parallel (palette-independent —
    // the per-object tint palette is applied downstream from the EntityPayload's
    // `palette` field, so the same loaded asset is shared across placements).
    let loaded: Vec<Option<LoadedChildPayload>> = unique_keys
        .par_iter()
        .map(|(geometry_path, material_path)| {
            let node = ResolvedNode {
                entity_name: cga_entity_name(geometry_path),
                attachment_name: String::new(),
                item_port_name: None,
                no_rotation: false,
                offset_position: [0.0; 3],
                offset_rotation: [0.0; 3],
                detach_direction: [0.0; 3],
                port_flags: String::new(),
                nmc: None,
                bones: Vec::new(),
                has_geometry: true,
                record: placeholder_record(),
                geometry_path: Some(geometry_path.clone()),
                material_path: material_path.clone(),
                allows_child_object_containers: false,
                children: Vec::new(),
            };
            load_child_payload_asset(&node, db, p4k, opts, opts.material_mode, None)
        })
        .collect();

    // Phase 3: build one EntityPayload per placement, sharing the loaded geometry.
    let mut children = Vec::with_capacity(anim_placements.len());
    for ap in anim_placements {
        let Some(loaded_payload) = loaded[ap.unique_index].as_ref() else {
            continue;
        };
        // The placement's own door (class values with the instance's overrides)
        // and links travel on the child as on a loadout child; the rest of its
        // identity is exported with it (scene.json `children[].placement`).
        interiors.promoted.push(super::interiors::PromotedPlacement {
            child_index: children.len(),
            container_index: ap.container_index,
            identity: ap.identity.clone(),
            transform: ap.transform,
            controller: ap.controller,
        });
        children.push(EntityPayload {
            mesh: loaded_payload.mesh.clone(),
            materials: loaded_payload.materials.clone(),
            textures: loaded_payload.textures.clone(),
            nmc: loaded_payload.nmc.clone(),
            // Keep the placement's per-object tint palette if it had one.
            palette: ap.palette.or_else(|| loaded_payload.palette.clone()),
            geometry_path: loaded_payload.geometry_path.clone(),
            material_path: loaded_payload.material_path.clone(),
            bones: loaded_payload.bones.clone(),
            skeleton_source_path: loaded_payload.skeleton_source_path.clone(),
            entity_name: ap.entity_name,
            entity_category: None,
            attach_def_type: None,
            door: ap.identity.door.clone(),
            item_port_links: ap.identity.item_port_links.clone(),
            parent_node_name: String::new(),
            parent_entity_name: String::new(),
            no_rotation: false,
            offset_position: ap.offset_position,
            offset_rotation: ap.offset_rotation,
            detach_direction: [0.0; 3],
            port_flags: String::new(),
            port_path: None,
            ui_bindings: ap.ui_bindings,
        });
    }
    children
}

#[cfg(test)]
mod tests {
    use super::{cga_entity_name, chrparams_path_for_cga, namespace_by_controller};
    use crate::animation::AnimationControllerSource;

    fn controller(adb: &str, ctl: &str) -> Option<AnimationControllerSource> {
        Some(AnimationControllerSource { animation_database: adb.into(), animation_controller: ctl.into() })
    }

    #[test]
    fn placements_of_one_geometry_with_different_controllers_get_their_own_namespace() {
        // Synthetic paths: one geometry spawned by two classes with different controllers, another by one.
        let mut names = vec!["door_a".to_string(), "door_a".to_string(), "door_a".to_string(), "lift_b".to_string(), "lift_b".to_string()];
        let controllers = vec![
            controller("Animations/Mannequin/ADB/Door_One.adb", "Animations/Mannequin/Door_One.xml"),
            controller("Animations/Mannequin/ADB/Door_Two.adb", "Animations/Mannequin/Door_Two.xml"),
            None,
            controller("Animations/Mannequin/ADB/Lift.adb", "Animations/Mannequin/Lift.xml"),
            controller("Animations/Mannequin/ADB/Lift.adb", "Animations/Mannequin/Lift.xml"),
        ];
        namespace_by_controller(&mut names, &controllers);
        assert_eq!(names, vec!["door_a__Door_One_Door_One", "door_a__Door_Two_Door_Two", "door_a__none", "lift_b", "lift_b"]);
    }

    #[test]
    fn chrparams_path_for_cga_maps_only_cga_geometry() {
        assert_eq!(
            chrparams_path_for_cga(
                "objects/buildingsets/human/hightech/delta/int/hangar/elev/ht_hangar_ship_elev_xl_a.cga"
            )
            .as_deref(),
            Some("objects/buildingsets/human/hightech/delta/int/hangar/elev/ht_hangar_ship_elev_xl_a.chrparams")
        );
        // static .cgf geometry has no animation chrparams
        assert_eq!(chrparams_path_for_cga("objects/walls/hangar_wall_a.cgf"), None);
        // case-insensitive extension match, stem case preserved
        assert_eq!(
            chrparams_path_for_cga("Objects/Elev/Door_A.CGA").as_deref(),
            Some("Objects/Elev/Door_A.chrparams")
        );
        // not geometry
        assert_eq!(chrparams_path_for_cga("foo/bar.dds"), None);
    }

    #[test]
    fn cga_entity_name_strips_dir_and_extension() {
        assert_eq!(cga_entity_name("a/b/ht_hangar_ship_elev_xl_a.cga"), "ht_hangar_ship_elev_xl_a");
        assert_eq!(cga_entity_name("Door_A.CGA"), "Door_A");
    }
}
