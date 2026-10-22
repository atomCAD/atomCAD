//! Record fields a node declares in its own data rather than taking from a
//! record def: an anonymous `record_construct`'s fields
//! (`doc/design_wire_declared_inputs.md`) and an anonymous reader's —
//! `record_destructure` (`doc/design_anonymous_record_readers.md`). The field
//! type, its id allocation, the type gate and the pin-id space are shared, so
//! every node that holds such fields gets the same rules; the reference
//! walkers reach them through `NodeData::local_record_fields[_mut]`.

use crate::data_type::{DataType, walk_data_type_record_names};
use crate::node_type_registry::{RecordDefs, is_valid_record_field_type};
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
