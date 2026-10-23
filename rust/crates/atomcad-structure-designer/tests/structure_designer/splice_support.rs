//! The shared oracles of `doc/design_insert_node_on_wire.md` (Testing), used
//! by every splice operation's test file: *no mutation* for plans and
//! refusals, *round trip* and *invariants* for commits.

use crate::library_links_support::check_invariants;
use crate::undo_test::snapshot_all_networks;
use atomcad_structure_designer::structure_designer::StructureDesigner;

/// What a plan or a refusal must leave exactly as it was: every network's
/// serialized form, the undo stack's depth, and the dirty flag. The
/// serialized form alone would miss a stray undo push.
#[derive(Debug, PartialEq)]
pub struct Untouched {
    networks: serde_json::Value,
    pushes: u64,
    dirty: bool,
}

pub fn untouched(d: &mut StructureDesigner) -> Untouched {
    Untouched {
        networks: snapshot_all_networks(&mut d.node_type_registry),
        pushes: d.undo_stack.push_count(),
        dirty: d.is_dirty(),
    }
}

/// Runs `action` and asserts it changed nothing (*no mutation*).
#[track_caller]
pub fn assert_no_mutation<R>(
    d: &mut StructureDesigner,
    action: impl FnOnce(&mut StructureDesigner) -> R,
) -> R {
    let before = untouched(d);
    let result = action(d);
    assert!(before == untouched(d), "the call changed the design");
    result
}

/// Runs a successful commit and asserts it is exactly one undo step whose
/// undo restores the networks as they were and whose redo restores them as
/// the commit left them (*round trip*), with no fatal invariant violation
/// after the commit, the undo or the redo (*invariants*). Leaves the design
/// in the committed state and returns the commit's result.
#[track_caller]
pub fn assert_one_step_round_trip<R>(
    d: &mut StructureDesigner,
    commit: impl FnOnce(&mut StructureDesigner) -> R,
) -> R {
    let before = snapshot_all_networks(&mut d.node_type_registry);
    let pushes = d.undo_stack.push_count();
    let result = commit(d);
    assert_eq!(pushes + 1, d.undo_stack.push_count(), "one undo step");
    check_invariants(d).unwrap();
    let after = snapshot_all_networks(&mut d.node_type_registry);

    assert!(d.undo());
    let undone = snapshot_all_networks(&mut d.node_type_registry);
    assert!(
        before == undone,
        "undo restores the design; differs at {:?}",
        json_diff(&before, &undone)
    );
    check_invariants(d).unwrap();
    assert!(d.redo());
    let redone = snapshot_all_networks(&mut d.node_type_registry);
    assert!(
        after == redone,
        "redo restores the commit; differs at {:?}",
        json_diff(&after, &redone)
    );
    check_invariants(d).unwrap();
    result
}

/// The paths at which two JSON values differ, with both sides, for a failure
/// message that names the difference instead of printing two whole designs.
pub fn json_diff(a: &serde_json::Value, b: &serde_json::Value) -> Vec<String> {
    fn walk(a: &serde_json::Value, b: &serde_json::Value, path: &str, out: &mut Vec<String>) {
        use serde_json::Value;
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for key in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                    let null = Value::Null;
                    walk(
                        x.get(key).unwrap_or(&null),
                        y.get(key).unwrap_or(&null),
                        &format!("{path}.{key}"),
                        out,
                    );
                }
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (x, y)) in x.iter().zip(y).enumerate() {
                    walk(x, y, &format!("{path}[{i}]"), out);
                }
            }
            _ if a != b => {
                let short = |v: &Value| {
                    let text = v.to_string();
                    text.chars().take(2000).collect::<String>()
                };
                out.push(format!("{path}: {} vs {}", short(a), short(b)));
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(a, b, "", &mut out);
    out
}
