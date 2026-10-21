//! Paths into nested record types (`doc/design_record_get_set.md`): the one
//! resolver every user of a `record_get` / `record_set` row goes through, and
//! the path text syntax.
//!
//! A path is a list of [`PathSegment`]s starting at a record def (the node's
//! `schema`). A segment inside a **named** def is identified by its field's
//! [`FieldId`](crate::node_type_registry::FieldId), so a field rename anywhere
//! along the path keeps the path; inside an **anonymous** record type, which
//! has no ids, it is identified by name. The stored names are a snapshot taken
//! when the row was last edited: current names always come from
//! [`resolve_path`].
//!
//! Text syntax: segments joined with `.`; a segment containing `.` is written
//! in backticks (`` `a.b`.c ``). A leading `!` marks a dangling row
//! ([`PathSpec::Dangling`]); a first segment that itself starts with `!` is
//! written in backticks, so the marker is unambiguous. Field names cannot
//! contain a backtick (`identifier::is_valid_user_name`), so the quoting needs
//! no escape.

use crate::data_type::{DataType, RecordType};
use crate::node_type_registry::RecordDefs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathSegment {
    /// The field's stable identity when the containing record type is a
    /// named def. `None` inside an anonymous record type, which has no
    /// `FieldId`s — resolved by `name` there.
    #[serde(default)]
    pub field_id: Option<u64>,
    /// The field's name when the row was last edited. Used to resolve a
    /// segment with no `field_id`, and as the identity of a dangling row (its
    /// warning, its `!path` text form). Never used to resolve a segment that
    /// has an id, and not kept up to date on renames.
    pub name: String,
}

impl PathSegment {
    /// The `field_id` of a segment whose field is known to be gone, but not
    /// which field it was: a dangling row recreated from its `!path` text by a
    /// node that never held it (a `--replace` builds every node afresh). Real
    /// ids count up from 0 per def, so it matches no field, and the row stays
    /// dangling even if a field of the same name is added later — as a row
    /// that lost its field to a delete does.
    pub const GONE_FIELD_ID: u64 = u64::MAX;

    /// A segment that resolves to nothing (see [`Self::GONE_FIELD_ID`]).
    pub fn gone(name: String) -> Self {
        PathSegment {
            field_id: Some(Self::GONE_FIELD_ID),
            name,
        }
    }
}

/// A path that resolves: its current segment names and the leaf field's
/// declared type (possibly `Optional[T]`).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPath {
    pub names: Vec<String>,
    pub leaf_type: DataType,
}

/// Why a path no longer resolves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DanglingReason {
    /// A def the path needs (the schema, or a nested def) is not in the defs.
    DefMissing(String),
    /// Segment `depth`'s field is not in its record type any more.
    FieldGone { depth: usize },
    /// Segment `depth` is an inner segment whose field is no longer a record
    /// type, so the path cannot step through it.
    NotARecord { depth: usize },
}

/// A path as the text format and the API carry it: by names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSpec {
    /// A path that must resolve against the schema now.
    Live(Vec<String>),
    /// An existing dangling row, named by its stored segment names
    /// (`PathSegment::name`). Never resolved.
    Dangling(Vec<String>),
}

/// One step of a walk: the field a segment landed on.
struct Step<'a> {
    field_id: Option<u64>,
    name: &'a str,
    data_type: &'a DataType,
}

/// The fields of the record type `t`, as steps, or `Err` naming a missing def;
/// `Ok(None)` when `t` is not a record type.
fn record_fields<'a>(
    t: &'a DataType,
    defs: RecordDefs<'a>,
) -> Result<Option<Vec<Step<'a>>>, String> {
    match t {
        DataType::Record(RecordType::Named(n)) => match defs.lookup(n) {
            Some(def) => Ok(Some(
                def.fields
                    .iter()
                    .map(|f| Step {
                        field_id: Some(f.id.0),
                        name: &f.name,
                        data_type: &f.data_type,
                    })
                    .collect(),
            )),
            None => Err(n.clone()),
        },
        DataType::Record(RecordType::Anonymous(fields)) => Ok(Some(
            fields
                .iter()
                .map(|(name, data_type)| Step {
                    field_id: None,
                    name,
                    data_type,
                })
                .collect(),
        )),
        _ => Ok(None),
    }
}

/// The common walk. `pick(depth, fields)` chooses the field of segment
/// `depth`; `visit_def` sees every named def stepped *into* below the schema
/// (including a missing one). Returns the steps taken, or why the walk stopped.
fn walk<'a>(
    schema: &str,
    len: usize,
    defs: RecordDefs<'a>,
    mut pick: impl FnMut(usize, &[Step<'a>]) -> Option<usize>,
    mut visit_def: impl FnMut(&str),
) -> Result<Vec<Step<'a>>, DanglingReason> {
    let schema_def = defs
        .lookup(schema)
        .ok_or_else(|| DanglingReason::DefMissing(schema.to_string()))?;
    let mut fields: Vec<Step<'a>> = schema_def
        .fields
        .iter()
        .map(|f| Step {
            field_id: Some(f.id.0),
            name: &f.name,
            data_type: &f.data_type,
        })
        .collect();
    let mut taken = Vec::with_capacity(len);
    for depth in 0..len {
        let index = pick(depth, &fields).ok_or(DanglingReason::FieldGone { depth })?;
        let step = fields.swap_remove(index);
        if depth + 1 < len {
            if let DataType::Record(RecordType::Named(n)) = step.data_type {
                visit_def(n);
            }
            fields = match record_fields(step.data_type, defs) {
                Ok(Some(next)) => next,
                Ok(None) => return Err(DanglingReason::NotARecord { depth }),
                Err(missing) => return Err(DanglingReason::DefMissing(missing)),
            };
        }
        taken.push(step);
    }
    Ok(taken)
}

/// Resolves `path` against `schema`: the current segment names and the leaf's
/// declared type, or why the path no longer resolves. A segment with a
/// `field_id` is found by id, one without by name. Every segment but the last
/// must land on a `Record` field — not `Optional`, not `Array`. An empty path
/// is `FieldGone { depth: 0 }`.
pub fn resolve_path(
    schema: &str,
    path: &[PathSegment],
    defs: RecordDefs<'_>,
) -> Result<ResolvedPath, DanglingReason> {
    if path.is_empty() {
        return Err(DanglingReason::FieldGone { depth: 0 });
    }
    let steps = walk(
        schema,
        path.len(),
        defs,
        |depth, fields| {
            let seg = &path[depth];
            fields
                .iter()
                .position(|f| match (seg.field_id, f.field_id) {
                    (Some(want), Some(have)) => want == have,
                    _ => f.name == seg.name,
                })
        },
        |_| {},
    )?;
    Ok(ResolvedPath {
        names: steps.iter().map(|s| s.name.to_string()).collect(),
        leaf_type: steps.last().unwrap().data_type.clone(),
    })
}

/// The inverse direction, for the text format and the API, where paths
/// arrive as names: resolves `names` against `schema` and returns the
/// segments with their current `FieldId`s, or the depth of the first name
/// that does not resolve (a missing schema, or an empty path, is depth 0).
pub fn resolve_names(
    schema: &str,
    names: &[String],
    defs: RecordDefs<'_>,
) -> Result<Vec<PathSegment>, usize> {
    if names.is_empty() {
        return Err(0);
    }
    let steps = walk(
        schema,
        names.len(),
        defs,
        |depth, fields| fields.iter().position(|f| f.name == names[depth]),
        |_| {},
    )
    .map_err(|reason| match reason {
        DanglingReason::DefMissing(_) => {
            // The schema (depth 0), or the def under the deepest resolved
            // segment. Find how far the names do resolve.
            resolved_prefix_len(schema, names, defs)
        }
        DanglingReason::FieldGone { depth } => depth,
        DanglingReason::NotARecord { depth } => depth + 1,
    })?;
    Ok(steps
        .iter()
        .map(|s| PathSegment {
            field_id: s.field_id,
            name: s.name.to_string(),
        })
        .collect())
}

/// How many leading `names` resolve, segment by segment.
fn resolved_prefix_len(schema: &str, names: &[String], defs: RecordDefs<'_>) -> usize {
    (1..=names.len())
        .take_while(|&n| resolve_names_exact(schema, &names[..n], defs))
        .count()
}

fn resolve_names_exact(schema: &str, names: &[String], defs: RecordDefs<'_>) -> bool {
    walk(
        schema,
        names.len(),
        defs,
        |depth, fields| fields.iter().position(|f| f.name == names[depth]),
        |_| {},
    )
    .is_ok()
}

/// Like [`resolve_names`], but a def that is **missing** (rather than a name
/// that is not in a def that exists) does not fail: the names from there on
/// are kept as name-only segments, to be resolved by name once the def is
/// back. Used where a path must survive while its defs are unavailable — a
/// frozen node (`doc/design_library_linking.md` §8) rebuilt by a text-format
/// `--replace`. `Err(depth)` as for `resolve_names`.
pub fn resolve_names_allowing_missing_defs(
    schema: &str,
    names: &[String],
    defs: RecordDefs<'_>,
) -> Result<Vec<PathSegment>, usize> {
    if names.is_empty() {
        return Err(0);
    }
    let mut segments = Vec::with_capacity(names.len());
    let mut current = DataType::Record(RecordType::Named(schema.to_string()));
    for (depth, name) in names.iter().enumerate() {
        let fields = match record_fields(&current, defs) {
            Ok(Some(fields)) => fields,
            Ok(None) => return Err(depth),
            Err(_missing) => {
                segments.extend(names[depth..].iter().map(|n| PathSegment {
                    field_id: None,
                    name: n.clone(),
                }));
                return Ok(segments);
            }
        };
        let Some(step) = fields.iter().find(|f| f.name == name.as_str()) else {
            return Err(depth);
        };
        segments.push(PathSegment {
            field_id: step.field_id,
            name: step.name.to_string(),
        });
        current = step.data_type.clone();
    }
    Ok(segments)
}

/// Segments for `names` against `schema` that never fail: each segment gets
/// its field's id as far as the names resolve, and the rest are kept by name
/// only. Used when a schema change moves a row onto a def where its names do
/// not resolve (`doc/design_record_get_set.md`, "Schema change"): the row
/// goes dangling under its current names, and its unresolved tail resolves
/// again only by name.
pub fn resolve_names_partially(
    schema: &str,
    names: &[String],
    defs: RecordDefs<'_>,
) -> Vec<PathSegment> {
    let mut segments = Vec::with_capacity(names.len());
    let mut current = DataType::Record(RecordType::Named(schema.to_string()));
    for name in names {
        let step = match record_fields(&current, defs) {
            Ok(Some(fields)) => fields
                .into_iter()
                .find(|f| f.name == name.as_str())
                .map(|f| (f.field_id, f.data_type.clone())),
            _ => None,
        };
        let Some((field_id, data_type)) = step else {
            break;
        };
        segments.push(PathSegment {
            field_id,
            name: name.clone(),
        });
        current = data_type;
    }
    let resolved = segments.len();
    segments.extend(names[resolved..].iter().map(|n| PathSegment {
        field_id: None,
        name: n.clone(),
    }));
    segments
}

/// The names of the defs below `schema` that `path` walks into, including a
/// missing one (where the walk stops). For `NodeData::record_path_defs`.
pub fn path_defs(schema: &str, path: &[PathSegment], defs: RecordDefs<'_>) -> Vec<String> {
    let mut out = Vec::new();
    let _ = walk(
        schema,
        path.len(),
        defs,
        |depth, fields| {
            let seg = &path[depth];
            fields
                .iter()
                .position(|f| match (seg.field_id, f.field_id) {
                    (Some(want), Some(have)) => want == have,
                    _ => f.name == seg.name,
                })
        },
        |name| out.push(name.to_string()),
    );
    out
}

/// The names to show for `path` when it does not resolve because a def is
/// missing: the current names as far as the path resolves, the stored names
/// from there on.
pub fn display_names(schema: &str, path: &[PathSegment], defs: RecordDefs<'_>) -> Vec<String> {
    let resolved = (1..=path.len())
        .rev()
        .find_map(|n| resolve_path(schema, &path[..n], defs).ok());
    let mut names = resolved.map(|r| r.names).unwrap_or_default();
    names.extend(path[names.len()..].iter().map(|s| s.name.clone()));
    names
}

// ---------------------------------------------------------------------------
// Text syntax
// ---------------------------------------------------------------------------

/// Formats segment names as path text: joined with `.`, a segment containing
/// `.` (or a first segment starting with `!`) in backticks.
pub fn format_path(names: &[String]) -> String {
    names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if name.contains('.') || (i == 0 && name.starts_with('!')) || name.is_empty() {
                format!("`{}`", name)
            } else {
                name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Formats a [`PathSpec`]: a dangling one with a leading `!`.
pub fn format_path_spec(spec: &PathSpec) -> String {
    match spec {
        PathSpec::Live(names) => format_path(names),
        PathSpec::Dangling(names) => format!("!{}", format_path(names)),
    }
}

/// Parses path text into segment names. Errors on empty input, an empty
/// segment, an unclosed backtick, or a backtick inside an unquoted segment.
pub fn parse_path(text: &str) -> Result<Vec<String>, String> {
    if text.is_empty() {
        return Err("empty path".to_string());
    }
    let chars: Vec<char> = text.chars().collect();
    let mut names = Vec::new();
    let mut i = 0;
    loop {
        let mut name = String::new();
        if chars.get(i) == Some(&'`') {
            i += 1;
            loop {
                match chars.get(i) {
                    None => return Err(format!("unclosed backtick in path `{}`", text)),
                    Some('`') => {
                        i += 1;
                        break;
                    }
                    Some(&c) => {
                        name.push(c);
                        i += 1;
                    }
                }
            }
            if !matches!(chars.get(i), None | Some('.')) {
                return Err(format!(
                    "expected `.` after a quoted segment in path `{}`",
                    text
                ));
            }
        } else {
            while let Some(&c) = chars.get(i) {
                if c == '.' {
                    break;
                }
                if c == '`' {
                    return Err(format!("unexpected backtick in path `{}`", text));
                }
                name.push(c);
                i += 1;
            }
        }
        if name.is_empty() {
            return Err(format!("empty segment in path `{}`", text));
        }
        names.push(name);
        match chars.get(i) {
            None => return Ok(names),
            Some('.') => i += 1,
            Some(_) => unreachable!(),
        }
    }
}

/// Parses path text into a [`PathSpec`]: a leading `!` makes it `Dangling`.
pub fn parse_path_spec(text: &str) -> Result<PathSpec, String> {
    match text.strip_prefix('!') {
        Some(rest) => parse_path(rest).map(PathSpec::Dangling),
        None => parse_path(text).map(PathSpec::Live),
    }
}
