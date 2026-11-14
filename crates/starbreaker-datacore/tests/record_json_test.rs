//! `query::record_json`: record lookup by `Type.Name`, bare name and GUID
//! (with ambiguity left to the caller) and the `__type` JSON shape, over a
//! synthetic database (two struct types, three records; values are arbitrary).
mod test_helpers;

use starbreaker_datacore::database::Database;
use starbreaker_datacore::query::record_json::{find_records, record_json};
use starbreaker_datacore::types::{CigGuid, DataMapping, PropertyDefinition, Record, StringId2, StructDefinition};
use test_helpers::DcbBuilder;

const STRING: u16 = 0x000A;
const SINGLE: u16 = 0x000B;
const GUID: u16 = 0x000E;

fn guid(n: u8) -> CigGuid {
    CigGuid::from_bytes([n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
}

/// struct 0 `Thing` { label: String, weight: Single, other: Guid } (24 bytes)
/// struct 1 `Other` { label: String } (4 bytes)
/// records `Thing.alpha`, `Thing.beta`, `Other.alpha`, each in its own file.
fn build_db() -> Vec<u8> {
    let mut b = DcbBuilder::new();
    let files = [b.add_string1("a.json"), b.add_string1("b.json"), b.add_string1("c.json")];
    let label_alpha = b.add_string1("synthetic alpha");
    let label_beta = b.add_string1("synthetic beta");
    let label_other = b.add_string1("synthetic other");

    let s_thing = b.add_string2("Thing");
    let s_other = b.add_string2("Other");
    let p_label = b.add_string2("label");
    let p_weight = b.add_string2("weight");
    let p_other = b.add_string2("other");
    let names = [b.add_string2("Thing.alpha"), b.add_string2("Thing.beta"), b.add_string2("Other.alpha")];

    b.struct_defs.push(StructDefinition {
        name_offset: s_thing,
        parent_type_index: -1,
        attribute_count: 3,
        first_attribute_index: 0,
        struct_size: 24,
    });
    b.struct_defs.push(StructDefinition {
        name_offset: s_other,
        parent_type_index: -1,
        attribute_count: 1,
        first_attribute_index: 3,
        struct_size: 4,
    });
    for (name_offset, data_type) in [(p_label, STRING), (p_weight, SINGLE), (p_other, GUID), (p_label, STRING)] {
        b.property_defs.push(PropertyDefinition {
            name_offset,
            struct_index: 0,
            data_type,
            conversion_type: 0x00,
            _padding: 0,
        });
    }
    b.data_mappings.push(DataMapping { struct_count: 2, struct_index: 0 });
    b.data_mappings.push(DataMapping { struct_count: 1, struct_index: 1 });

    for (index, (struct_index, instance_index)) in [(0, 0), (0, 1), (1, 0)].into_iter().enumerate() {
        b.records.push(Record {
            name_offset: names[index],
            file_name_offset: files[index],
            tag_offset: StringId2(-1),
            struct_index,
            id: guid(index as u8 + 1),
            instance_index,
            struct_size: if struct_index == 0 { 24 } else { 4 },
        });
    }

    for (label, weight, other) in [(label_alpha, 1.5f32, guid(3)), (label_beta, 2.5f32, guid(1))] {
        b.instance_data.extend_from_slice(&label.0.to_le_bytes());
        b.instance_data.extend_from_slice(&weight.to_le_bytes());
        b.instance_data.extend_from_slice(&other.bytes);
    }
    b.instance_data.extend_from_slice(&label_other.0.to_le_bytes());
    b.build()
}

fn names(db: &Database, query: &str) -> Vec<String> {
    find_records(db, query).iter().map(|r| db.resolve_string2(r.name_offset).to_string()).collect()
}

#[test]
fn records_are_found_by_full_name_bare_name_and_guid() {
    let data = build_db();
    let db = Database::from_bytes(&data).unwrap();
    assert_eq!(names(&db, "Thing.alpha"), ["Thing.alpha"]);
    assert_eq!(names(&db, "thing.ALPHA"), ["Thing.alpha"], "case-insensitive");
    assert_eq!(names(&db, "beta"), ["Thing.beta"]);
    assert_eq!(names(&db, &guid(3).to_string()), ["Other.alpha"]);
    assert_eq!(names(&db, "alpha").len(), 2, "a bare name shared by two types is ambiguous");
    assert!(names(&db, "gamma").is_empty());
    assert!(names(&db, "alph").is_empty(), "names match exactly, not by substring");
}

#[test]
fn record_json_has_the_dcb_query_shape() {
    let data = build_db();
    let db = Database::from_bytes(&data).unwrap();
    let record = find_records(&db, "Thing.alpha")[0];
    let json = record_json(&db, record).unwrap();
    assert_eq!(json["__type"], "Thing");
    assert_eq!(json["__record_name"], "Thing.alpha");
    assert_eq!(json["__record_id"], guid(1).to_string());
    assert_eq!(json["label"], "synthetic alpha");
    assert_eq!(json["weight"], 1.5);
    assert_eq!(json["other"], guid(3).to_string(), "a GUID is a plain string another lookup resolves");
}
