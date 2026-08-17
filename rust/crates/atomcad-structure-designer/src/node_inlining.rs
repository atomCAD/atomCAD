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
use std::collections::{HashMap, HashSet};

use super::node_layout;
use super::node_network::{
    Argument, IncomingWire, Node, NodeNetwork, SourcePin, resolve_body_collapsed,
};
use super::node_type_registry::NodeTypeRegistry;
use super::nodes::parameter::ParameterData;

/// Push the lower-right region of `network` outward to make room for inlined
/// content, keeping the instance node's upper-left corner fixed.
///
/// The content that will replace the instance is generally larger than the
/// single node it replaces, and it grows **rightward and downward** from the
/// fixed anchor (`r.UL = anchor`, `r.LR = anchor + original_size`). Each other
/// node is classified purely by its **upper-left corner `p`** — no size estimate
/// is needed, so the rule is robust to imperfect node-size estimation:
///
/// - **Past the far corner on both axes** (`p.x > r.LR.x && p.y > r.LR.y`): the
///   growth reaches it diagonally, so shift it on **both** axes (`p += delta`).
///   This preserves its offset from the instance's bottom-right corner.
/// - **In the near-corner quadrant** (`p.x >= r.UL.x && p.y >= r.UL.y`) but not
///   past the far corner on both axes — i.e. overlapping or edge-adjacent: split
///   by which side of the instance's own diagonal (the line `r.UL → r.LR`) the
///   corner falls on. With bottom = positive `y`, the 2D cross product of the
///   diagonal direction `d = original_size = (W, H)` with `v = p - r.UL` is
///   `cross = W·v.y − H·v.x`. `cross > 0` ⇒ `p` is **below** the diagonal ⇒ more
///   "below" the instance ⇒ shift **down**; otherwise (above or on the diagonal)
///   ⇒ more "to the right" ⇒ shift **right**.
/// - **Above or left of the near corner** (`p.x < r.UL.x` or `p.y < r.UL.y`): the
///   growth never reaches it, so it stays put.
///
/// The near-corner gate is **inclusive** (`>=`) so the common case of a
/// downstream neighbour sharing the instance's top row (`p.y == r.UL.y`, to the
/// right) shifts right, and one sharing its left column (`p.x == r.UL.x`, below)
/// shifts down. Gating on the **near** corner (`r.UL`) rather than the far one
/// also means a node that merely *overlaps* the instance — or sits a few pixels
/// under it — is still moved, fixing the prior far-corner rule that left such a
/// node unmoved. Only the instance itself is unconditionally exempt (via
/// `instance_id`); a hypothetical other node coincident with the anchor falls on
/// the diagonal (`cross == 0`) and shifts right.
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

    let r_ul = anchor;
    let r_lr = anchor + original_size;

    for (&id, node) in network.nodes.iter_mut() {
        if id == instance_id {
            continue;
        }
        let p = node.position; // the node's upper-left corner

        if p.x > r_lr.x && p.y > r_lr.y {
            // Far diagonal region: past both far edges — shift on both axes.
            node.position += delta;
        } else if p.x >= r_ul.x && p.y >= r_ul.y {
            // Overlapping / edge-adjacent: split on the instance's own diagonal.
            // cross = d × (p - r.UL), d = original_size = (W, H), y positive down.
            let cross = original_size.x * (p.y - r_ul.y) - original_size.y * (p.x - r_ul.x);
            if cross > 0.0 {
                // Below the diagonal → shift down.
                node.position.y += delta.y;
            } else {
                // Above (or on) the diagonal → shift right.
                node.position.x += delta.x;
            }
        }
        // Otherwise above-or-left of the near corner: untouched.
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
            function_pin_roles: old_node.function_pin_roles.clone(),
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

/// Estimated (width, height) of `node` within its network, using the node's
/// resolved type (custom-node types resolve through `custom_node_type`). Mirrors
/// the layout heuristic used elsewhere (`auto_layout::get_node_size`): subtitle
/// always assumed present, which is the common case for the node kinds inline
/// operates on.
///
/// An **expanded** HOF / zone-owning node (`map` / `filter` / `fold` /
/// `foreach` / `closure`) is sized by its body region via
/// [`node_layout::estimate_hof_node_size`] — its real footprint is dominated by
/// the body, far larger than the pin-count estimate. Without this, inlining a
/// network that contains an expanded HOF leaves too little room and the copied
/// content overlaps existing parent nodes. A collapsed HOF falls back to the
/// regular size (it renders as a regular-node footprint).
///
/// The body dimensions fed to [`node_layout::estimate_hof_node_size`] come from
/// [`rendered_body_size`], which measures the body's **actual content**
/// (recursing into nested HOFs) rather than trusting the stored
/// `body_width`/`body_height`. The stored values are only a floor — a freshly
/// built `closure` carries the flat `DEFAULT_BODY_*` even when its body holds
/// nested zone nodes (a `map`, another `closure`) that render far wider, so
/// trusting them would undersize the node and leave neighbours overlapping.
fn estimate_node_size_in_network(node: &Node, registry: &NodeTypeRegistry) -> DVec2 {
    let node_type = registry.get_node_type_for_node(node);
    let (n_in, n_out) = node_type
        .map(|nt| (nt.parameters.len(), nt.output_pin_count()))
        .unwrap_or((0, 1));

    if let Some(nt) = node_type
        && nt.has_zone()
        && !resolve_body_collapsed(node, nt)
    {
        let (body_width, body_height) = rendered_body_size(node, registry);
        return node_layout::estimate_hof_node_size(
            n_in,
            n_out,
            nt.zone_input_pins.len(),
            nt.zone_output_pins.len(),
            body_width,
            body_height,
            true,
            node.node_type_name == "closure",
        );
    }

    node_layout::estimate_node_size(n_in, n_out, true)
}

/// Rendered body-region size (logical) of an expanded zone-owning node,
/// mirroring Flutter's `_computeBodySize` (`scope_resolver.dart`):
/// `max(content_extent + padding, stored)`, where each body node's footprint is
/// its **rendered** size — recursing into nested HOFs via
/// [`estimate_node_size_in_network`]. The content extent is measured from the
/// body-local origin (rightmost / bottommost edge), matching Flutter and the
/// fact that [`copy_content_into`] anchors freshly-copied body content at the
/// origin.
///
/// Without the recursion, a body containing a nested expanded HOF is undersized
/// by that inner HOF's entire footprint, so the space-made for the outer node
/// falls short and it overlaps its neighbours — the closure-conversion bug this
/// addresses.
fn rendered_body_size(node: &Node, registry: &NodeTypeRegistry) -> (f64, f64) {
    let stored_width = node.body_width;
    let stored_height = node.body_height;
    let Some(body) = node.zone.as_deref() else {
        return (stored_width, stored_height);
    };

    let mut max_right = 0.0_f64;
    let mut max_bottom = 0.0_f64;
    for child in body.nodes.values() {
        let size = estimate_node_size_in_network(child, registry);
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

/// Estimated size of the custom-node *instance* that is being inlined. Equal to
/// [`estimate_node_size_in_network`] applied to the instance node — exposed
/// separately so the orchestrator reads as the design's `original_size`.
pub fn instance_size(instance: &Node, registry: &NodeTypeRegistry) -> DVec2 {
    estimate_node_size_in_network(instance, registry)
}

/// Bounding box of `source`'s non-`parameter` content as `(content_min, content_size)`:
/// the top-left of the box and its extent, where each node is expanded by its
/// estimated size. Returns `(ZERO, ZERO)` when there is no non-`parameter` node.
pub fn content_bounding_box(source: &NodeNetwork, registry: &NodeTypeRegistry) -> (DVec2, DVec2) {
    let mut min = DVec2::splat(f64::MAX);
    let mut max = DVec2::splat(f64::MIN);
    let mut any = false;
    for node in source.nodes.values() {
        if node.node_type_name == "parameter" {
            continue;
        }
        any = true;
        let size = estimate_node_size_in_network(node, registry);
        min = min.min(node.position);
        max = max.max(node.position + size);
    }
    if !any {
        return (DVec2::ZERO, DVec2::ZERO);
    }
    (min, max - min)
}

// ---------------------------------------------------------------------------
// Scope-aware boundary splice
// ---------------------------------------------------------------------------

/// Classification context for **Descent A** (shared across the recursion into
/// nested bodies). All ids it indexes by are in `N`'s original id space, since
/// the copied content's wires still carry `N`'s ids (see [`copy_content_into`]).
struct DescentA<'a> {
    /// `N`'s `parameter` node id → its `param_index`.
    param_id_to_index: &'a HashMap<u64, usize>,
    /// `param_index` → the wires that replace a reference to that parameter
    /// (the instance's input wires on that pin, or the parameter's default).
    instance_wires: &'a HashMap<usize, Vec<IncomingWire>>,
    /// `N`'s top-level non-`parameter` node id → its new (copied) id.
    id_mapping: &'a HashMap<u64, u64>,
}

impl DescentA<'_> {
    /// Rebuild every argument list's wires at nesting `k`, classifying each wire
    /// whose `source_scope_depth == k` (wires at other depths point at
    /// preserved-id body-internal / intermediate nodes and are kept verbatim).
    fn reclassify(&self, args: &mut [Argument], k: u8) {
        for arg in args.iter_mut() {
            let mut new_wires: Vec<IncomingWire> = Vec::with_capacity(arg.incoming_wires.len());
            for wire in &arg.incoming_wires {
                if wire.source_scope_depth != k {
                    new_wires.push(wire.clone());
                    continue;
                }
                if let Some(&pidx) = self.param_id_to_index.get(&wire.source_node_id) {
                    // Parameter-splice: replace with the instance's wires, each
                    // reached from `k` frames deeper (depth shift by `k`). An
                    // empty `instance_wires(p)` drops the wire.
                    if let Some(iws) = self.instance_wires.get(&pidx) {
                        for iw in iws {
                            new_wires.push(IncomingWire {
                                source_node_id: iw.source_node_id,
                                source_pin: iw.source_pin,
                                source_scope_depth: k + iw.source_scope_depth,
                            });
                        }
                    }
                } else if let Some(&new_id) = self.id_mapping.get(&wire.source_node_id) {
                    // Reference to a co-copied node: follow the id remap, pin and
                    // depth unchanged (the paste-path action).
                    new_wires.push(IncomingWire {
                        source_node_id: new_id,
                        source_pin: wire.source_pin,
                        source_scope_depth: wire.source_scope_depth,
                    });
                }
                // otherwise drop — cannot happen for a valid self-contained N.
            }
            arg.incoming_wires = new_wires;
        }
    }

    /// Recurse into a copied body, processing body-node `arguments` +
    /// `zone_output_arguments` at the body's nesting.
    fn descend_body(&self, body: &mut NodeNetwork, nesting: u8) {
        for node in body.nodes.values_mut() {
            self.reclassify(&mut node.arguments, nesting);
            self.reclassify(&mut node.zone_output_arguments, nesting);
            if let Some(nested) = node.zone_mut() {
                self.descend_body(nested, nesting + 1);
            }
        }
    }
}

/// **Descent B** per-argument-list pass: repoint any wire reading the instance's
/// output pin (`source_node_id == instance_id`, `NodeOutput`, `depth == k`) to
/// the return node, preserving the pin index (multi-output passthrough) and
/// depth. With no return node, such wires are dropped.
fn descent_b_repoint(args: &mut [Argument], k: u8, instance_id: u64, return_id: Option<u64>) {
    for arg in args.iter_mut() {
        let mut new_wires: Vec<IncomingWire> = Vec::with_capacity(arg.incoming_wires.len());
        for wire in &arg.incoming_wires {
            let is_instance_output = wire.source_scope_depth == k
                && wire.source_node_id == instance_id
                && matches!(wire.source_pin, SourcePin::NodeOutput { .. });
            if is_instance_output {
                if let Some(rid) = return_id {
                    new_wires.push(IncomingWire {
                        source_node_id: rid,
                        source_pin: wire.source_pin, // keep consumer's pin index
                        source_scope_depth: wire.source_scope_depth,
                    });
                }
                // else: no return node — drop the consumer wire.
            } else {
                new_wires.push(wire.clone());
            }
        }
        arg.incoming_wires = new_wires;
    }
}

/// Recurse into a (non-copied) body for Descent B.
fn descent_b_body(body: &mut NodeNetwork, nesting: u8, instance_id: u64, return_id: Option<u64>) {
    for node in body.nodes.values_mut() {
        descent_b_repoint(&mut node.arguments, nesting, instance_id, return_id);
        descent_b_repoint(
            &mut node.zone_output_arguments,
            nesting,
            instance_id,
            return_id,
        );
        if let Some(nested) = node.zone_mut() {
            descent_b_body(nested, nesting + 1, instance_id, return_id);
        }
    }
}

/// All wire fix-up for inlining, scope-aware. `target` already contains the
/// copied content (see [`copy_content_into`]); `id_mapping` is its `old → new`
/// id map. `source` is `N`, read for its `parameter` nodes and `return_node_id`.
///
/// Performs **Descent A** (fix the copied content: parameter-splice + copied-node
/// remap, recursing into bodies at the `depth == k` gate) and **Descent B**
/// (repoint the instance's output consumers to `N`'s return node, recursing into
/// sibling bodies), then deletes the instance.
pub fn splice_inline_boundary(
    target: &mut NodeNetwork,
    instance_id: u64,
    source: &NodeNetwork,
    id_mapping: &HashMap<u64, u64>,
) {
    // (1) N's parameter node ids → param_index.
    let mut param_id_to_index: HashMap<u64, usize> = HashMap::new();
    for node in source.nodes.values() {
        if node.node_type_name == "parameter" {
            // `as_ref()` first so the method resolves on `dyn NodeData` (the
            // inner value), not on `Box<dyn NodeData>` itself — the latter
            // downcasts to the Box and silently misses.
            if let Some(pd) = node
                .data
                .as_ref()
                .as_any_ref()
                .downcast_ref::<ParameterData>()
            {
                param_id_to_index.insert(node.id, pd.param_index);
            }
        }
    }

    // (2) instance_wires(p): the instance's incoming wires on input pin p
    //     (verbatim — shape + depth preserved). If pin p is unconnected, fall
    //     back to the parameter node's default wires, remapped through
    //     id_mapping (a default references a node inside N).
    let mut instance_wires: HashMap<usize, Vec<IncomingWire>> = HashMap::new();
    {
        let Some(instance) = target.nodes.get(&instance_id) else {
            return;
        };
        for (&pid, &idx) in &param_id_to_index {
            let connected = instance
                .arguments
                .get(idx)
                .map(|a| a.incoming_wires.clone())
                .unwrap_or_default();
            if !connected.is_empty() {
                instance_wires.insert(idx, connected);
            } else {
                let fallback = source
                    .nodes
                    .get(&pid)
                    .and_then(|p| p.arguments.first())
                    .map(|arg| {
                        arg.incoming_wires
                            .iter()
                            .filter_map(|w| {
                                id_mapping
                                    .get(&w.source_node_id)
                                    .map(|&new_id| IncomingWire {
                                        source_node_id: new_id,
                                        source_pin: w.source_pin,
                                        source_scope_depth: w.source_scope_depth,
                                    })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                instance_wires.insert(idx, fallback);
            }
        }
    }

    // (3) Descent A — fix the copied content. Top-level copied nodes are at
    //     k = 0 (process `arguments` only, matching the paste path); their
    //     bodies recurse at k = 1, 2, … (arguments + zone_output_arguments).
    let ctx = DescentA {
        param_id_to_index: &param_id_to_index,
        instance_wires: &instance_wires,
        id_mapping,
    };
    let copied_ids: Vec<u64> = id_mapping.values().copied().collect();
    for &new_id in &copied_ids {
        if let Some(node) = target.nodes.get_mut(&new_id) {
            ctx.reclassify(&mut node.arguments, 0);
            if let Some(body) = node.zone_mut() {
                ctx.descend_body(body, 1);
            }
        }
    }

    // (4) Descent B — repoint the instance's output consumers to the return
    //     node. Walk the instance scope + all its bodies, skipping the freshly
    //     copied nodes (they come from N and can't reference the instance).
    let return_id = source
        .return_node_id
        .and_then(|rid| id_mapping.get(&rid).copied());
    let copied_set: HashSet<u64> = copied_ids.into_iter().collect();
    for (&id, node) in target.nodes.iter_mut() {
        if copied_set.contains(&id) {
            continue;
        }
        descent_b_repoint(&mut node.arguments, 0, instance_id, return_id);
        if let Some(body) = node.zone_mut() {
            descent_b_body(body, 1, instance_id, return_id);
        }
    }

    // (5) Delete the instance — no wire references it after Descent B.
    target.displayed_nodes.remove(&instance_id);
    target.nodes.remove(&instance_id);
}
