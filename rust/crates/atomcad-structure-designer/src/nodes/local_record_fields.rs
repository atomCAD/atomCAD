//! Record fields a node declares in its own data rather than taking from a
//! record def: an anonymous `record_construct`'s fields
//! (`doc/design_wire_declared_inputs.md`) and an anonymous reader's —
//! `record_destructure`, `record_get` and `record_set`
//! (`doc/design_anonymous_record_readers.md`). The field
//! type, its id allocation, the type gate and the pin-id space are shared, so
//! every node that holds such fields gets the same rules; the reference
//! walkers reach them through `NodeData::local_record_fields[_mut]`.

use crate::data_type::{DataType, RecordType, walk_data_type_record_names};
use crate::node_type_registry::{RecordDefs, is_valid_record_field_type};
use crate::text_format::TextValue;
use serde::{Deserialize, Serialize};

/// Anonymous field pins carry `ANON_FIELD_PIN_ID_BASE | field.id`. `FieldId`s
/// are counters allocated from a per-file floor and never reach this range, so
/// `Node::set_custom_node_type`'s id-first matching can never pair an
/// anonymous field with an unrelated def field on a mode switch: it falls
/// through to name matching, which is the intended correspondence.
pub const ANON_FIELD_PIN_ID_BASE: u64 = 1 << 62;

/// One record field a node declares itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalRecordField {
    /// Stable within the node; the pin id is `ANON_FIELD_PIN_ID_BASE | id`.
    pub id: u64,
    pub name: String,
    /// Never `Optional`: the type comes from a pin, and `Optional` is never a
    /// pin type. A promoted def can make the field optional later.
    pub data_type: DataType,
}

/// `skip_serializing_if` helper for a node's `next_field_id`.
pub(crate) fn is_zero(value: &u64) -> bool {
    *value == 0
}

/// The type gate of a local field: any type a record def field may hold
/// ([`is_valid_record_field_type`]), except the ones a declared field can
/// never have — `None`, `Unit`, `AnyFunction` (not a value type) and
/// `Optional` (never a pin type).
pub fn accepts_field_type(data_type: &DataType) -> Result<(), String> {
    match data_type {
        DataType::None => Err("A record field cannot have type None".to_string()),
        DataType::Unit => Err("A record field cannot have type Unit".to_string()),
        DataType::AnyFunction { .. } => {
            Err("A record field needs a concrete function type".to_string())
        }
        DataType::Optional(_) => Err("An anonymous record field cannot be Optional".to_string()),
        _ => is_valid_record_field_type(data_type),
    }
}

/// A fresh local field id: at or above `next_field_id` and above every id in
/// `fields`. Advances `next_field_id` past it.
pub fn allocate_field_id(fields: &[LocalRecordField], next_field_id: &mut u64) -> u64 {
    let floor = fields.iter().map(|f| f.id + 1).max().unwrap_or(0);
    let id = (*next_field_id).max(floor);
    *next_field_id = id + 1;
    id
}

/// The index of the local field a pin id names, if any.
pub fn field_index_for_pin(fields: &[LocalRecordField], pin_id: u64) -> Option<usize> {
    if pin_id & ANON_FIELD_PIN_ID_BASE == 0 {
        return None;
    }
    let id = pin_id & !ANON_FIELD_PIN_ID_BASE;
    fields.iter().position(|f| f.id == id)
}

/// Replace `fields` with `new_fields` (names and types, in order), keeping a
/// field's id when a current field has the **same name** and giving every
/// other field a fresh one. There is no positional fallback: a field that
/// disappeared while another appeared is not the same field. Refuses, with
/// nothing changed, when a type fails [`accepts_field_type`].
pub fn rebuild_fields_by_name(
    fields: &mut Vec<LocalRecordField>,
    next_field_id: &mut u64,
    new_fields: Vec<(String, DataType)>,
) -> Result<(), String> {
    for (name, data_type) in &new_fields {
        accepts_field_type(data_type).map_err(|reason| format!("Field '{}': {}", name, reason))?;
    }
    // Every id in use stays reserved while fresh ones are handed out, so a
    // fresh id never equals one kept further down the list.
    *next_field_id = (*next_field_id).max(fields.iter().map(|f| f.id + 1).max().unwrap_or(0));
    let old = std::mem::take(fields);
    for (name, data_type) in new_fields {
        let id = match old.iter().find(|f| f.name == name) {
            Some(kept) => kept.id,
            None => allocate_field_id(&old, next_field_id),
        };
        fields.push(LocalRecordField {
            id,
            name,
            data_type,
        });
    }
    Ok(())
}

/// One warning per local field type that names a record def which does not
/// exist (a hand-edited file): the field's pin is typed by a dangling name,
/// and this is what tells the user so. A name under a library mount freezes
/// the node instead and never gets here.
pub fn local_field_problems(fields: &[LocalRecordField], defs: RecordDefs<'_>) -> Vec<String> {
    let mut problems = Vec::new();
    for field in fields {
        walk_data_type_record_names(&field.data_type, &mut |name| {
            if defs.lookup(name).is_none() {
                problems.push(format!(
                    "Field '{}' has unknown record type '{}'",
                    field.name, name
                ));
            }
        });
    }
    problems
}

/// The `fields` text property of a reader in anonymous mode, written exactly
/// like `record_construct`'s: `[{name: "a", data_type: Float}, …]`.
pub fn fields_text_value(fields: &[LocalRecordField]) -> TextValue {
    TextValue::Array(
        fields
            .iter()
            .map(|f| {
                TextValue::Object(vec![
                    ("name".to_string(), TextValue::String(f.name.clone())),
                    (
                        "data_type".to_string(),
                        TextValue::DataType(f.data_type.clone()),
                    ),
                ])
            })
            .collect(),
    )
}

/// Parses a reader's `fields` text property into names and types, in order.
/// Errors on a malformed entry, a name that is not an identifier, or a
/// duplicate name. The types are checked when the fields are installed
/// ([`rebuild_fields_by_name`]).
pub fn parse_fields_text_value(value: &TextValue) -> Result<Vec<(String, DataType)>, String> {
    let TextValue::Array(entries) = value else {
        return Err("fields must be an array".to_string());
    };
    let mut new_fields: Vec<(String, DataType)> = Vec::with_capacity(entries.len());
    for entry in entries {
        let TextValue::Object(obj) = entry else {
            return Err("each field must be an object { name, data_type }".to_string());
        };
        let name = obj
            .iter()
            .find(|(k, _)| k == "name")
            .and_then(|(_, v)| v.as_string())
            .ok_or_else(|| "field name must be a string".to_string())?
            .to_string();
        let data_type = obj
            .iter()
            .find(|(k, _)| k == "data_type")
            .and_then(|(_, v)| v.to_data_type())
            .ok_or_else(|| "field data_type must be a DataType".to_string())?;
        if !crate::declared_inputs::is_input_identifier(&name) {
            return Err(format!(
                "Field name '{}' must be a letter or '_' followed by letters, digits or '_'",
                name
            ));
        }
        if new_fields.iter().any(|(n, _)| *n == name) {
            return Err(format!("Duplicate field name '{}'", name));
        }
        new_fields.push((name, data_type));
    }
    Ok(new_fields)
}

/// A reader's mode change as its data sees it: the `schema` / `fields` /
/// `next_field_id` triple after adopting `t` (`RecordTypedInput::
/// set_record_input_type`). A named type sets the schema and clears the
/// fields; an anonymous one clears the schema and rebuilds the fields from
/// `current`, keeping ids by name; `{}` leaves both empty (unset). Refuses,
/// with nothing changed, when a field type fails [`accepts_field_type`].
pub fn adopted_reader_fields(
    t: &RecordType,
    current: &[LocalRecordField],
    next_field_id: u64,
) -> Result<(String, Vec<LocalRecordField>, u64), String> {
    match t {
        RecordType::Named(name) => Ok((name.clone(), Vec::new(), next_field_id)),
        RecordType::Anonymous(type_fields) => {
            let mut fields = current.to_vec();
            let mut next_field_id = next_field_id;
            rebuild_fields_by_name(&mut fields, &mut next_field_id, type_fields.clone())?;
            Ok((String::new(), fields, next_field_id))
        }
    }
}

/// A reader's `schema` and `fields` text properties applied to its current
/// `schema` / `fields` / `next_field_id`, `schema` first: a schema that
/// differs from the current one clears the fields, and `fields` is then
/// rebuilt by name when there is no schema (`fields: []` makes the node
/// unset) and refused on a named node. `kind` names the node in the refusal.
pub fn reader_fields_from_text(
    props: &std::collections::HashMap<String, TextValue>,
    schema: &str,
    fields: &[LocalRecordField],
    next_field_id: u64,
    kind: &str,
) -> Result<(String, Vec<LocalRecordField>, u64), String> {
    let new_schema = match props.get("schema") {
        Some(v) => v
            .as_string()
            .ok_or_else(|| "schema must be a String".to_string())?
            .to_string(),
        None => schema.to_string(),
    };
    let mut new_fields = if new_schema == schema {
        fields.to_vec()
    } else {
        Vec::new()
    };
    let mut next_field_id = next_field_id;
    if let Some(v) = props.get("fields") {
        let listed = parse_fields_text_value(v)?;
        if new_schema.is_empty() {
            rebuild_fields_by_name(&mut new_fields, &mut next_field_id, listed)?;
        } else if !listed.is_empty() {
            return Err(format!(
                "fields can only be listed on a {} with no schema",
                kind
            ));
        }
    }
    Ok((new_schema, new_fields, next_field_id))
}
