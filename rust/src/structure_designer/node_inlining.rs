//! Inlining a custom-network instance — the structural inverse of
//! *Factor Selection into Subnetwork* (`selection_factoring.rs`).
//!
//! Inlining replaces a single custom-network instance node `I` (whose
//! `node_type_name` resolves to a user network `N`) with a copy of `N`'s
//! contents, spliced into the parent network in place. The named definition is
//! left untouched in the registry.
//!
//! This module holds the pure, registry-free building blocks:
//!
//! - [`make_space_for_inline`] — push the parent's lower-right region outward to
//!   make room for the (generally larger) inlined content.
//! - [`copy_content_into`] — copy `N`'s non-`parameter` nodes into the parent
//!   with fresh ids and shifted positions, touching no wires.
//!
//! The scope-aware wire splice (`splice_inline_boundary`) and the
//! `StructureDesigner` orchestrator land in later phases. See
//! `doc/design_inline_custom_node.md`.

use glam::f64::DVec2;
use std::collections::HashMap;

use super::node_network::{Node, NodeNetwork};

/// Push the lower-right region of `network` outward to make room for inlined
/// content, keeping the instance node's upper-left corner fixed.
///
/// The content that will replace the instance is generally larger than the
/// single node it replaces. We make room with a simple, predictable rule: every
/// node strictly past the instance's right edge shifts right by the extra width
/// the content needs; every node strictly past the instance's bottom edge shifts
/// down by the extra height. A node in the lower-right quadrant (past both edges)
/// shifts on both axes; a node that merely overlaps the instance vertically or
/// horizontally does not move.
///
/// `instance_id` is excluded from the shift (its top-left corner is the anchor).
/// Returns the `delta` actually applied (componentwise `max(0, content - original)`),
/// for tests and for the caller to reason about placement.
pub fn make_space_for_inline(
    network: &mut NodeNetwork,
    instance_id: u64,
    anchor: DVec2,
    original_size: DVec2,
    content_size: DVec2,
) -> DVec2 {
    // Extra space the content needs beyond the original node, never negative.
    let delta = (content_size - original_size).max(DVec2::ZERO);

    let right_edge = anchor.x + original_size.x;
    let bottom_edge = anchor.y + original_size.y;

    for (&id, node) in network.nodes.iter_mut() {
        if id == instance_id {
            continue;
        }
        if node.position.x > right_edge {
            node.position.x += delta.x;
        }
        if node.position.y > bottom_edge {
            node.position.y += delta.y;
        }
    }

    delta
}

/// Copy `source` network `N`'s non-`parameter` node **structure** into `target`:
/// fresh ids allocated from `target.next_node_id`, positions shifted so the
/// content's top-left (`content_min`) lands on `anchor`, and HOF body `Arc`s
/// cloned verbatim.
///
/// **No wires are rewired here.** The copied nodes' `arguments`,
/// `zone_output_arguments`, and all nested body wires still carry `N`'s original
/// ids on exit; `splice_inline_boundary` (a later phase) is the single place that
/// fixes every wire, so id-classification happens exactly once against `N`'s
/// original id space. This avoids a subtle hazard: a freshly allocated `new_id`
/// can numerically collide with one of `N`'s old `parameter` ids, so any
/// "remap, then match parameters by id" scheme could misclassify.
///
/// Copied nodes inherit their display state from `N` (round-trip faithful with
/// factoring) and have `custom_name` collisions against existing `target` nodes
/// de-duplicated by suffix bump.
///
/// Returns `old_id -> new_id` for every copied node (`parameter` nodes absent).
pub fn copy_content_into(
    target: &mut NodeNetwork,
    source: &NodeNetwork,
    anchor: DVec2,
    content_min: DVec2,
) -> HashMap<u64, u64> {
    let mut id_mapping: HashMap<u64, u64> = HashMap::new();

    // Iterate in id order so fresh-id allocation is deterministic regardless of
    // the source HashMap's iteration order.
    let mut source_ids: Vec<u64> = source
        .nodes
        .values()
        .filter(|n| n.node_type_name != "parameter")
        .map(|n| n.id)
        .collect();
    source_ids.sort_unstable();

    for old_id in source_ids {
        let old_node = &source.nodes[&old_id];

        let new_id = target.next_node_id;
        target.next_node_id += 1;
        id_mapping.insert(old_id, new_id);

        let new_position = anchor + (old_node.position - content_min);

        // Preserve the authored name where possible; de-dup only on collision
        // (mirrors `make_names_unique`'s spirit). The scan sees previously
        // inserted copied nodes too, so two copies of the same name diverge.
        let custom_name = old_node
            .custom_name
            .as_deref()
            .map(|name| dedup_name(target, name));

        let new_node = Node {
            id: new_id,
            node_type_name: old_node.node_type_name.clone(),
            custom_name,
            position: new_position,
            arguments: old_node.arguments.clone(), // wires fixed by the splice
            data: old_node.data.clone_box(),
            custom_node_type: old_node.custom_node_type.clone(),
            zone: old_node.zone.clone(), // body Arc verbatim, ids preserved
            zone_output_arguments: old_node.zone_output_arguments.clone(),
            body_width: old_node.body_width,
            body_height: old_node.body_height,
            collapse_mode: old_node.collapse_mode,
        };
        target.nodes.insert(new_id, new_node);

        // Inherit display state from N (full state: type + displayed pins).
        if let Some(state) = source.get_node_display_state(old_id) {
            target.displayed_nodes.insert(new_id, state.clone());
        }
    }

    id_mapping
}

/// Returns `desired` if no existing node in `target` already uses it as a
/// `custom_name`; otherwise appends `_2`, `_3`, … until a free name is found.
fn dedup_name(target: &NodeNetwork, desired: &str) -> String {
    let taken = |candidate: &str| {
        target
            .nodes
            .values()
            .any(|n| n.custom_name.as_deref() == Some(candidate))
    };

    if !taken(desired) {
        return desired.to_string();
    }

    let mut suffix = 2;
    loop {
        let candidate = format!("{}_{}", desired, suffix);
        if !taken(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}
