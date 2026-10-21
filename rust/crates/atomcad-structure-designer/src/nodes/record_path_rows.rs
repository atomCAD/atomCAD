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
use crate::nodes::variadic_ids::merge_ids_by_optional_key;
use crate::record_path::{
    DanglingReason, PathSegment, PathSpec, ResolvedPath, display_names, format_path,
    resolve_names_allowing_missing_defs, resolve_names_partially, resolve_path,
};
use crate::text_format::TextValue;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

impl AsMut<RecordPathRow> for RecordPathRow {
    fn as_mut(&mut self) -> &mut RecordPathRow {
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

/// The result of [`merge_rows`].
#[derive(Debug, Clone)]
pub struct MergedRows {
    /// The new rows, in spec order.
    pub rows: Vec<RecordPathRow>,
    /// Per new row, the index of the old row it continues — whose id it has
    /// (and, for a claimed row, everything else) — if any. A `record_set`
    /// carries that row's literal over from here.
    pub sources: Vec<Option<usize>>,
}

/// Two paths are the same key when their segments match by `field_id` where
/// both sides have one, and by name otherwise.
fn same_path(a: &[PathSegment], b: &[PathSegment]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| match (x.field_id, y.field_id) {
                (Some(i), Some(j)) => i == j,
                _ => x.name == y.name,
            })
}

/// The error for a live path whose names do not resolve: `depth` is the
/// first name that fails.
fn unresolved_path_error(schema: &str, names: &[String], depth: usize) -> String {
    format!(
        "path `{}`: `{}` is not a field of `{}`",
        format_path(names),
        names[depth],
        if depth == 0 {
            schema.to_string()
        } else {
            format_path(&names[..depth])
        }
    )
}

/// Moves `rows` from `old_schema` onto `new_schema`, keeping their ids: each
/// row is re-resolved **by its current names** under the old schema (its
/// [`row_spec`] names — the stored names only for a row already dangling),
/// never by the stored names, which go stale on renames. A row whose names do
/// not resolve against the new schema becomes dangling
/// ([`resolve_names_partially`]).
pub fn rebase_rows<R: AsRef<RecordPathRow>>(
    old_schema: &str,
    new_schema: &str,
    rows: &[R],
    defs: RecordDefs<'_>,
) -> Vec<RecordPathRow> {
    rows.iter()
        .map(|row| {
            let row = row.as_ref();
            let names = match row_spec(old_schema, row, defs) {
                PathSpec::Live(names) | PathSpec::Dangling(names) => names,
            };
            RecordPathRow {
                id: row.id,
                path: resolve_names_partially(new_schema, &names, defs),
            }
        })
        .collect()
}

/// Edits a node's rows from by-name specs (the text format, the
/// `StructureDesigner` setters), preserving ids so wires follow their rows
/// (`doc/design_record_get_set.md`, "Row editing"). `specs: None` keeps the
/// rows (a schema-only edit).
///
/// 1. **Checks.** An empty or duplicate `Live` path, a `Live` path that does
///    not resolve (and claims no row, below), or paths without a schema are
///    errors; nothing is changed.
/// 2. **Claims.** A `Dangling` spec claims the first unclaimed old dangling
///    row with the same stored names, kept verbatim. One that matches none is
///    **recreated** as a dangling row with a fresh id whose segments resolve
///    to nothing ([`PathSegment::gone`]): a `--replace` builds every node
///    afresh, so this is how its dangling rows survive the round trip. When
///    the **schema changes**, every spec instead
///    claims the old row it names under the old schema (its current names,
///    as the getter and serializer print them): the row is moved onto the new
///    schema by [`rebase_rows`], keeps its id, and goes dangling if its names
///    do not resolve there.
/// 3. **Merge.** The remaining `Live` specs go through the three-pass id
///    merge ([`merge_ids_by_optional_key`]): path match, positional fallback
///    (indices count every spec and every old row), mint. Claimed specs and
///    rows take no part. A path match keeps the old row's path verbatim, so
///    echoing a node's rows back changes nothing.
pub fn merge_rows<R: AsRef<RecordPathRow>>(
    old_schema: &str,
    old_rows: &[R],
    schema: &str,
    specs: Option<&[PathSpec]>,
    defs: RecordDefs<'_>,
    next_row_id: &mut u64,
) -> Result<MergedRows, String> {
    let changed = old_schema != schema;
    let old_specs: Vec<PathSpec> = old_rows
        .iter()
        .map(|row| row_spec(old_schema, row.as_ref(), defs))
        .collect();
    let base: Vec<RecordPathRow> = if changed {
        rebase_rows(old_schema, schema, old_rows, defs)
    } else {
        old_rows.iter().map(|row| row.as_ref().clone()).collect()
    };
    let specs: Vec<PathSpec> = match specs {
        Some(specs) => specs.to_vec(),
        None if !changed => {
            return Ok(MergedRows {
                sources: (0..base.len()).map(Some).collect(),
                rows: base,
            });
        }
        None => old_specs.clone(),
    };

    // 1. Checks that need no resolution.
    if schema.is_empty() && !specs.is_empty() {
        return Err("paths need a schema".to_string());
    }
    let mut seen: Vec<&Vec<String>> = Vec::new();
    for spec in &specs {
        if let PathSpec::Live(names) = spec {
            if names.is_empty() {
                return Err("empty path".to_string());
            }
            if seen.contains(&names) {
                return Err(format!("duplicate path `{}`", format_path(names)));
            }
            seen.push(names);
        }
    }

    // 2. Claims.
    let mut claimed = vec![false; base.len()];
    let mut claims: Vec<Option<usize>> = vec![None; specs.len()];
    for (si, spec) in specs.iter().enumerate() {
        if !changed && matches!(spec, PathSpec::Live(_)) {
            continue;
        }
        if let Some(oi) = (0..base.len()).find(|&oi| !claimed[oi] && old_specs[oi] == *spec) {
            claimed[oi] = true;
            claims[si] = Some(oi);
        }
    }

    // The rest of the live specs must resolve now.
    let mut keys: Vec<Option<Vec<PathSegment>>> = Vec::with_capacity(specs.len());
    for (si, spec) in specs.iter().enumerate() {
        match spec {
            PathSpec::Live(names) if claims[si].is_none() => {
                let segments = resolve_names_allowing_missing_defs(schema, names, defs)
                    .map_err(|depth| unresolved_path_error(schema, names, depth))?;
                keys.push(Some(segments));
            }
            _ => keys.push(None),
        }
    }

    // 3. Merge.
    let old_keys: Vec<(Option<u64>, Vec<PathSegment>)> = base
        .iter()
        .zip(&claimed)
        .map(|(row, &claimed)| (if claimed { None } else { row.id }, row.path.clone()))
        .collect();
    let ids = merge_ids_by_optional_key(&old_keys, &keys, |a, b| same_path(a, b), next_row_id);

    let mut rows: Vec<RecordPathRow> = Vec::with_capacity(specs.len());
    let mut sources = Vec::with_capacity(specs.len());
    for (si, key) in keys.into_iter().enumerate() {
        if let Some(oi) = claims[si] {
            rows.push(base[oi].clone());
            sources.push(Some(oi));
            continue;
        }
        let (Some(path), Some(id)) = (key, ids[si]) else {
            // An unclaimed dangling spec: recreate the row. Two identical
            // ones would be identical rows, which the loader's healing drops
            // anyway, so the second is dropped here.
            let PathSpec::Dangling(names) = &specs[si] else {
                unreachable!("every live spec has a key")
            };
            let path: Vec<PathSegment> = names.iter().cloned().map(PathSegment::gone).collect();
            if !rows.iter().any(|row| row.path == path) {
                rows.push(RecordPathRow {
                    id: Some(*next_row_id),
                    path,
                });
                *next_row_id += 1;
                sources.push(None);
            }
            continue;
        };
        let source = (0..base.len()).find(|&oi| !claimed[oi] && base[oi].id == Some(id));
        let path = match source {
            Some(oi) if same_path(&base[oi].path, &path) => base[oi].path.clone(),
            _ => path,
        };
        rows.push(RecordPathRow { id: Some(id), path });
        sources.push(source);
    }
    Ok(MergedRows { rows, sources })
}

/// Loader healing (`doc/design_record_get_set.md`, "Serialization and
/// healing"): drops rows with an empty path and later duplicates of a path
/// (identical stored segments — the loader has no defs, so this is never a
/// false positive), bumps `next_row_id` past every id, and gives id-less rows
/// fresh ids. Idempotent.
pub fn heal_rows<R: AsRef<RecordPathRow> + AsMut<RecordPathRow>>(
    rows: &mut Vec<R>,
    next_row_id: &mut u64,
) {
    let mut kept: Vec<R> = Vec::with_capacity(rows.len());
    for row in rows.drain(..) {
        let path = &row.as_ref().path;
        if path.is_empty() || kept.iter().any(|k| k.as_ref().path == *path) {
            continue;
        }
        kept.push(row);
    }
    *rows = kept;
    if let Some(max_id) = rows.iter().filter_map(|r| r.as_ref().id).max()
        && *next_row_id <= max_id
    {
        *next_row_id = max_id + 1;
    }
    for row in rows.iter_mut() {
        let row = row.as_mut();
        if row.id.is_none() {
            row.id = Some(*next_row_id);
            *next_row_id += 1;
        }
    }
}

/// Per row, the pin name the text format writes a `record_set` literal
/// under (`None` for a dangling row). Like [`row_pins`]' names, except that a
/// row through a **missing** def is named too, from its display names: a
/// frozen node keeps that row's pin from its recorded interface, and its
/// literal must survive a `query` → `--replace`.
pub fn text_pin_names<R: AsRef<RecordPathRow>>(
    schema: &str,
    rows: &[R],
    defs: RecordDefs<'_>,
    reserved: &[&str],
) -> Vec<Option<String>> {
    let resolved: Vec<Option<ResolvedPath>> = rows
        .iter()
        .map(|row| {
            let path = &row.as_ref().path;
            match resolve_path(schema, path, defs) {
                Ok(r) => Some(r),
                Err(DanglingReason::DefMissing(_)) => Some(ResolvedPath {
                    names: display_names(schema, path, defs),
                    leaf_type: DataType::None,
                }),
                Err(_) => None,
            }
        })
        .collect();
    derived_pin_names(&resolved, reserved)
}

/// Whether `literal` can be a row's stored value: it converts to the row's
/// pin type. A row that does not resolve has no known type, and its literal
/// is kept unchecked.
pub fn literal_fits(
    schema: &str,
    row: &RecordPathRow,
    literal: &TextValue,
    defs: RecordDefs<'_>,
) -> bool {
    match resolve_path(schema, &row.path, defs) {
        Ok(r) => literal
            .to_network_result(&r.leaf_type.record_field_pin_type())
            .is_some(),
        Err(_) => true,
    }
}
