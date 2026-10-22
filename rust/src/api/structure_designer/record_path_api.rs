//! Kernel seam for the `record_get` / `record_set` panels
//! (`doc/design_record_get_set.md` Phase 4).
//!
//! The row edits are the Phase 3 `StructureDesigner::set_record_get_data` /
//! `set_record_set_data` ops (id merge, schema re-resolution, the
//! whole-network undo command); this file only translates between those and
//! the shapes Dart holds. Three rules it encodes:
//!
//! - **Row ids never cross the API.** A row is named by its path: current
//!   names for a live row, stored names plus `dangling: true` for a dangling
//!   one — exactly what the text format prints, so echoing the getter's rows
//!   back to the setter is a no-op, and a row the panel leaves out is deleted.
//! - **The literal pair is keyed by row index** (dangling rows included) and
//!   is a plain node-data edit through `set_node_network_data_scoped`, so it
//!   brings the ordinary `SetNodeData` undo with it, like
//!   `set_record_construct_literal`.
//! - **The field tree is built here, not in Dart**: a path steps through
//!   built-in defs and anonymous record types, neither of which the record-def
//!   getters expose. It starts at the node's root — its `schema` def, or the
//!   anonymous type it took from its wire
//!   (`doc/design_anonymous_record_readers.md`).
//!
//! The work is in `#[frb(ignore)]` functions over a plain `StructureDesigner`
//! so the api-side tests reach it without the global `CAD_INSTANCE`.

use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance_or,
};
use crate::api::common_api_types::APIResult;
use crate::api::structure_designer::structure_designer_api::{
    api_literal_to_text_value, data_type_to_api_data_type, data_type_to_simple_param_type,
    field_editor_hint_to_api, text_value_to_api_literal,
};
use crate::api::structure_designer::structure_designer_api_types::{
    APILiteralField, APILiteralValue, APIRecordFieldNode, APIRecordLocalField, APIRecordPath,
    APIRecordPathNodeData, APIRecordPathRow,
};
use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::node_data::MissingDefs;
use atomcad_structure_designer::node_network::Node;
use atomcad_structure_designer::node_type_registry::{FieldEditorHint, RecordDefs};
use atomcad_structure_designer::nodes::local_record_fields::LocalRecordField;
use atomcad_structure_designer::nodes::record_get::RecordGetData;
use atomcad_structure_designer::nodes::record_path_rows::{
    RecordPathRow, literal_fits, row_pins, row_problem, row_spec,
};
use atomcad_structure_designer::nodes::record_set::{RESERVED_PIN_NAMES, RecordSetData};
use atomcad_structure_designer::record_path::{PathSpec, resolve_path};
use atomcad_structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Getters
// ============================================================================

/// The panel view of a `record_get` node, or `None` if the node does not
/// exist or is not a `record_get`.
#[flutter_rust_bridge::frb(ignore)]
pub fn record_get_view(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIRecordPathNodeData> {
    let node = designer
        .get_scope_network(scope_path)?
        .nodes
        .get(&node_id)?;
    let data = node.data.as_any_ref().downcast_ref::<RecordGetData>()?;
    let defs = designer.node_type_registry.record_defs();
    Some(APIRecordPathNodeData {
        schema: data.schema.clone(),
        fields: local_fields_view(&data.fields),
        rows: rows_view(&data.root(), &data.rows, &[], defs, |_, _, _| None),
    })
}

/// The panel view of a `record_set` node, its rows' literals included, or
/// `None` if the node does not exist or is not a `record_set`.
#[flutter_rust_bridge::frb(ignore)]
pub fn record_set_view(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIRecordPathNodeData> {
    let node = designer
        .get_scope_network(scope_path)?
        .nodes
        .get(&node_id)?;
    let data = node.data.as_any_ref().downcast_ref::<RecordSetData>()?;
    let defs = designer.node_type_registry.record_defs();
    let root = data.root();
    Some(APIRecordPathNodeData {
        schema: data.schema.clone(),
        fields: local_fields_view(&data.fields),
        rows: rows_view(
            &root,
            &data.rows,
            &RESERVED_PIN_NAMES,
            defs,
            |row_index, pin_name, pin_type| {
                let row = &data.rows[row_index];
                let simple_type = data_type_to_simple_param_type(pin_type)?;
                Some(APILiteralField {
                    name: pin_name.to_string(),
                    data_type: simple_type,
                    stored_value: row
                        .literal
                        .as_ref()
                        .and_then(|tv| text_value_to_api_literal(tv, pin_type)),
                    // No default layer: an unset row leaves the field unchanged.
                    default_value: None,
                    is_wired: row_pin_wired(node, row.row.id),
                    hint: leaf_hint(&root, &row.row, defs)
                        .as_ref()
                        .map(field_editor_hint_to_api),
                })
            },
        ),
    })
}

/// An anonymous node's root fields, for the read-only "Type from wire" list
/// (empty in the other modes, which have none).
fn local_fields_view(fields: &[LocalRecordField]) -> Vec<APIRecordLocalField> {
    fields
        .iter()
        .map(|field| APIRecordLocalField {
            name: field.name.clone(),
            data_type: data_type_to_api_data_type(&field.data_type),
        })
        .collect()
}

/// The rows as the panel shows them. `literal(row_index, pin_name,
/// pin_type)` builds a resolving row's literal part (`record_set` only).
fn rows_view<R: AsRef<RecordPathRow>>(
    root: &RecordType,
    rows: &[R],
    reserved: &[&str],
    defs: RecordDefs<'_>,
    literal: impl Fn(usize, &str, &DataType) -> Option<APILiteralField>,
) -> Vec<APIRecordPathRow> {
    let pins = row_pins(root, rows, defs, reserved, MissingDefs::Dangle).unwrap_or_default();
    rows.iter()
        .enumerate()
        .map(|(row_index, row)| {
            let row = row.as_ref();
            let pin = pins.iter().find(|pin| pin.row_index == row_index);
            let path = match row_spec(root, row, defs) {
                PathSpec::Live(names) => APIRecordPath {
                    names,
                    dangling: false,
                },
                PathSpec::Dangling(names) => APIRecordPath {
                    names,
                    dangling: true,
                },
            };
            APIRecordPathRow {
                path,
                pin_name: pin.map(|pin| pin.name.clone()),
                leaf_type: pin.map(|pin| pin.leaf_type.to_string()),
                problem: row_problem(root, row, defs),
                literal: pin.and_then(|pin| literal(row_index, &pin.name, &pin.pin_type())),
            }
        })
        .collect()
}

/// Whether the input pin carrying row id `row_id` has a wire. Looked up by
/// pin id in the node's installed layout, so it is right for a frozen node
/// too, whose pins come from a recorded interface.
fn row_pin_wired(node: &Node, row_id: Option<u64>) -> bool {
    let Some(row_id) = row_id else {
        return false;
    };
    node.custom_node_type
        .as_ref()
        .and_then(|nt| nt.parameters.iter().position(|p| p.id == Some(row_id)))
        .and_then(|index| node.arguments.get(index))
        .is_some_and(|arg| !arg.is_empty())
}

/// The editor hint of the field `row` ends on, when that field is in a named
/// def (an anonymous record type has none).
fn leaf_hint(
    root: &RecordType,
    row: &RecordPathRow,
    defs: RecordDefs<'_>,
) -> Option<FieldEditorHint> {
    let (leaf, parent) = row.path.split_last()?;
    let def_name = if parent.is_empty() {
        match root {
            RecordType::Named(name) => name.clone(),
            RecordType::Anonymous(_) => return None,
        }
    } else {
        match resolve_path(root, parent, defs).ok()?.leaf_type {
            DataType::Record(RecordType::Named(name)) => name,
            _ => return None,
        }
    };
    let def = defs.lookup(&def_name)?;
    def.fields
        .iter()
        .find(|f| Some(f.id.0) == leaf.field_id)?
        .hint
        .clone()
}

/// The fields of `schema` as a tree, for the path picker: a record-typed field
/// (a named def that exists, or an anonymous record type) has its own fields as
/// children. Empty if `schema` names no def. The def graph is acyclic, so the
/// recursion ends.
#[flutter_rust_bridge::frb(ignore)]
pub fn record_field_tree(designer: &StructureDesigner, schema: &str) -> Vec<APIRecordFieldNode> {
    record_field_tree_for_root(designer, &RecordType::Named(schema.to_string()))
}

/// [`record_field_tree`] from a root record type: a def (in its authored field
/// order), or an anonymous type (in its canonical order). Empty for a missing
/// def and for the unset node's `{}`.
#[flutter_rust_bridge::frb(ignore)]
pub fn record_field_tree_for_root(
    designer: &StructureDesigner,
    root: &RecordType,
) -> Vec<APIRecordFieldNode> {
    let defs = designer.node_type_registry.record_defs();
    match root {
        RecordType::Named(schema) => match defs.lookup(schema) {
            Some(def) => def
                .fields
                .iter()
                .map(|f| field_node(&f.name, &f.data_type, defs))
                .collect(),
            None => Vec::new(),
        },
        RecordType::Anonymous(fields) => fields
            .iter()
            .map(|(name, data_type)| field_node(name, data_type, defs))
            .collect(),
    }
}

/// The path picker's tree for the `record_get` / `record_set` node `node_id`,
/// from its root. `None` if the node is neither.
#[flutter_rust_bridge::frb(ignore)]
pub fn record_path_node_field_tree(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<Vec<APIRecordFieldNode>> {
    let data = &designer
        .get_scope_network(scope_path)?
        .nodes
        .get(&node_id)?
        .data;
    let root = if let Some(get) = data.as_any_ref().downcast_ref::<RecordGetData>() {
        get.root()
    } else {
        data.as_any_ref().downcast_ref::<RecordSetData>()?.root()
    };
    Some(record_field_tree_for_root(designer, &root))
}

fn field_node(name: &str, data_type: &DataType, defs: RecordDefs<'_>) -> APIRecordFieldNode {
    let children: Option<Vec<APIRecordFieldNode>> = match data_type {
        DataType::Record(RecordType::Named(n)) => defs.lookup(n).map(|def| {
            def.fields
                .iter()
                .map(|f| field_node(&f.name, &f.data_type, defs))
                .collect()
        }),
        DataType::Record(RecordType::Anonymous(fields)) => {
            Some(fields.iter().map(|(n, t)| field_node(n, t, defs)).collect())
        }
        _ => None,
    };
    APIRecordFieldNode {
        name: name.to_string(),
        data_type: data_type.to_string(),
        steppable: children.is_some(),
        children: children.unwrap_or_default(),
    }
}

// ============================================================================
// Setters
// ============================================================================

fn path_specs(paths: Vec<APIRecordPath>) -> Vec<PathSpec> {
    paths
        .into_iter()
        .map(|p| {
            if p.dangling {
                PathSpec::Dangling(p.names)
            } else {
                PathSpec::Live(p.names)
            }
        })
        .collect()
}

/// Sets a `record_get`'s schema and rows (`StructureDesigner::set_record_get_data`).
#[flutter_rust_bridge::frb(ignore)]
pub fn edit_record_get(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
    schema: String,
    paths: Vec<APIRecordPath>,
) -> Result<(), String> {
    designer.set_record_get_data(scope_path, node_id, schema, path_specs(paths))
}

/// Sets a `record_set`'s schema and rows (`StructureDesigner::set_record_set_data`).
#[flutter_rust_bridge::frb(ignore)]
pub fn edit_record_set(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
    schema: String,
    paths: Vec<APIRecordPath>,
) -> Result<(), String> {
    designer.set_record_set_data(scope_path, node_id, schema, path_specs(paths))
}

/// Stores (`Some`) or clears (`None`) the literal of the `record_set` row at
/// `row_index`. A plain node-data edit with the ordinary `SetNodeData` undo.
/// Refused for a row without a pin, and for a literal that does not convert
/// to the row's pin type. Clearing a row that has no literal changes nothing.
#[flutter_rust_bridge::frb(ignore)]
pub fn edit_record_set_literal(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
    row_index: usize,
    value: Option<APILiteralValue>,
) -> Result<(), String> {
    designer.ensure_active_editable()?;
    let mut data = designer
        .get_scope_network(scope_path)
        .and_then(|network| network.nodes.get(&node_id))
        .ok_or_else(|| "Node not found".to_string())?
        .data
        .as_any_ref()
        .downcast_ref::<RecordSetData>()
        .ok_or_else(|| "Node is not a record_set".to_string())?
        .clone();
    let defs = designer.node_type_registry.record_defs();
    let root = data.root();
    let row = data
        .rows
        .get_mut(row_index)
        .ok_or_else(|| format!("record_set has no row {}", row_index))?;
    let literal = match value {
        Some(value) => {
            if resolve_path(&root, &row.row.path, defs).is_err() {
                return Err("the row's path does not resolve, so it has no pin".to_string());
            }
            let literal = api_literal_to_text_value(value);
            if !literal_fits(&root, &row.row, &literal, defs) {
                return Err(format!(
                    "{} does not fit the field's type",
                    literal.to_text()
                ));
            }
            Some(literal)
        }
        None => None,
    };
    if row.literal == literal {
        return Ok(());
    }
    row.literal = literal;
    designer.set_node_network_data_scoped(scope_path, node_id, Box::new(data));
    Ok(())
}

fn api_result(result: Result<(), String>) -> APIResult {
    match result {
        Ok(()) => APIResult {
            success: true,
            error_message: String::new(),
        },
        Err(error_message) => APIResult {
            success: false,
            error_message,
        },
    }
}

fn no_instance() -> APIResult {
    APIResult {
        success: false,
        error_message: "No CAD instance".to_string(),
    }
}

// ============================================================================
// FRB entry points
// ============================================================================

/// The `record_get` panel's data: the schema and each row's path (current
/// names; stored names and `dangling: true` for a dangling row), pin name,
/// leaf type and problem. `None` if the node is not a `record_get`.
#[flutter_rust_bridge::frb(sync)]
pub fn get_record_get_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIRecordPathNodeData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| record_get_view(&cad_instance.structure_designer, &scope_path, node_id),
            None,
        )
    }
}

/// Edits a `record_get`'s schema and rows. `paths` is the getter's row list
/// with the panel's change applied: a dangling row sent back is kept, one left
/// out is deleted. One undo step; an edit that changes nothing pushes none.
#[flutter_rust_bridge::frb(sync)]
pub fn set_record_get_data(
    scope_path: Vec<u64>,
    node_id: u64,
    schema: String,
    paths: Vec<APIRecordPath>,
) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = edit_record_get(
                    &mut cad_instance.structure_designer,
                    &scope_path,
                    node_id,
                    schema,
                    paths,
                );
                refresh_structure_designer_auto(cad_instance);
                api_result(result)
            },
            no_instance(),
        )
    }
}

/// The `record_set` panel's data: as `get_record_get_data`, plus each row's
/// inline literal. `None` if the node is not a `record_set`.
#[flutter_rust_bridge::frb(sync)]
pub fn get_record_set_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIRecordPathNodeData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| record_set_view(&cad_instance.structure_designer, &scope_path, node_id),
            None,
        )
    }
}

/// Edits a `record_set`'s schema and rows; same contract as
/// `set_record_get_data`. A row keeps its literal while the literal still fits.
#[flutter_rust_bridge::frb(sync)]
pub fn set_record_set_data(
    scope_path: Vec<u64>,
    node_id: u64,
    schema: String,
    paths: Vec<APIRecordPath>,
) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = edit_record_set(
                    &mut cad_instance.structure_designer,
                    &scope_path,
                    node_id,
                    schema,
                    paths,
                );
                refresh_structure_designer_auto(cad_instance);
                api_result(result)
            },
            no_instance(),
        )
    }
}

/// Stores the literal of the `record_set` row at `row_index` (an index into
/// the getter's rows). Rejected when it does not fit the row's type.
#[flutter_rust_bridge::frb(sync)]
pub fn set_record_set_literal(
    scope_path: Vec<u64>,
    node_id: u64,
    row_index: u32,
    value: APILiteralValue,
) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = edit_record_set_literal(
                    &mut cad_instance.structure_designer,
                    &scope_path,
                    node_id,
                    row_index as usize,
                    Some(value),
                );
                refresh_structure_designer_auto(cad_instance);
                api_result(result)
            },
            no_instance(),
        )
    }
}

/// Clears the literal of the `record_set` row at `row_index`.
#[flutter_rust_bridge::frb(sync)]
pub fn clear_record_set_literal(scope_path: Vec<u64>, node_id: u64, row_index: u32) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = edit_record_set_literal(
                    &mut cad_instance.structure_designer,
                    &scope_path,
                    node_id,
                    row_index as usize,
                    None,
                );
                refresh_structure_designer_auto(cad_instance);
                api_result(result)
            },
            no_instance(),
        )
    }
}

/// The path picker's tree for a `record_get` / `record_set` node, from its
/// root: its schema's fields, or those of the type it took from its wire.
/// Empty when the node is unset or is neither.
#[flutter_rust_bridge::frb(sync)]
pub fn get_record_path_field_tree(scope_path: Vec<u64>, node_id: u64) -> Vec<APIRecordFieldNode> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                record_path_node_field_tree(&cad_instance.structure_designer, &scope_path, node_id)
                    .unwrap_or_default()
            },
            Vec::new(),
        )
    }
}

/// Point a `record_get` / `record_set` at the def `schema`, or at none (`""`,
/// which makes it unset), from its schema dropdown — one undo step, "Set
/// record schema" (`StructureDesigner::set_record_input_schema`). Rows move
/// onto the new root by their current names; a row that does not resolve
/// there is kept as a broken row.
#[flutter_rust_bridge::frb(sync)]
pub fn set_record_path_node_schema(
    scope_path: Vec<u64>,
    node_id: u64,
    schema: String,
) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = cad_instance.structure_designer.set_record_input_schema(
                    &scope_path,
                    node_id,
                    &schema,
                );
                refresh_structure_designer_auto(cad_instance);
                api_result(result)
            },
            no_instance(),
        )
    }
}
