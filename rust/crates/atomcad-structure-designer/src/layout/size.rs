//! **The** node-size function.
//!
//! Before `doc/design_incremental_layout.md` four size functions existed and no
//! two agreed: the Sugiyama helper ignored HOF bodies, the inlining helper
//! ignored comments, the creation-time placer knew only a type name, and
//! Flutter's `ScopeResolver` — the only one that is actually rendered — knew
//! both. That is tolerable while sizes only feed heuristics; it is not
//! tolerable once "did this node's footprint grow?" is the event the
//! incremental layout pass is driven by (D13).
//!
//! [`rendered_node_size`] is the single answer, and it mirrors Flutter's
//! `ScopeResolver.effectiveNodeSizeLogical` / `_computeBodySize`
//! (`lib/structure_designer/node_network/scope_resolver.dart`) rule for rule:
//!
//! | Node | Footprint |
//! |---|---|
//! | comment | its real `CommentData` width × height |
//! | expanded HOF / closure | gutters + [`rendered_body_size`] (recursive) |
//! | collapsed HOF, everything else | title + pins + subtitle + padding |
//!
//! `tests/structure_designer/layout_size_parity_test.rs` pins the agreement
//! against a JSON file the Dart test `test/layout_size_parity_test.dart`
//! regenerates, so a Flutter-side size change fails the Rust build until the
//! fixture is refreshed.

use std::collections::HashSet;

use glam::DVec2;

use crate::node_layout;
use crate::node_network::{Node, NodeNetwork, resolve_body_collapsed};
use crate::node_type_registry::NodeTypeRegistry;
use crate::nodes::comment::CommentData;

/// The `CommentData` of `node`, or `None` if it is not a comment node.
///
/// Detection is by data type rather than by node type name, so a renamed or
/// re-registered comment type keeps working.
pub fn comment_data(node: &Node) -> Option<&CommentData> {
    node.data.as_any_ref().downcast_ref::<CommentData>()
}

/// Whether `node` renders a subtitle row, which is worth 20 px of height.
///
/// Flutter asks the view's `subtitle` field, which
/// `structure_designer_api::build_node_view` fills from
/// `NodeData::get_subtitle(connected_input_pins)` — a subtitle usually
/// summarises the *literal* values of pins that have no wire, so it appears and
/// disappears as wires are made. Reproducing that here rather than assuming
/// "always present" (what all three Rust estimates used to do) is what lets the
/// parity fixture be an equality rather than an approximation.
pub fn node_has_subtitle(node: &Node, registry: &NodeTypeRegistry) -> bool {
    let mut connected: HashSet<String> = HashSet::new();
    if let Some(node_type) = registry.get_node_type_for_node(node) {
        for (index, argument) in node.arguments.iter().enumerate() {
            if !argument.is_empty()
                && let Some(parameter) = node_type.parameters.get(index)
            {
                connected.insert(parameter.name.clone());
            }
        }
    }
    node.data
        .get_subtitle(&connected)
        .is_some_and(|subtitle| !subtitle.is_empty())
}

/// The rendered footprint of `node` in logical pixels, top-left anchored.
///
/// This is the authority every layout, reflow and placement path reads; see the
/// module docs for the three cases. `registry` resolves the node's type, custom
/// instances included (`get_node_type_for_node`).
pub fn rendered_node_size(node: &Node, registry: &NodeTypeRegistry) -> DVec2 {
    // A comment carries its own dimensions. Checked first: a comment's node
    // type has no zone and its pin counts say nothing about how big the user
    // dragged it (a 400×300 note against an 83 px estimate).
    if let Some(comment) = comment_data(node) {
        return DVec2::new(comment.width, comment.height);
    }

    let node_type = registry.get_node_type_for_node(node);
    let (num_inputs, num_outputs) = node_type
        .map(|nt| (nt.parameters.len(), nt.output_pin_count()))
        .unwrap_or((0, 1));
    let has_subtitle = node_has_subtitle(node, registry);

    if let Some(nt) = node_type
        && nt.has_zone()
        && !resolve_body_collapsed(node, nt)
    {
        let (body_width, body_height) = rendered_body_size(node, registry);
        return node_layout::estimate_hof_node_size(
            num_inputs,
            num_outputs,
            nt.zone_input_pins.len(),
            nt.zone_output_pins.len(),
            body_width,
            body_height,
            has_subtitle,
            node.node_type_name == "closure",
        );
    }

    node_layout::estimate_node_size(num_inputs, num_outputs, has_subtitle)
}

/// Rendered body-region size of an expanded zone-owning node:
/// `max(stored, content extent + padding)`, measured from the body-local
/// origin and recursing into nested HOFs. Mirrors Flutter's `_computeBodySize`.
///
/// The stored `body_width` / `body_height` are only a **floor** — a freshly
/// built `closure` carries the flat defaults even when its body holds a nested
/// `map` that renders far wider — and layout never writes them back (design
/// doc, "Prerequisite: one size function").
pub fn rendered_body_size(node: &Node, registry: &NodeTypeRegistry) -> (f64, f64) {
    let stored_width = node.body_width;
    let stored_height = node.body_height;
    let Some(body) = node.zone.as_deref() else {
        return (stored_width, stored_height);
    };

    let mut max_right = 0.0_f64;
    let mut max_bottom = 0.0_f64;
    for child in body.nodes.values() {
        let size = rendered_node_size(child, registry);
        max_right = max_right.max(child.position.x + size.x);
        max_bottom = max_bottom.max(child.position.y + size.y);
    }

    let content_width = max_right + node_layout::HOF_BODY_BOTTOM_PADDING;
    let content_height = max_bottom + node_layout::HOF_BODY_BOTTOM_PADDING;
    (
        content_width.max(stored_width),
        content_height.max(stored_height),
    )
}

/// [`rendered_node_size`] for a node addressed by id in `network`.
///
/// A missing id falls back to the plain one-output estimate, which is what the
/// layout helpers did before and keeps a stale id from panicking mid-layout.
pub fn rendered_node_size_by_id(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    node_id: u64,
) -> DVec2 {
    match network.nodes.get(&node_id) {
        Some(node) => rendered_node_size(node, registry),
        None => node_layout::estimate_node_size(0, 1, true),
    }
}
