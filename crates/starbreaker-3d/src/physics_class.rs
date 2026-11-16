//! Collision classes of CryPhysics colliders, from the engine's node-naming convention. The `StatObjPhysics`
//! chunk carries no collision filter (its `part` is a per-node index; the blob headers of filtered and ordinary
//! colliders are identical), so Star Engine reads the class from the node name, as CryEngine does for its `$`
//! helper conventions. [`collision_class`] is the single table; `None` is an ordinary collider. Surveyed on ship
//! geometry (WP-P3): `$IP_*` interaction volumes (724 of 4 158 sampled colliders), `TIRES_ONLY_COLLISION(S)`,
//! `PLAYER_ONLY_COLLISIONS`, `IGNORE_TERRAIN_COLLIDE` / `_COLLISION`, `NO_TERRAIN_COLLISION`, and
//! `physicsgrid_` / `localgrid_allow_no_collide_*` local physics-grid extensions (cargo lifts, ramps, docking
//! collars). Contract: docs/decomposed-export-contract.md.
//!
//! One class comes from the geometry itself: a trimesh carries a sub-material index per triangle, and
//! [`NO_SURFACE`] marks triangles that have none. In a collider that also has triangles with a surface (vehicle
//! hulls) these are the faces that close it over an opening another part fills (a ramp, a door, a canopy, a gear
//! bay, an engine nozzle): [`NO_SURFACE_CLASS`]. Surveyed on 21 700 ship geometry files (WP-P4): 32 278 of
//! 2 355 546 trimesh triangles; 697 colliders in 275 files (nearly all of them a ship's exterior `.cga`) mix
//! both. 87 colliders have no surface on any triangle (mostly 12-triangle boxes on lifts, ramps and docking
//! collars): nothing marks a cap in those, they stay ordinary colliders.

/// Per-triangle sub-material index of a trimesh triangle without a surface (CryPhysics `char` −1).
pub(crate) const NO_SURFACE: u8 = 255;
/// Class of the triangles without a surface in a collider that has triangles with one: the cap the collider
/// closes an opening with, no obstacle to what passes the opening (the part that fills it carries its own
/// colliders, in its own pose).
pub(crate) const NO_SURFACE_CLASS: &str = "no_surface";

/// The collision class a node's name gives its colliders, case-insensitively:
/// - `interaction`: `$IP_*` — interaction-point volumes (use prompts), not physical obstacles;
/// - `tires_only`: `TIRES_ONLY_COLLISION(S)` — collides with vehicle wheels only;
/// - `player_only`: `PLAYER_ONLY_COLLISIONS` — collides with actors only;
/// - `no_terrain`: `IGNORE_TERRAIN_COLLIDE` / `IGNORE_TERRAIN_COLLISION` / `NO_TERRAIN_COLLISION` — not with terrain;
/// - `grid_volume`: `physicsgrid_allow_no_collide*` / `localgrid_allow_no_collide*` — extends the physics grid.
pub(crate) fn collision_class(node_name: &str) -> Option<&'static str> {
    let name = node_name.to_ascii_lowercase();
    if name.starts_with("$ip_") {
        Some("interaction")
    } else if name.starts_with("tires_only_collision") {
        Some("tires_only")
    } else if name.starts_with("player_only_collision") {
        Some("player_only")
    } else if ["ignore_terrain_collide", "ignore_terrain_collision", "no_terrain_collision"].iter().any(|p| name.starts_with(p)) {
        Some("no_terrain")
    } else if ["physicsgrid_allow_no_collide", "localgrid_allow_no_collide"].iter().any(|p| name.starts_with(p)) {
        Some("grid_volume")
    } else {
        None
    }
}

#[cfg(test)]
#[path = "physics_class_tests.rs"]
mod tests;
