//! Network serializer for the AI assistant integration.
//!
//! This module provides the `NetworkSerializer` struct which converts a `NodeNetwork`
//! to the human-readable text format suitable for AI assistant consumption.
//!
//! # Example Output
//!
//! ```text
//! sphere1 = sphere { center: (0, 0, 0), radius: 5 }
//! box1 = cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }
//! union1 = union { shapes: [sphere1, box1] }
//! output union1
//! ```
//!
//! # Zone bodies
//!
//! A zone-owning node (`map`, `filter`, `fold`, `foreach`, `zip_with`,
//! `closure`) carries an inline body — a whole `NodeNetwork` hanging off
//! `Node.zone`. Those bodies are projected as a `body { … }` block nested in
//! the owner's property block, which makes the node's statement span multiple
//! lines (`doc/design_hof_body_text_format.md` D14):
//!
//! ```text
//! m1 = map {
//!   xs: r,
//!   body {
//!     d = mul { a: $element, b: ^scale }
//!     output d
//!   }
//! }
//! ```
//!
//! A body communicates with the outside in exactly four ways, and each has one
//! spelling (D3/D4/D5). With `k` = the number of leading `^`:
//!
//! | Written | Resolves to | Encoding |
//! |---|---|---|
//! | `n` | a node in this body | `NodeOutput`, depth 0 |
//! | `^n` | a node one scope out | `NodeOutput`, depth 1 |
//! | `$element` | this body's own HOF's iteration value | `ZoneInput`, depth 1 |
//! | `^$element` | the enclosing HOF's iteration value | `ZoneInput`, depth 2 |
//!
//! i.e. **`k` carets → `NodeOutput` at depth `k`, `$`-prefixed → `ZoneInput`
//! at depth `k + 1`.** The `+ 1` is the base asymmetry between the two source
//! kinds: a `NodeOutput` depth addresses a *network* (`0` = this body), while
//! a `ZoneInput` depth addresses an *owning HOF's body frame* (`1` = this
//! body's own owner). See the `ZoneInput` arm of `network_evaluator.rs` for
//! the runtime side of the same arithmetic.
//!
//! The `output` statement inside a block feeds the **parent HOF node's**
//! `zone_output_arguments`, not the body network's `return_node_id` (D5).

use super::network_editor::{PIN_ROLES_PROPERTY, unique_node_names};
use super::parser::Parser;
use super::serializer::format_string;
use crate::node_network::{
    ArgumentKind, FunctionPinRole, IncomingWire, Node, NodeDisplayState, NodeDisplayType,
    NodeNetwork, SourcePin,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::nodes::comment::{ANCHOR_PROPERTY, CommentAnchor, CommentData};
use std::borrow::Cow;
use std::collections::HashSet;

/// One nesting level of indentation inside a `body { … }` block.
const INDENT: &str = "  ";

/// Format a name for the text format: emit it bare if it lexes as a single
/// bare identifier, otherwise wrap it in backticks. The name is assumed to
/// already be valid (no embedded backticks); this is enforced by
/// `is_valid_user_name` at every entry point.
pub fn format_identifier(name: &str) -> Cow<'_, str> {
    if Parser::needs_quoting(name) {
        Cow::Owned(format!("`{}`", name))
    } else {
        Cow::Borrowed(name)
    }
}

/// One frame of the scope chain the serializer is currently inside.
///
/// The first frame is always the root network (`owner: None`); every further
/// frame is one zone body, and its `owner` is the zone-owning node — living in
/// the *previous* frame's network — whose `zone` this network is. `^` walks
/// index by network and `$` walks index by owning HOF, so one structure has to
/// carry both halves.
#[derive(Clone, Copy)]
struct ScopeFrame<'a> {
    network: &'a NodeNetwork,
    owner: Option<&'a Node>,
}

/// Serializes a node network to text format.
pub struct NetworkSerializer<'a> {
    network: &'a NodeNetwork,
    registry: &'a NodeTypeRegistry,
    network_name: Option<&'a str>,
}

impl<'a> NetworkSerializer<'a> {
    /// Create a new serializer for the given network.
    pub fn new(
        network: &'a NodeNetwork,
        registry: &'a NodeTypeRegistry,
        network_name: Option<&'a str>,
    ) -> Self {
        Self {
            network,
            registry,
            network_name,
        }
    }

    /// Serialize the network to text format.
    pub fn serialize(&self) -> String {
        let mut output = String::new();

        // Add header with network name (if provided)
        if let Some(name) = self.network_name {
            output.push_str(&format!("# Network: {}\n", name));
        }

        // Add description (if non-empty)
        let description = &self.network.node_type.description;
        if !description.is_empty() {
            output.push_str(&format!("description {}\n", format_string(description)));
        }

        // Add summary (if set)
        if let Some(ref summary) = self.network.node_type.summary {
            output.push_str(&format!("summary {}\n", format_string(summary)));
        }

        // Add blank line after header/description/summary
        let has_header = self.network_name.is_some()
            || !description.is_empty()
            || self.network.node_type.summary.is_some();
        if has_header {
            output.push('\n');
        }

        // Handle empty network
        if self.network.nodes.is_empty() {
            output.push_str("# Empty network\n");
            return output;
        }

        // Serialize the root scope: every node statement, then the network's
        // own `output`. A cycle aborts before the footer, as it always has.
        let stack = vec![ScopeFrame {
            network: self.network,
            owner: None,
        }];
        if !self.serialize_scope(&stack, 0, &mut output) {
            return output;
        }

        // Add footer with node count. Body nodes are part of the listing now,
        // so they are part of the count.
        let node_count = Self::count_nodes(self.network);
        let node_word = if node_count == 1 { "node" } else { "nodes" };
        output.push_str(&format!("\n# {} {}\n", node_count, node_word));

        output
    }

    /// Total number of nodes in `network`, including every node nested in a
    /// zone body at any depth.
    fn count_nodes(network: &NodeNetwork) -> usize {
        network
            .nodes
            .values()
            .map(|node| {
                1 + node
                    .zone
                    .as_ref()
                    .map_or(0, |body| Self::count_nodes(body.as_ref()))
            })
            .sum()
    }

    /// Emit every statement of the scope on top of `stack`, indented by
    /// `indent` levels: the node statements in topological order, then the
    /// scope's `output` statement.
    ///
    /// `output` means two different things depending on the frame, and its
    /// position in the text disambiguates them exactly as the design says: at
    /// the root it names the network's `return_node_id`; inside a body it
    /// names the source of the **owning node's** `zone_output_arguments`.
    ///
    /// Returns `false` if the scope could not be ordered (a wire cycle), in
    /// which case a `# Error:` comment has been written in its place.
    fn serialize_scope(&self, stack: &[ScopeFrame<'a>], indent: usize, out: &mut String) -> bool {
        let frame = match stack.last() {
            Some(frame) => frame,
            None => return false,
        };
        let pad = INDENT.repeat(indent);

        let sorted_ids = match Self::topological_sort(frame.network) {
            Ok(ids) => ids,
            Err(cycle_error) => {
                out.push_str(&format!("{}# Error: {}\n", pad, cycle_error));
                return false;
            }
        };

        for node_id in &sorted_ids {
            out.push_str(&self.serialize_node(stack, *node_id, indent));
            out.push('\n');
        }

        match frame.owner {
            None => {
                if let Some(return_node_id) = frame.network.return_node_id
                    && let Some(return_name) = Self::get_node_name(frame.network, return_node_id)
                {
                    out.push_str(&format!(
                        "{}output {}\n",
                        pad,
                        format_identifier(&return_name)
                    ));
                }
            }
            Some(owner) => {
                // Every zone type today has exactly one zone-output pin, so
                // the bare `output <node>` form suffices (D5). Should a type
                // ever grow a second, this loop already emits one line per
                // wired pin and the format gains a pin-qualified form.
                for argument in &owner.zone_output_arguments {
                    if let Some(wire) = argument.incoming_wires.first()
                        && let Some(source) = self.format_wire_source(stack, wire)
                    {
                        out.push_str(&format!("{}output {}\n", pad, source));
                    }
                }
            }
        }

        true
    }

    /// Perform topological sort of a network's nodes (dependencies before
    /// dependents). Returns an error if a cycle is detected.
    fn topological_sort(network: &NodeNetwork) -> Result<Vec<u64>, String> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_mark = HashSet::new();

        // Get all node IDs sorted for deterministic output
        let mut node_ids: Vec<u64> = network.nodes.keys().copied().collect();
        node_ids.sort();

        // Visit all nodes in sorted order
        for node_id in node_ids {
            if !visited.contains(&node_id) {
                Self::dfs_visit(network, node_id, &mut result, &mut visited, &mut temp_mark)?;
            }
        }

        Ok(result)
    }

    /// Depth-first search visit for topological sort.
    fn dfs_visit(
        network: &NodeNetwork,
        node_id: u64,
        result: &mut Vec<u64>,
        visited: &mut HashSet<u64>,
        temp_mark: &mut HashSet<u64>,
    ) -> Result<(), String> {
        // Cycle detection
        if temp_mark.contains(&node_id) {
            let node = network.nodes.get(&node_id);
            let node_type = node.map(|n| n.node_type_name.as_str()).unwrap_or("unknown");
            return Err(format!(
                "Cycle detected at node {} (type: {})",
                node_id, node_type
            ));
        }

        // Already fully visited
        if visited.contains(&node_id) {
            return Ok(());
        }

        // Mark temporarily (for cycle detection)
        temp_mark.insert(node_id);

        // Visit dependencies first (nodes that this node depends on). Only
        // *local* wires order this network: a capture or zone-input wire names
        // something in an enclosing scope, which is already ordered.
        if let Some(node) = network.nodes.get(&node_id) {
            for argument in &node.arguments {
                // Sort dependency node IDs for deterministic output
                let mut dep_ids: Vec<u64> = argument
                    .incoming_wires
                    .iter()
                    .filter(|wire| Self::is_local_node_output(wire))
                    .map(|wire| wire.source_node_id)
                    .collect();
                dep_ids.sort();
                for source_node_id in dep_ids {
                    Self::dfs_visit(network, source_node_id, result, visited, temp_mark)?;
                }
            }
        }

        // Remove temp mark, add permanent mark
        temp_mark.remove(&node_id);
        visited.insert(node_id);

        // Add to result (post-order)
        result.push(node_id);

        Ok(())
    }

    /// True for a wire whose source is a node in the *same* network as its
    /// destination — the only kind that constrains this network's ordering.
    fn is_local_node_output(wire: &IncomingWire) -> bool {
        wire.source_scope_depth == 0 && matches!(wire.source_pin, SourcePin::NodeOutput { .. })
    }

    /// Get the name for a node in `network` from its custom_name field.
    ///
    /// Since all nodes now have persistent names assigned at creation,
    /// this is a simple lookup of the custom_name field.
    /// The name a node is written under: its `custom_name`, made unique per
    /// scope by the one rule the whole text layer shares
    /// (`network_editor::unique_node_names`).
    fn get_node_name(network: &NodeNetwork, node_id: u64) -> Option<String> {
        unique_node_names(network).remove(&node_id)
    }

    /// Serialize a single node to text format, indented by `indent` levels.
    ///
    /// The result is a single line for a node with no body, and a multi-line
    /// block (D14) for a zone-owning node that has one. Every line carries its
    /// own indentation; there is no trailing newline.
    fn serialize_node(&self, stack: &[ScopeFrame<'a>], node_id: u64, indent: usize) -> String {
        let pad = INDENT.repeat(indent);
        let inner_pad = INDENT.repeat(indent + 1);
        let network = match stack.last() {
            Some(frame) => frame.network,
            None => return format!("{}# Error: no enclosing scope", pad),
        };
        let node = match network.nodes.get(&node_id) {
            Some(n) => n,
            None => return format!("{}# Error: node {} not found", pad, node_id),
        };

        let node_name = match Self::get_node_name(network, node_id) {
            Some(name) => name,
            None => return format!("{}# Error: no name for node {}", pad, node_id),
        };

        // Get the node type to access parameter information
        let node_type = self.registry.get_node_type_for_node(node);

        // Collect all properties (stored values + connections)
        let mut properties: Vec<(String, String)> = Vec::new();

        // Track which parameters have connections
        let mut connected_params: HashSet<String> = HashSet::new();

        // First pass: gather connections
        if let Some(nt) = node_type {
            for (arg_index, argument) in node.arguments.iter().enumerate() {
                if !argument.is_empty() && arg_index < nt.parameters.len() {
                    let param_name = &nt.parameters[arg_index].name;
                    connected_params.insert(param_name.clone());

                    // Check if this is a multi-input parameter
                    let is_multi = argument.len() > 1;

                    // Stored order, never sorted: on an array pin the order is
                    // the value (`atom_union`, `array_concat` concatenate in
                    // it), and a sort by source id — the old "determinism" —
                    // reshuffled it on every `--replace`, which mints fresh
                    // ids in statement order.
                    let wires: Vec<&IncomingWire> = argument.incoming_wires.iter().collect();

                    if is_multi {
                        // Multi-input: format as array of references
                        let refs: Vec<String> = wires
                            .iter()
                            .filter_map(|wire| self.format_wire_source(stack, wire))
                            .collect();
                        properties.push((param_name.clone(), format!("[{}]", refs.join(", "))));
                    } else {
                        // Single input: format as direct reference
                        if let Some(source) = wires
                            .first()
                            .and_then(|wire| self.format_wire_source(stack, wire))
                        {
                            properties.push((param_name.clone(), source));
                        }
                    }
                }
            }
        }

        // Second pass: add stored properties (skip those with connections)
        let text_props = node.data.get_text_properties();
        for (prop_name, prop_value) in text_props {
            // Only include properties that don't have connections
            if !connected_params.contains(&prop_name) {
                properties.push((prop_name, prop_value.to_text()));
            }
        }

        // Third pass: add visibility property (only if visible, since invisible
        // is the default). `displayed_nodes` is per-`NodeNetwork`, so this is
        // per-scope for free. The default state — Normal, pin 0 — is still
        // written as `true`, so a network that never touched pin display or
        // ghosting prints exactly as before.
        if let Some(state) = network.displayed_nodes.get(&node_id) {
            properties.push((
                "visible".to_string(),
                self.format_display_state(network, node_id, state),
            ));
        }

        // Fourth pass: comment anchors. Like `visible`, these are not a
        // `NodeData` text property — they are node *ids*, and only the
        // network can turn those back into names — so they are emitted here.
        // An anchor that no longer resolves is simply not written; repair
        // drops such anchors anyway (D6), so the two agree.
        if let Some(comment) = node.data.as_any_ref().downcast_ref::<CommentData>() {
            let refs: Vec<String> = comment
                .anchors
                .iter()
                .filter_map(|anchor| self.format_anchor(stack, anchor))
                .collect();
            match refs.len() {
                0 => {}
                1 => properties.push((ANCHOR_PROPERTY.to_string(), refs[0].clone())),
                _ => properties.push((
                    ANCHOR_PROPERTY.to_string(),
                    format!("[{}]", refs.join(", ")),
                )),
            }
        }

        // Fifth pass: function pin roles. `Node` state keyed by pin index;
        // written by pin name, in pin order, and only when some pin is
        // overridden — absence is `Auto`, the canonical form, so a network
        // without overrides prints exactly as before.
        if !node.function_pin_roles.is_empty()
            && let Some(node_type) = self.registry.get_node_type_for_node(node)
        {
            let entries: Vec<String> = node
                .function_pin_roles
                .iter()
                .filter_map(|(&index, role)| {
                    let spelling = match role {
                        FunctionPinRole::Auto => return None,
                        FunctionPinRole::Delayed => "delayed",
                        FunctionPinRole::Supplied => "supplied",
                    };
                    let name = &node_type.parameters.get(index)?.name;
                    Some(format!("{}: {}", format_identifier(name), spelling))
                })
                .collect();
            if !entries.is_empty() {
                properties.push((
                    PIN_ROLES_PROPERTY.to_string(),
                    format!("{{ {} }}", entries.join(", ")),
                ));
            }
        }

        // Sixth pass: the zone body, when this node owns one worth writing.
        let body_block = self.serialize_body(stack, node, indent + 1);

        // Format the node. The LHS name and the RHS node type are both
        // identifier positions and must be quoted if they contain reserved
        // characters (relevant for custom networks with relaxed names used as
        // node types).
        let lhs = format_identifier(&node_name);
        let rhs = format_identifier(&node.node_type_name);
        let props_str: Vec<String> = properties
            .iter()
            // A property key is an identifier position too: a custom node's
            // parameter can be named after a dotted network.
            .map(|(k, v)| format!("{}: {}", format_identifier(k), v))
            .collect();

        match body_block {
            // A body forces the multi-line form: one property per line, then
            // the `body { … }` block last, comma-separated from the properties
            // exactly as another property would be.
            Some(body) => {
                let mut out = format!("{}{} = {} {{\n", pad, lhs, rhs);
                for prop in &props_str {
                    out.push_str(&format!("{}{},\n", inner_pad, prop));
                }
                out.push_str(&body);
                out.push_str(&pad);
                out.push('}');
                out
            }
            None if props_str.is_empty() => format!("{}{} = {}", pad, lhs, rhs),
            None => format!("{}{} = {} {{ {} }}", pad, lhs, rhs, props_str.join(", ")),
        }
    }

    /// Serialize `node`'s zone body as a `body { … }` block indented by
    /// `indent` levels, or `None` when there is nothing to write.
    ///
    /// A zone-owning node whose body is empty *and* whose zone-output pin is
    /// unwired emits no block at all: an omitted `body` leaves the body
    /// untouched (D6), which for an already-empty body is the same state, so
    /// the round-trip stays exact without the noise. That is the normal shape
    /// of an HOF driven through its `f:` function pin.
    ///
    /// The returned string ends with a newline, so the caller can append the
    /// owner's closing brace directly.
    fn serialize_body(
        &self,
        stack: &[ScopeFrame<'a>],
        node: &'a Node,
        indent: usize,
    ) -> Option<String> {
        let body = node.zone.as_ref()?.as_ref();
        let has_output = node
            .zone_output_arguments
            .iter()
            .any(|argument| !argument.is_empty());
        if body.nodes.is_empty() && !has_output {
            return None;
        }

        let pad = INDENT.repeat(indent);
        let mut child_stack: Vec<ScopeFrame<'a>> = stack.to_vec();
        child_stack.push(ScopeFrame {
            network: body,
            owner: Some(node),
        });

        let mut inner = String::new();
        self.serialize_scope(&child_stack, indent + 1, &mut inner);

        Some(format!("{}body {{\n{}{}}}\n", pad, inner, pad))
    }

    /// Format one comment anchor as a text-format reference, or `None` when
    /// it cannot be written: a dangling anchor, a node without a name, or a
    /// wire the text format has no projection for.
    fn format_anchor(&self, stack: &[ScopeFrame<'a>], anchor: &CommentAnchor) -> Option<String> {
        let network = stack.last()?.network;
        match anchor {
            CommentAnchor::Node(node_id) => {
                Some(format_identifier(&Self::get_node_name(network, *node_id)?).into_owned())
            }
            CommentAnchor::Wire(wire_anchor) => {
                let wire = wire_anchor.resolve(network)?;
                // A `ZoneOutput` wire terminates at an HOF body return. Its
                // destination is the owner's inside-facing zone-output pin,
                // which the body's own `output` statement spells — there is no
                // `source -> dest.param` form for it.
                if wire.destination_argument_kind != ArgumentKind::External {
                    return None;
                }
                let source = self.format_wire_source(
                    stack,
                    &IncomingWire {
                        source_node_id: wire.source_node_id,
                        source_pin: wire.source_pin,
                        source_scope_depth: wire.source_scope_depth,
                    },
                )?;
                let dest_name = Self::get_node_name(network, wire.destination_node_id)?;
                let dest_node = network.nodes.get(&wire.destination_node_id)?;
                let param_name = self
                    .registry
                    .get_node_type_for_node(dest_node)?
                    .parameters
                    .get(wire.destination_argument_index)?
                    .name
                    .clone();
                Some(format!(
                    "{} -> {}.{}",
                    source,
                    format_identifier(&dest_name),
                    format_identifier(&param_name)
                ))
            }
        }
    }

    /// Format the source side of one incoming wire, relative to the scope on
    /// top of `stack`.
    ///
    /// This is the single place the reference table in the module docs is
    /// implemented: `k` carets for a `NodeOutput` at depth `k`, and `k` carets
    /// plus `$` for a `ZoneInput` at depth `k + 1`. The function-pin (`@name`)
    /// and multi-output (`name.pin`) qualifiers compose with the carets, which
    /// always come first — `^` walks the scope, and everything after it names
    /// something in the scope it landed on.
    fn format_wire_source(&self, stack: &[ScopeFrame<'a>], wire: &IncomingWire) -> Option<String> {
        let depth = wire.source_scope_depth as usize;
        match wire.source_pin {
            SourcePin::NodeOutput { pin_index } => {
                // `depth` frames up from the current one; `0` is this network.
                let source_network = stack.get(stack.len().checked_sub(depth + 1)?)?.network;
                let source_name = Self::get_node_name(source_network, wire.source_node_id)?;
                let carets = "^".repeat(depth);
                let formatted_name = format_identifier(&source_name);
                Some(if pin_index == -1 {
                    // Function pin reference
                    format!("{}@{}", carets, formatted_name)
                } else if pin_index > 0 {
                    // Multi-output pin: look up the pin name from the node
                    // type. A `record_destructure` pin is a record field, so
                    // it is not necessarily a bare identifier (`z-shift`).
                    match self.get_output_pin_name(source_network, wire.source_node_id, pin_index) {
                        Some(pin_name) => format!(
                            "{}{}.{}",
                            carets,
                            formatted_name,
                            format_identifier(&pin_name)
                        ),
                        // Fallback: use numeric index if pin name unavailable
                        None => format!("{}{}.pin{}", carets, formatted_name, pin_index),
                    }
                } else {
                    // Pin 0: regular output reference (no qualifier for
                    // backward compat)
                    format!("{}{}", carets, formatted_name)
                })
            }
            SourcePin::ZoneInput { pin_index } => {
                // A `ZoneInput` depth addresses an owning HOF's *body frame*,
                // which sits at `stack_len - depth`. Depth 0 is not a legal
                // encoding for this variant.
                if depth == 0 {
                    return None;
                }
                let owner = stack.get(stack.len().checked_sub(depth)?)?.owner?;
                debug_assert_eq!(
                    owner.id, wire.source_node_id,
                    "ZoneInput wire at depth {} should name the owning HOF node",
                    depth
                );
                // Zone-input pin names are not uniform across node types
                // (`element` / `acc` / `element1` / a closure's own parameter
                // names), so they must be read off the resolved node type
                // rather than assumed.
                let pin_name = self
                    .registry
                    .get_node_type_for_node(owner)
                    .and_then(|nt| nt.zone_input_pins.get(pin_index))
                    .map(|pin| pin.name.clone());
                let carets = "^".repeat(depth - 1);
                Some(match pin_name {
                    Some(name) => format!("{}${}", carets, format_identifier(&name)),
                    None => format!("{}$pin{}", carets, pin_index),
                })
            }
        }
    }

    /// Spell a node's display state as the value of `visible:`.
    ///
    /// `true` for Normal + pin 0, `ghost` for Ghost + pin 0, a list of output
    /// pin names (in pin order, the same names the `.pin` wire syntax uses)
    /// when the displayed set is anything else, and the object form
    /// `{ pins: [...], ghost: true }` only for a ghosted node with a custom
    /// pin set. A displayed pin index the node type has no name for cannot be
    /// spelled and is left out.
    fn format_display_state(
        &self,
        network: &NodeNetwork,
        node_id: u64,
        state: &NodeDisplayState,
    ) -> String {
        let ghost = state.display_type == NodeDisplayType::Ghost;
        let default_pins = state.displayed_pins.len() == 1 && state.displayed_pins.contains(&0);
        if default_pins {
            return if ghost { "ghost" } else { "true" }.to_string();
        }
        let mut indices: Vec<i32> = state.displayed_pins.iter().copied().collect();
        indices.sort_unstable();
        let names: Vec<String> = indices
            .into_iter()
            .filter_map(|index| self.get_output_pin_name(network, node_id, index))
            .map(|name| format_identifier(&name).into_owned())
            .collect();
        let list = format!("[{}]", names.join(", "));
        if ghost {
            format!("{{ pins: {list}, ghost: true }}")
        } else {
            list
        }
    }

    /// Look up the name of an output pin by index on a node in `network`.
    fn get_output_pin_name(
        &self,
        network: &NodeNetwork,
        node_id: u64,
        pin_index: i32,
    ) -> Option<String> {
        let node = network.nodes.get(&node_id)?;
        let node_type = self.registry.get_node_type_for_node(node)?;
        node_type
            .output_pins
            .get(pin_index as usize)
            .map(|p| p.name.clone())
    }
}

/// Serialize a node network to text format.
///
/// This is the main entry point for network serialization.
///
/// # Arguments
/// * `network` - The node network to serialize
/// * `registry` - The node type registry for looking up parameter information
/// * `network_name` - Optional name to include in the header
///
/// # Returns
/// A string containing the text format representation of the network.
///
/// # Example
/// ```rust,ignore
/// let text = serialize_network(&network, &registry, Some("Main"));
/// println!("{}", text);
/// // Output:
/// // # Network: Main
/// //
/// // sphere1 = sphere { center: (0, 0, 0), radius: 5 }
/// // output sphere1
/// //
/// // # 1 node
/// ```
pub fn serialize_network(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    network_name: Option<&str>,
) -> String {
    let serializer = NetworkSerializer::new(network, registry, network_name);
    serializer.serialize()
}
