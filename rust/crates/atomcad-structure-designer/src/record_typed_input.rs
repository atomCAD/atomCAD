//! Nodes whose pins are derived from the record type of their `record` input:
//! the [`RecordTypedInput`] capability (`doc/design_anonymous_record_readers.md`).
//!
//! A node implements [`RecordTypedInput`] and hands it out through
//! [`NodeData::record_typed_input`](crate::node_data::NodeData::record_typed_input);
//! that is all it takes to be adopted from a wire on drag-place, on connect
//! while it is uncommitted, and by *Take type from wire*. The orchestrators
//! live in `record_input_type_ops.rs` and never name a node type. It is a
//! separate trait from `DeclaredInputs` because a reader has no open port, and
//! `declared_inputs().is_some()` is the open port's gate.

use crate::data_type::RecordType;
use crate::node_type_registry::RecordDefs;

/// A node whose pins are derived from the record type of its `record` input
/// (`record_destructure`; `record_get` and `record_set` in Phase 3 of the
/// design). The type is a snapshot, taken from the wire on drag-place, on
/// connect while the node is uncommitted, and by Take type from wire.
/// Mutating methods edit the data only; the caller re-derives, remaps output
/// wires, reflows, revalidates and records undo (`record_input_type_ops.rs`).
pub trait RecordTypedInput {
    /// `Some(Named(schema))`, `Some(anonymous(fields))`, or `None` when unset.
    fn record_input_type(&self) -> Option<RecordType>;

    /// Adopt `t`: a named type sets `schema` and clears `fields`; an anonymous
    /// one clears `schema` and rebuilds `fields`, keeping ids by name. Path
    /// rows re-resolve by name. `{}` makes the node unset. Errors (nothing
    /// changed) when a field's type is one the node cannot hold.
    fn set_record_input_type(&mut self, t: &RecordType, defs: RecordDefs<'_>)
    -> Result<(), String>;

    /// The `record` pin: index 0 on all three readers.
    fn is_record_input_pin(&self, index: usize) -> bool {
        index == 0
    }
}
