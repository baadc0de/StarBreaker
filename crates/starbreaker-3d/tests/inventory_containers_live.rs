//! Live-data regression for inventory containers in decomposed `scene.json`:
//! a vehicle's geometry-less cargo-grid loadout items (`inventory_containers[]`,
//! `children[].port_path`), an environment's freight-elevator grid
//! (`interiors[].inventory_containers[]`) and a cargo box's footprint
//! (`root_entity.inventory_occupancy`). Skips when no `Data.p4k` is installed.
use starbreaker_3d::{ExportFormat, ExportKind, ExportOptions, MaterialMode};
use starbreaker_datacore::{
    Database,
    loadout::{EntityIndex, resolve_loadout_indexed},
};

fn open() -> Option<(starbreaker_p4k::MappedP4k, Vec<u8>)> {
    let Ok(found) = starbreaker_common::discover::find_p4k() else {
        eprintln!("skipping: no Data.p4k installed");
        return None;
    };
    let p4k = starbreaker_p4k::MappedP4k::open(found.path).unwrap();
    let path = if p4k.entry_case_insensitive(r"Data\Game2.dcb").is_some() { r"Data\Game2.dcb" } else { r"Data\Game.dcb" };
    let bytes = p4k.read_file(path).unwrap();
    Some((p4k, bytes))
}

/// Metadata-only options: the manifest is under test, not the assets.
fn options() -> ExportOptions {
    ExportOptions {
        kind: ExportKind::Decomposed,
        format: ExportFormat::Glb,
        material_mode: MaterialMode::None,
        include_animations: false,
        include_lights: false,
        apply_default_animation_pose: false,
        ..Default::default()
    }
}

fn scene(files: &[starbreaker_3d::ExportedFile]) -> serde_json::Value {
    let file = files.iter().find(|f| f.relative_path.ends_with("/scene.json")).unwrap();
    serde_json::from_slice(&file.bytes).unwrap()
}

fn entity_scene(db: &Database, p4k: &starbreaker_p4k::MappedP4k, name: &str) -> serde_json::Value {
    let idx = EntityIndex::new(db);
    let record = idx.find_record(name).expect("live regression fixture");
    let tree = resolve_loadout_indexed(&idx, record);
    let result = starbreaker_3d::assemble_glb_with_loadout(db, p4k, record, &tree, &options()).unwrap();
    scene(&result.decomposed.unwrap().files)
}

#[test]
fn vehicle_cargo_grids_are_exported_with_distinct_port_paths() {
    let Some((p4k, bytes)) = open() else { return };
    let db = Database::from_bytes(&bytes).unwrap();
    let scene = entity_scene(&db, &p4k, "MISC_Hull_A");
    let containers = scene["inventory_containers"].as_array().unwrap();
    assert_eq!(containers.len(), 4, "four cargo plates, one grid each");
    let parents: std::collections::BTreeSet<String> =
        containers.iter().map(|c| c["parent_port_path"].to_string()).collect();
    assert_eq!(parents.len(), 4, "distinct parent_port_path per grid");
    for container in containers {
        assert_eq!(container["container"]["interior_dimensions"], serde_json::json!([2.5, 5.0, 2.5]));
        assert_eq!(container["container"]["inventory_type"], "InventoryOpenContainerType");
        assert!(container["local_transform_sc"].is_array());
        // The parent port path names a scene child: the plate the grid hangs on.
        let parent = scene["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|child| child["port_path"] == container["parent_port_path"])
            .expect("parent child by port_path");
        assert_eq!(parent["entity_name"], container["parent_entity_name"]);
    }
}

#[test]
fn hangar_freight_elevator_grid_is_an_interior_container() {
    let Some((p4k, bytes)) = open() else { return };
    let db = Database::from_bytes(&bytes).unwrap();
    let socpak = r"Data\ObjectContainers\PU\loc\mod\common\hangar\hightech_a\hangar_lrgtop_001.socpak";
    let entry = p4k.entry_case_insensitive(socpak).expect("live regression fixture").name.clone();
    let export = starbreaker_3d::socpaks_to_decomposed_blend(&db, &p4k, &[entry], "hangar", &options()).unwrap();
    let scene = scene(&export.files);
    let grids: Vec<_> = scene["interiors"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|interior| interior["inventory_containers"].as_array().unwrap())
        .filter(|c| c["container"]["inventory_type"] == "InventoryOpenContainerType")
        .collect();
    assert!(!grids.is_empty(), "the hangar's freight elevator grid");
    for grid in grids {
        assert!(grid["entity_id"].is_string());
        assert!(grid["transform"].is_array());
        assert!(grid["container"]["max_permitted_item_size"].is_array());
    }
}

#[test]
fn cargo_box_root_carries_its_footprint() {
    let Some((p4k, bytes)) = open() else { return };
    let db = Database::from_bytes(&bytes).unwrap();
    let scene = entity_scene(&db, &p4k, "Carryable_TBO_FL_16SCU_Commodity_Organic");
    let root = &scene["root_entity"];
    assert!(root["inventory_occupancy"]["dimensions"].is_array());
    assert_eq!(root["inventory_occupancy"]["volume_unit"], "SStandardCargoUnit");
    assert!(root["standard_cargo_units"].is_number());
    assert!(root["mass"].is_number());
}
