//! Migration of `.cnnd` save files from serialization version 6 to version 7.
//!
//! Design: see `doc/design_export_atoms_node.md` ("`.cnnd` migration: v6 → v7").
//!
//! In v7 the built-in `export_xyz` node is renamed to `export_atoms` (issue
//! #353 — the node now derives its export format from the file extension, so it
//! is no longer XYZ-specific). The stored node *data* is unchanged
//! (`{ file_name: String }`), so this pass is a pure name rewrite.
//!
//! **Two keys, not one.** A serialized built-in node carries its type name
//! twice: in `node_type_name` and in the polymorphic data tag `data_type`
//! (`node_to_serializable` writes `data_type = node_type_name` for built-ins) —
//! and it is `data_type` that `serializable_to_node` uses to dispatch the
//! node-data loader. Rewriting only `node_type_name` would leave
//! `data_type: "export_xyz"` matching no built-in while the new
//! `node_type_name` *is* one, so the loader's fallback would construct
//! `NoData {}` — silently dropping the stored `file_name` and breaking the
//! property API's `ExportAtomsData` downcast. So the pass rewrites **both**
//! keys.
//!
//! **Whole-tree walk.** Rather than a network→nodes→zone recursion, the pass
//! walks the entire parsed JSON value and rewrites every object entry whose key
//! is `"node_type_name"` OR `"data_type"` and whose string value is
//! `"export_xyz"`. This automatically covers nodes at every zone-body depth.
//! Rewriting `data_type` tree-wide is unambiguous: the key's other use
//! (serialized `DataType`s on parameters/pins) holds enum encodings like
//! `"Float"` / `{"Record": …}`, never a bare node-type name. A network
//! definition's own name lives under `"name"` / the `node_networks` map key,
//! neither of which this pass touches — so a hypothetical (very old) user
//! network literally named `export_xyz` is left alone; it merely becomes
//! un-shadowed once the built-in vacates the name.
//!
//! **Idempotency.** The rewrite is gated on the value still being the old
//! string `"export_xyz"`, which no longer exists after the pass — so a re-run
//! is a no-op. Frozen at the v7 release: the old/new type-name strings and the
//! two rewritten keys are hardcoded, never read from the live registry.

use serde_json::Value;
use std::cell::Cell;

use super::migrate_v2_to_v3::MigrationError;

/// The two object keys that carry a built-in node's type name in the serialized
/// form. Frozen at the v7 release. See the module doc for why both must be
/// rewritten.
const TYPE_NAME_KEYS: [&str; 2] = ["node_type_name", "data_type"];

/// The old built-in node-type name and its v7 replacement. Frozen at the v7
/// release.
const OLD_NODE_TYPE_NAME: &str = "export_xyz";
const NEW_NODE_TYPE_NAME: &str = "export_atoms";

// Test-only instrumentation: counts invocations of `migrate_v6_to_v7` so the
// test suite can verify the version dispatch skips the pre-pass for v7 files.
// Production code never reads this. Mirrors the v2→v3 / v5→v6 counters; see
// `migrate_v2_to_v3` for the thread-locality rationale.
thread_local! {
    static MIGRATION_CALL_COUNT: Cell<u64> = const { Cell::new(0) };
}

/// Returns the number of times [`migrate_v6_to_v7`] has been called on the
/// current thread.
pub fn migration_call_count() -> u64 {
    MIGRATION_CALL_COUNT.with(|c| c.get())
}

/// Resets the current thread's [`migration_call_count`] counter.
pub fn reset_migration_call_count() {
    MIGRATION_CALL_COUNT.with(|c| c.set(0));
}

/// Top-level v6 → v7 pre-pass. Runs on the parsed JSON value before strict
/// deserialization. Walks the whole tree and renames every `export_xyz`
/// type-name reference (in `node_type_name` / `data_type`) to `export_atoms`.
pub fn migrate_v6_to_v7(root: &mut Value) -> Result<(), MigrationError> {
    MIGRATION_CALL_COUNT.with(|c| c.set(c.get() + 1));
    rewrite_type_names(root);
    Ok(())
}

/// Recursively walks `value`, rewriting every object entry whose key is one of
/// [`TYPE_NAME_KEYS`] and whose value is the string [`OLD_NODE_TYPE_NAME`] to
/// [`NEW_NODE_TYPE_NAME`]. Descends into every object value and array element,
/// so nodes at any zone-body depth are covered.
fn rewrite_type_names(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if TYPE_NAME_KEYS.contains(&key.as_str())
                    && child.as_str() == Some(OLD_NODE_TYPE_NAME)
                {
                    *child = Value::from(NEW_NODE_TYPE_NAME);
                } else {
                    rewrite_type_names(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                rewrite_type_names(item);
            }
        }
        _ => {}
    }
}
