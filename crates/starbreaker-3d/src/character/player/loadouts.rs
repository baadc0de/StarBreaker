//! Default-loadout discovery and merge onto the default CHF's item-port tree, and the request's edits of that
//! tree: weapon selection ([`select_weapons`]) and spare weapon attachments on the wearer's ports
//! ([`spare_attachments`]).
use super::super::{Gender, loadout::LoadoutEntry};
use super::{
    ports,
    xml::{self, Result, error},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use starbreaker_datacore::{Database, loadout::EntityIndex};
use starbreaker_p4k::MappedP4k;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerRequest {
    pub gender: String,
    pub loadout: Option<String>,
    /// Keep only these `WeaponPersonal` items of the loadout (entity class names, case-insensitive; grenades and
    /// melee items are `WeaponPersonal` too). `None` keeps every weapon.
    #[serde(default)]
    pub weapons: Option<Vec<String>>,
    /// Put each kept weapon's default-loadout attachments (its own item-port entries, e.g. its magazine) into the
    /// wearer's empty item ports that accept them, as the game's players carry spares on armour ports.
    #[serde(default)]
    pub spare_attachments: bool,
}
impl Default for PlayerRequest {
    fn default() -> Self {
        Self {
            gender: "male".into(),
            loadout: None,
            weapons: None,
            spare_attachments: false,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct PlayerLoadout {
    pub path: String,
    pub weapons: Vec<String>,
}
#[derive(Clone, Debug)]
pub(super) struct Item {
    pub entity: String,
    pub port: Option<Value>,
    pub children: Vec<Item>,
}

pub(super) fn read(p4k: &MappedP4k, path: &str) -> Result<Vec<u8>> {
    let p = path.replace('/', "\\");
    let p = if p.to_ascii_lowercase().starts_with("data\\") {
        p
    } else {
        format!("Data\\{p}")
    };
    p4k.read_file(&p).map_err(|e| error(format!("{path}: {e}")))
}
pub(super) fn entries(p4k: &MappedP4k, path: &str) -> Result<Vec<LoadoutEntry>> {
    fn walk(n: &xml::Node) -> Vec<LoadoutEntry> {
        n.child("Items")
            .into_iter()
            .flat_map(|n| &n.children)
            .filter(|n| n.tag == "Item")
            .map(|i| LoadoutEntry {
                port: i.attr("portName").into(),
                item: i.attr("itemName").into(),
                children: walk(i),
            })
            .collect()
    }
    let root = xml::parse(&read(p4k, path)?)?;
    if root.tag != "Loadout" {
        return Err(error("expected Loadout"));
    }
    Ok(walk(&root))
}

pub(super) fn tree(
    db: &Database,
    chf: &starbreaker_chf::ChfData,
    entries: &[LoadoutEntry],
    gender: &str,
) -> Result<Item> {
    let idx = EntityIndex::new(db);
    let tags = vec![gender.into()];
    fn from_chf(db: &Database, p: &starbreaker_chf::ItemPort) -> Item {
        let entity = db
            .record_by_id(&p.id)
            .map(|r| db.resolve_string2(r.name_offset).rsplit('.').next().unwrap().to_owned())
            .unwrap_or_default();
        Item {
            entity,
            port: Some(serde_json::json!({"name":p.name.to_string()})),
            children: p.children.iter().map(|p| from_chf(db, p)).collect(),
        }
    }
    fn merge(node: &mut Item, entries: &[LoadoutEntry]) {
        for e in entries {
            let i = match node
                .children
                .iter()
                .position(|c| ports::text(&c.port.as_ref().unwrap()["name"]).eq_ignore_ascii_case(&e.port))
            {
                Some(i) => i,
                None => {
                    node.children.push(Item {
                        entity: String::new(),
                        port: Some(serde_json::json!({"name":e.port})),
                        children: vec![],
                    });
                    node.children.len() - 1
                }
            };
            let c = &mut node.children[i];
            if !e.item.is_empty() {
                c.entity = e.item.clone();
                c.children.clear();
            }
            merge(c, &e.children);
        }
    }
    fn bind(node: &mut Item, idx: &EntityIndex, tags: &[String]) -> Result<()> {
        let defs = if let Some(r) = idx.find_record(&node.entity) {
            ports::ports(idx.db, r, &node.entity, tags)?
        } else {
            vec![]
        };
        for child in &mut node.children {
            let name = child.port.as_ref().map(|p| ports::text(&p["name"])).unwrap_or("");
            child.port = defs
                .iter()
                .find(|p| ports::text(&p["name"]).eq_ignore_ascii_case(name))
                .cloned();
            bind(child, idx, tags)?;
        }
        Ok(())
    }
    let mut root = from_chf(db, &chf.itemport);
    for e in entries {
        if e.port.eq_ignore_ascii_case(&chf.itemport.name.to_string()) {
            merge(&mut root, &e.children);
        } else {
            merge(&mut root, std::slice::from_ref(e));
        }
    }
    bind(&mut root, &idx, &tags)?;
    Ok(root)
}

fn is_weapon(idx: &EntityIndex, entity: &str) -> Result<bool> {
    match idx.find_record(entity) {
        Some(r) => Ok(ports::attach(idx.db, r)?["Type"] == "WeaponPersonal"),
        None => Ok(false),
    }
}
/// Drop every `WeaponPersonal` item not named in `keep` (see [`PlayerRequest::weapons`]).
pub(super) fn select_weapons(node: &mut Item, idx: &EntityIndex, keep: &[String]) -> Result<()> {
    let mut kept = Vec::with_capacity(node.children.len());
    for child in std::mem::take(&mut node.children) {
        if !is_weapon(idx, &child.entity)? || keep.iter().any(|k| k.eq_ignore_ascii_case(&child.entity)) {
            kept.push(child);
        }
    }
    node.children = kept;
    for child in &mut node.children {
        select_weapons(child, idx, keep)?;
    }
    Ok(())
}
/// Whether an item port's authored `Types` take an item with this `AttachDef` (type, and subtype unless the port
/// lists none).
pub(super) fn accepts(types: &Value, attach: &Value) -> bool {
    let (ty, sub) = (ports::text(&attach["Type"]), ports::text(&attach["SubType"]));
    types.as_array().into_iter().flatten().any(|t| {
        let subs = t["SubTypes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["value"].as_str().or_else(|| s.as_str()))
            .collect::<Vec<_>>();
        ports::text(&t["Type"]).eq_ignore_ascii_case(ty) && (subs.is_empty() || subs.iter().any(|s| s.eq_ignore_ascii_case(sub)))
    })
}
/// See [`PlayerRequest::spare_attachments`]: the wearer's items (not the weapons themselves) receive the spares.
pub(super) fn spare_attachments(root: &mut Item, idx: &EntityIndex, tags: &[String]) -> Result<()> {
    fn weapons(n: &Item, idx: &EntityIndex, out: &mut Vec<String>) -> Result<()> {
        if is_weapon(idx, &n.entity)? {
            out.push(n.entity.clone());
        }
        for c in &n.children {
            weapons(c, idx, out)?;
        }
        Ok(())
    }
    let mut kept = Vec::new();
    weapons(root, idx, &mut kept)?;
    let mut spares = Vec::new();
    for weapon in &kept {
        let Some(record) = idx.find_record(weapon) else { continue };
        for child in starbreaker_datacore::loadout::resolve_loadout_indexed(idx, record).root.children {
            if !child.entity_name.is_empty() {
                spares.push((child.entity_name.clone(), ports::attach(idx.db, &child.record)?));
            }
        }
    }
    fn fill(node: &mut Item, idx: &EntityIndex, tags: &[String], spares: &[(String, Value)]) -> Result<()> {
        if !is_weapon(idx, &node.entity)? {
            if let Some(record) = idx.find_record(&node.entity) {
                for def in ports::ports(idx.db, record, &node.entity, tags)? {
                    // Storage ports carry `inventory`; `select` ports (hands) are where items are used, not kept.
                    let flag = |f: &str| def["flags"].as_array().into_iter().flatten().any(|v| v.as_str().is_some_and(|s| s.eq_ignore_ascii_case(f)));
                    if !flag("inventory") || flag("select") {
                        continue;
                    }
                    let name = ports::text(&def["name"]).to_owned();
                    let same = |c: &Item| c.port.as_ref().is_some_and(|p| ports::text(&p["name"]).eq_ignore_ascii_case(&name));
                    if node.children.iter().any(|c| same(c) && !c.entity.is_empty()) {
                        continue;
                    }
                    if let Some((entity, _)) = spares.iter().find(|(_, attach)| accepts(&def["types"], attach)) {
                        node.children.retain(|c| !same(c));
                        node.children.push(Item { entity: entity.clone(), port: Some(def), children: vec![] });
                    }
                }
            }
            for c in &mut node.children {
                fill(c, idx, tags, spares)?;
            }
        }
        Ok(())
    }
    fill(root, idx, tags, &spares)
}

pub fn player_loadouts(p4k: &MappedP4k, db: &Database, gender: Gender) -> Result<Vec<PlayerLoadout>> {
    let chf = super::super::default_wearer_chf(p4k, gender)?;
    let chf = starbreaker_chf::ChfFile::from_chf(&chf)?
        .parse()
        .map_err(|e| error(e.to_string()))?;
    let idx = EntityIndex::new(db);
    let mut paths = p4k
        .entries()
        .iter()
        .filter(|e| {
            let n = e.name.replace('\\', "/").to_ascii_lowercase();
            n.starts_with("data/scripts/loadouts/player/defaultloadouts/") && n.ends_with(".xml")
        })
        .map(|e| e.name.replace('\\', "/"))
        .collect::<Vec<_>>();
    paths.sort_by_key(|s| s.to_ascii_lowercase());
    let mut out = Vec::new();
    fn weapons(n: &Item, idx: &EntityIndex, out: &mut Vec<String>) -> Result<()> {
        if let Some(p) = &n.port {
            if p["attachment"] == "bone" && !p["select_tag"].is_null() {
                if let Some(r) = idx.find_record(&n.entity) {
                    if ports::attach(idx.db, r)?["Type"] == "WeaponPersonal" {
                        out.push(n.entity.clone());
                    }
                }
            }
        }
        for c in &n.children {
            weapons(c, idx, out)?;
        }
        Ok(())
    }
    for path in paths {
        let entries = entries(p4k, &path)?;
        let t = tree(db, &chf, &entries, gender.tag())?;
        let mut w = Vec::new();
        weapons(&t, &idx, &mut w)?;
        out.push(PlayerLoadout { path, weapons: w });
    }
    Ok(out)
}
/// The wearer supplies identity; loadout head trees must not become duplicate clothing.
pub(super) fn equipment(
    entries: &[LoadoutEntry],
    identity_ports: &std::collections::BTreeSet<String>,
) -> Vec<LoadoutEntry> {
    entries
        .iter()
        .filter(|e| !identity_ports.contains(&e.port.to_ascii_lowercase()))
        .map(|e| LoadoutEntry {
            port: e.port.clone(),
            item: e.item.clone(),
            children: equipment(&e.children, identity_ports),
        })
        .collect()
}
pub(super) fn wearer_equipment(
    db: &Database,
    chf: &starbreaker_chf::ChfData,
    entries: &[LoadoutEntry],
) -> Result<Vec<LoadoutEntry>> {
    let mut identity = std::collections::BTreeSet::new();
    let mut pending = vec![&chf.itemport];
    while let Some(port) = pending.pop() {
        if let Some(record) = db.record_by_id(&port.id) {
            if ports::attach(db, record)?["Type"] == "Char_Head" {
                identity.insert(port.name.to_string().to_ascii_lowercase());
            }
        }
        pending.extend(&port.children);
    }
    Ok(equipment(entries, &identity))
}
