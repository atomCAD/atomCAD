use super::rename_helpers::apply_rename_core;
use crate::structure_designer::node_type_registry::UserTypeKind;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// One leaf affected by a namespace rename/move, tagged with its kind so
/// undo/redo dispatches the right rewrite (network vs user record def).
#[derive(Debug, Clone)]
pub struct NamespaceRename {
    pub old_name: String,
    pub new_name: String,
    pub kind: UserTypeKind,
}

/// Command for undoing/redoing a namespace rename — a batch rename of every
/// user type (networks AND record defs) under a prefix. Each leaf is renamed
/// via the kind-appropriate infallible primitive: `apply_rename_core` for
/// networks, `rename_record_type_def_unchecked` for records. When any record is
/// touched the registry's record-node pin layouts are repaired (the `Full`
/// refresh does not do this) and the active record def is remapped so the
/// schema-editor selection follows the move across undo/redo.
/// See `doc/design_hierarchical_records.md`.
#[derive(Debug)]
pub struct RenameNamespaceCommand {
    pub renames: Vec<NamespaceRename>,
}

impl RenameNamespaceCommand {
    /// Apply every rename in the given direction (`forward = true` is old→new,
    /// `false` is new→old), remapping the active record def and repairing
    /// record-node pin layouts if any record was touched.
    fn apply(&self, ctx: &mut UndoContext, forward: bool) {
        let mut touched_record = false;
        for r in &self.renames {
            let (from, to) = if forward {
                (&r.old_name, &r.new_name)
            } else {
                (&r.new_name, &r.old_name)
            };
            match r.kind {
                UserTypeKind::Network => {
                    apply_rename_core(ctx.node_type_registry, ctx.active_network_name, from, to);
                }
                UserTypeKind::Record => {
                    // Helper 1 — infallible. The target name was just vacated by
                    // the symmetric rename of this same batch, so no validation
                    // is needed and no `Err` can be silently dropped.
                    ctx.node_type_registry
                        .rename_record_type_def_unchecked(from, to);
                    if ctx.active_record_def_name.as_deref() == Some(from.as_str()) {
                        *ctx.active_record_def_name = Some(to.clone());
                    }
                    touched_record = true;
                }
            }
        }
        // Helper 2 — the `Full` refresh does NOT repair record-node pin layouts.
        if touched_record {
            ctx.node_type_registry.repair_all_networks();
        }
    }
}

impl UndoCommand for RenameNamespaceCommand {
    fn description(&self) -> &str {
        "Rename namespace"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, false);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, true);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
