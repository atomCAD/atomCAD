//! Path rows, shared by `record_get` and `record_set`
//! (`doc/design_record_get_set.md`): one row per user-chosen path into the
//! node's schema, each with a hidden stable id that its pin carries, so wires
//! survive path edits and the removal of other rows.
//!
//! A row whose path no longer resolves ([`resolve_path`]) is **dangling**: it
//! stays in the node data, gets no pin, and is reported by
//! [`dangling_row_problems`] as a non-blocking warning. Pin names come from
//! [`derived_pin_names`]; the pins themselves from [`row_pins`], which is the
//! one place both nodes' `build_record_node_type` and `eval` agree on which
//! rows have a pin and in what order.

use crate::data_type::DataType;
use crate::node_data::MissingDefs;
use crate::node_type_registry::RecordDefs;
use crate::record_path::{
    DanglingReason, PathSegment, PathSpec, ResolvedPath, display_names, format_path,
    resolve_names_allowing_missing_defs, resolve_path,
};
use crate::text_format::TextValue;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordPathRow {
    /// Hidden stable identity: the pin id. Wires survive path edits,
    /// reorders, and removals of other rows.
    #[serde(default)]
    pub id: Option<u64>,
    /// Never empty.
    pub path: Vec<PathSegment>,
}

/// The helpers below take a bare [`RecordPathRow`] (`record_get`) or a row
/// that wraps one with more per-row state (`record_set`'s literal).
impl AsRef<RecordPathRow> for RecordPathRow {
    fn as_ref(&self) -> &RecordPathRow {
        self
    }
}

impl RecordPathRow {
    /// The stored segment names: a dangling row's identity.
    pub fn stored_names(&self) -> Vec<String> {
        self.path.iter().map(|s| s.name.clone()).collect()
    }
}

/// The pin a resolving row gets.
#[derive(Debug, Clone, PartialEq)]
pub struct RowPin {
    /// Index of the row in the node's row list.
    pub row_index: usize,
    pub id: Option<u64>,
    /// Derived, deduplicated pin name.
    pub name: String,
    /// The row's current segment names.
    pub names: Vec<String>,
    /// The leaf's declared type (may be `Optional[T]`).
    pub leaf_type: DataType,
}

impl RowPin {
    /// The pin's type: an `Optional[T]` leaf is a plain `T` pin
    /// (`DataType::record_field_pin_type`).
    pub fn pin_type(&self) -> DataType {
        self.leaf_type.record_field_pin_type()
    }
}

/// Pin names for the rows, in row order (`None` for a dangling row). The base
/// name is the current segment names joined with `_` (`p.x` → `p_x`), not
/// sanitized: the text format backtick-quotes pin names that are not
/// identifiers. Collisions are resolved in row order like `switch`'s: the
/// first holder keeps the name, later holders get `__2`, `__3`, …. A name in
/// `reserved` counts as already taken. Dangling rows take no part.
pub fn derived_pin_names(
    resolved: &[Option<ResolvedPath>],
    reserved: &[&str],
) -> Vec<Option<String>> {
    let mut taken: HashSet<String> = reserved.iter().map(|s| s.to_string()).collect();
    resolved
        .iter()
        .map(|r| {
            let r = r.as_ref()?;
            let base = r.names.join("_");
            let name = if !taken.contains(&base) {
                base
            } else {
                (2..)
                    .map(|k| format!("{}__{}", base, k))
                    .find(|n| !taken.contains(n))
                    .unwrap()
            };
            taken.insert(name.clone());
            Some(name)
        })
        .collect()
}

/// Resolves every row against `schema`.
pub fn resolve_rows<R: AsRef<RecordPathRow>>(
    schema: &str,
    rows: &[R],
    defs: RecordDefs<'_>,
) -> Vec<Result<ResolvedPath, DanglingReason>> {
    rows.iter()
        .map(|row| resolve_path(schema, &row.as_ref().path, defs))
        .collect()
}

/// The row-to-pins builder: one [`RowPin`] per resolving row, in row order.
/// `None` only under [`MissingDefs::Refuse`] when the schema or a def a row
/// walks into is missing (a frozen node keeps its cached layout then).
pub fn row_pins<R: AsRef<RecordPathRow>>(
    schema: &str,
    rows: &[R],
    defs: RecordDefs<'_>,
    reserved: &[&str],
    missing: MissingDefs,
) -> Option<Vec<RowPin>> {
    if missing == MissingDefs::Refuse && defs.lookup(schema).is_none() {
        return None;
    }
    let results = resolve_rows(schema, rows, defs);
    if missing == MissingDefs::Refuse
        && results
            .iter()
            .any(|r| matches!(r, Err(DanglingReason::DefMissing(_))))
    {
        return None;
    }
    let resolved: Vec<Option<ResolvedPath>> = results.into_iter().map(Result::ok).collect();
    let names = derived_pin_names(&resolved, reserved);
    Some(
        resolved
            .into_iter()
            .zip(names)
            .enumerate()
            .filter_map(|(row_index, (r, name))| {
                let r = r?;
                Some(RowPin {
                    row_index,
                    id: rows[row_index].as_ref().id,
                    name: name?,
                    names: r.names,
                    leaf_type: r.leaf_type,
                })
            })
            .collect(),
    )
}

/// One warning per dangling row, built from the row's stored names.
pub fn dangling_row_problems<R: AsRef<RecordPathRow>>(
    schema: &str,
    rows: &[R],
    defs: RecordDefs<'_>,
) -> Vec<String> {
    rows.iter()
        .filter_map(|row| {
            let row = row.as_ref();
            let reason = resolve_path(schema, &row.path, defs).err()?;
            let path = format_path(&row.stored_names());
            Some(match reason {
                DanglingReason::DefMissing(def) if def == schema => {
                    format!("path `{}`: record type `{}` does not exist", path, def)
                }
                DanglingReason::DefMissing(def) => format!(
                    "path `{}` no longer exists in `{}`: record type `{}` does not exist",
                    path, schema, def
                ),
                DanglingReason::FieldGone { .. } => {
                    format!("path `{}` no longer exists in `{}`", path, schema)
                }
                DanglingReason::NotARecord { depth } => format!(
                    "path `{}` no longer exists in `{}`: `{}` is not a record",
                    path,
                    schema,
                    format_path(&row.stored_names()[..=depth])
                ),
            })
        })
        .collect()
}

/// The defs below `schema` that the rows walk into (deduplicated, first-seen
/// order), including a missing one. For `NodeData::record_path_defs`.
pub fn rows_path_defs<R: AsRef<RecordPathRow>>(
    schema: &str,
    rows: &[R],
    defs: RecordDefs<'_>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for row in rows {
        for name in crate::record_path::path_defs(schema, &row.as_ref().path, defs) {
            if name != schema && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// A row as the text format and the API name it. A live row by its current
/// names; a dangling row by its stored names (`Dangling`). A row that does
/// not resolve because a **def is missing** is written as a live path (its
/// resolvable prefix by current names, the rest by stored names): the def may
/// only be unavailable — a frozen node under an unloaded library — and the
/// path itself is not known to be broken.
pub fn row_spec(schema: &str, row: &RecordPathRow, defs: RecordDefs<'_>) -> PathSpec {
    match resolve_path(schema, &row.path, defs) {
        Ok(r) => PathSpec::Live(r.names),
        Err(DanglingReason::DefMissing(_)) => {
            PathSpec::Live(display_names(schema, &row.path, defs))
        }
        Err(_) => PathSpec::Dangling(row.stored_names()),
    }
}

/// The `paths` text property of a node's rows.
pub fn paths_text_value<R: AsRef<RecordPathRow>>(
    schema: &str,
    rows: &[R],
    defs: RecordDefs<'_>,
) -> TextValue {
    TextValue::Array(
        rows.iter()
            .map(|row| {
                TextValue::String(crate::record_path::format_path_spec(&row_spec(
                    schema,
                    row.as_ref(),
                    defs,
                )))
            })
            .collect(),
    )
}

/// The `paths` text property without the defs (stored names only, every row
/// written as live). Only for callers that have no registry.
pub fn paths_text_value_stored<R: AsRef<RecordPathRow>>(rows: &[R]) -> TextValue {
    TextValue::Array(
        rows.iter()
            .map(|row| TextValue::String(format_path(&row.as_ref().stored_names())))
            .collect(),
    )
}

/// Parses a `paths` text property into specs.
pub fn parse_paths(value: &TextValue) -> Result<Vec<PathSpec>, String> {
    let items = value
        .as_array()
        .ok_or_else(|| "paths must be an array of strings".to_string())?;
    items
        .iter()
        .map(|item| {
            let text = item
                .as_string()
                .ok_or_else(|| "paths must be an array of strings".to_string())?;
            crate::record_path::parse_path_spec(text)
        })
        .collect()
}

/// Replaces the rows **wholesale** from live specs: each path is resolved by
/// name and gets a freshly minted id. Rejects a `Dangling` spec, an empty or
/// duplicate path, and a name that does not resolve; on error nothing is
/// changed. A path through a def that is *missing* (as opposed to a name not
/// in a def that exists) is kept by name
/// ([`resolve_names_allowing_missing_defs`]), so a frozen node survives a
/// text-format `--replace`.
///
/// This is the minimal Phase 1 setter; `merge_rows` (Phase 3) replaces it
/// with an id-preserving merge.
pub fn replace_rows(
    schema: &str,
    specs: &[PathSpec],
    defs: RecordDefs<'_>,
    next_row_id: &mut u64,
) -> Result<Vec<RecordPathRow>, String> {
    if schema.is_empty() && !specs.is_empty() {
        return Err("paths need a schema".to_string());
    }
    let mut paths: Vec<Vec<PathSegment>> = Vec::with_capacity(specs.len());
    let mut seen: Vec<&Vec<String>> = Vec::new();
    for spec in specs {
        let names = match spec {
            PathSpec::Live(names) => names,
            PathSpec::Dangling(names) => {
                return Err(format!(
                    "dangling path `!{}` cannot be set here",
                    format_path(names)
                ));
            }
        };
        if names.is_empty() {
            return Err("empty path".to_string());
        }
        if seen.contains(&names) {
            return Err(format!("duplicate path `{}`", format_path(names)));
        }
        seen.push(names);
        let segments =
            resolve_names_allowing_missing_defs(schema, names, defs).map_err(|depth| {
                format!(
                    "path `{}`: `{}` is not a field of {}",
                    format_path(names),
                    names[depth],
                    if depth == 0 {
                        format!("`{}`", schema)
                    } else {
                        format!("`{}`", format_path(&names[..depth]))
                    }
                )
            })?;
        paths.push(segments);
    }
    Ok(paths
        .into_iter()
        .map(|path| {
            let id = *next_row_id;
            *next_row_id += 1;
            RecordPathRow { id: Some(id), path }
        })
        .collect())
}
