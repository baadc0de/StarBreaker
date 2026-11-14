//! One DataCore record as JSON, by name or GUID — the shape `dcb query`
//! prints for a property value.
//!
//! - [`find_records`]: the main records a `Type.Name`, a bare record name
//!   (case-insensitive, exact) or a GUID names; callers decide what zero or
//!   several matches mean.
//! - [`record_value`]: a record's instance as a [`Value`] tree, references kept
//!   as their record GUIDs (bounded: an entity's references fan out into the
//!   whole graph; each GUID resolves by another lookup).
//! - [`value_to_json`]: a [`Value`] as JSON, struct type in `__type`, GUIDs
//!   as plain `xxxxxxxx-xxxx-…` strings.
//! - [`record_json`]: both, with the record's `__record_name` / `__record_id`.

use std::str::FromStr;

use starbreaker_common::{CigGuid, SpanReader};

use crate::database::Database;
use crate::error::QueryError;
use crate::query::execute::materialize_struct_as_value_no_refs;
use crate::query::value::Value;
use crate::types::Record;

/// The main records `name_or_guid` names: the record with that GUID, else the
/// records whose full name (`Type.Name`) or name after the type prefix equals
/// it, ignoring ASCII case.
pub fn find_records<'a>(db: &'a Database<'a>, name_or_guid: &str) -> Vec<&'a Record> {
    let wanted = name_or_guid.trim();
    if let Ok(guid) = CigGuid::from_str(wanted) {
        return db.record_by_id(&guid).into_iter().collect();
    }
    db.records()
        .iter()
        .filter(|record| db.is_main_record(record))
        .filter(|record| {
            let full = db.resolve_string2(record.name_offset);
            let short = full.split_once('.').map_or(full, |(_, name)| name);
            full.eq_ignore_ascii_case(wanted) || short.eq_ignore_ascii_case(wanted)
        })
        .collect()
}

/// The record's instance as a value tree; references stay `Value::Guid`.
pub fn record_value<'a>(db: &'a Database<'a>, record: &Record) -> Result<Value<'a>, QueryError> {
    let bytes = db.get_instance(record.struct_index, record.instance_index as i32);
    materialize_struct_as_value_no_refs(db, record.struct_index, &mut SpanReader::new(bytes))
}

/// A value as JSON: objects carry their struct type in `__type`.
pub fn value_to_json(value: &Value) -> serde_json::Value {
    use serde_json::json;
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => json!(b),
        Value::Int8(n) => json!(n),
        Value::Int16(n) => json!(n),
        Value::Int32(n) => json!(n),
        Value::Int64(n) => json!(n),
        Value::UInt8(n) => json!(n),
        Value::UInt16(n) => json!(n),
        Value::UInt32(n) => json!(n),
        Value::UInt64(n) => json!(n),
        Value::Float(n) => json!(n),
        Value::Double(n) => json!(n),
        Value::String(s) | Value::Enum(s) | Value::Locale(s) => json!(s),
        Value::Guid(g) => json!(g.to_string()),
        Value::Array(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Object { type_name, fields, .. } => {
            let mut map = serde_json::Map::new();
            map.insert("__type".into(), json!(type_name));
            for (name, field) in fields {
                map.insert((*name).to_string(), value_to_json(field));
            }
            serde_json::Value::Object(map)
        }
    }
}

/// `record` as JSON (see [`record_value`], [`value_to_json`]), with its full
/// name in `__record_name` and GUID in `__record_id`.
pub fn record_json(db: &Database, record: &Record) -> Result<serde_json::Value, QueryError> {
    let mut json = value_to_json(&record_value(db, record)?);
    if let Some(map) = json.as_object_mut() {
        map.insert("__record_name".into(), db.resolve_string2(record.name_offset).into());
        map.insert("__record_id".into(), record.id.to_string().into());
    }
    Ok(json)
}
