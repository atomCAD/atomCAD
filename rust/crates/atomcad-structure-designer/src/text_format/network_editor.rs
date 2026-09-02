//! Network editor for the AI assistant integration.
//!
//! This module provides the `NetworkEditor` struct which applies text format
//! edit commands to a `NodeNetwork`, enabling AI assistants to modify node
//! networks programmatically.
//!
//! # Supported Operations
//!
//! - **Create nodes:** `name = type { prop: value }`
//! - **Update nodes:** Same syntax as create, with existing name
//! - **Delete nodes:** `delete name`
//! - **Set output:** `output name`
//! - **Wire connections:** `prop: other_node` or `prop: @func_node`
//! - **Zone bodies:** `name = map { xs: r, body { … } }`
//!
//! # Example Input
//!
//! ```text
//! sphere1 = sphere { center: (0, 0, 0), radius: 5 }
//! box1 = cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }
//! union1 = union { shapes: [sphere1, box1] }
//! output union1
//! ```
//!
//! # Scopes
//!
//! Since `doc/design_hof_body_text_format.md` Phase 2 the editor is
//! **scope-aware**: a zone body is a nested `NodeNetwork` and the editor walks
//! into it. Everything that used to be flat is keyed by a **scope path** — the
//! chain of zone-owning node ids from the top-level network down to the body —
//! and every name lookup happens in one named scope, because names are unique
//! *per scope*: `m1/a` and `m2/a` are different nodes and both may exist.
//!
//! The pass structure is dictated by the borrow discipline. `Node.zone` is an
//! `Arc<NodeNetwork>` mutated through `zone_mut()` → `Arc::make_mut`, so a body
//! borrow cannot be held while the parent's name map is read — which is exactly
//! what resolving a `^capture` needs. So no borrow is ever held across a scope
//! boundary: each pass re-walks the scope path from the root, which is
//! `O(depth)` map lookups with a depth of 1 or 2 in practice.
//!
//! 1. **Pass 0** — snapshot every `(name path) → position` over the whole
//!    network including bodies, **before** `clear_network` in replace mode.
//!    That is the only moment the old positions still exist.
//! 2. **Pass 1** — create and update nodes, scope by scope, recursing into
//!    `body { … }` blocks. A statement's properties are applied before its
//!    body block, structurally: the parser keeps the body out of `properties`.
//! 3. **Pass 2** — resolve wires, then comment anchors, then visibility.
//! 4. **Pass 3** — the deferred `delete` / `output` statements, in source
//!    order, scope-qualified.

use serde::Serialize;
use std::collections::{HashMap, HashSet};

use glam::DVec2;

use crate::node_network::{
    Argument, ArgumentKind, CollapseMode, IncomingWire, NodeNetwork, SourcePin, Wire,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::nodes::comment::{ANCHOR_PROPERTY, CommentAnchor, CommentData, WireAnchor};
use crate::nodes::parameter::ParameterData;
use crate::text_format::TextValue;
use crate::text_format::auto_layout;
use crate::text_format::{Parser, PropertyValue, Statement};

/// The chain of zone-owning node ids from the top-level network down to a
/// body. Empty is the top-level network itself.
type ScopePath = Vec<u64>;

/// The same chain spelled by node *name*. Identity across an edit is
/// name-keyed (D8), and a name path is the only form of it that survives
/// `clear_network` — node ids do not.
pub type NamePath = Vec<String>;

/// Everything about one node that the *text format cannot say* and that a
/// human none the less chose (`doc/design_incremental_layout.md` D14).
///
/// Position was the original member (D8 of
/// `doc/design_hof_body_text_format.md`); the rest were added because a
/// `--replace` round-trip clears them too. The text format carries no
/// `body_width`, `body_height` or `collapse_mode`, so replacing an unchanged
/// script used to reset every HOF to the 320x180 default *and* to `Auto`,
/// re-expanding a body the user had deliberately collapsed.
///
/// `footprint` is the node's rendered size at snapshot time. It is not
/// re-applied — a node's size is derived, never stored — but it is the "before"
/// half of the footprint comparison that decides whether a node counts as
/// *grown* (D13), which is the only growth detector the incremental layout pass
/// has.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeLayoutState {
    /// Top-left corner, in the coordinates of the node's own scope.
    pub position: DVec2,
    /// Rendered footprint (`layout::rendered_node_size`) at snapshot time.
    pub footprint: DVec2,
    /// Stored body width; a floor on an HOF's rendered body, never its value.
    pub body_width: f64,
    /// Stored body height. See [`body_width`](Self::body_width).
    pub body_height: f64,
    /// The user's collapse choice.
    pub collapse_mode: CollapseMode,
    /// Whether a human had dragged this node (D5).
    pub hand_moved: bool,
}

/// `(name path) -> layout state` over a whole network, bodies included. The one
/// identity map two designs share — see [`snapshot_node_positions`].
pub type PositionSnapshot = HashMap<NamePath, NodeLayoutState>;

/// Record `(name path) → position` for every node in `network`, including
/// every node nested in a zone body at any depth.
///
/// This is the only form of a node's identity that survives an edit. The text
/// format does not carry positions — the serializer emits none and the editor
/// synthesizes one for every node it creates — so without carrying them
/// across, a whole-body assign (and every `--replace`) would throw the layout
/// away (D8 of `doc/design_hof_body_text_format.md`).
///
/// Keyed by *name path*, never by id and never by a bare name.
/// `clear_network` deletes every node before the first statement is processed
/// and does not reset `next_node_id`, so a `--replace` mints fresh ids and an
/// id-keyed map matches nothing; and a bare name collides across scopes, since
/// names are unique *per scope* only — `m1/d` and `m2/d` are different nodes.
///
/// Shared deliberately: `doc/design_ai_edit_history.md` D9 measures layout
/// against exactly the identity match [`NetworkEditor`] performs, and
/// `layout::diff_scope` diffs against exactly the same map. Three
/// implementations of the same key would drift.
pub fn snapshot_node_positions(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> PositionSnapshot {
    fn walk(
        network: &NodeNetwork,
        registry: &NodeTypeRegistry,
        prefix: &mut NamePath,
        out: &mut PositionSnapshot,
    ) {
        for node in network.nodes.values() {
            let Some(name) = node.custom_name.as_ref() else {
                continue;
            };
            prefix.push(name.clone());
            out.insert(
                prefix.clone(),
                NodeLayoutState {
                    position: node.position,
                    footprint: crate::layout::rendered_node_size(node, registry),
                    body_width: node.body_width,
                    body_height: node.body_height,
                    collapse_mode: node.collapse_mode,
                    hand_moved: node.hand_moved,
                },
            );
            if let Some(body) = node.zone.as_deref() {
                walk(body, registry, prefix, out);
            }
            prefix.pop();
        }
    }
    let mut prefix = Vec::new();
    let mut out = HashMap::new();
    walk(network, registry, &mut prefix, &mut out);
    out
}

/// The separator between the scopes of a node's path, in messages and in
/// `EditResult` (D10). `/`, not `.`, because `.` already means pin access.
const PATH_SEPARATOR: char = '/';

/// Render a node's full path for reporting: `a` at the top level, `m1/a`
/// inside `m1`'s body.
fn path_string(name_path: &[String], name: &str) -> String {
    if name_path.is_empty() {
        name.to_string()
    } else {
        format!("{}{}{}", name_path.join("/"), PATH_SEPARATOR, name)
    }
}

/// Walk down to the network a scope path names, read-only.
fn scope_net<'n>(root: &'n NodeNetwork, scope: &[u64]) -> Option<&'n NodeNetwork> {
    let mut network = root;
    for owner_id in scope {
        network = network.nodes.get(owner_id)?.zone.as_deref()?;
    }
    Some(network)
}

/// Walk down to the network a scope path names, for mutation. Each step goes
/// through `zone_mut()` so the `Arc` copy-on-write stays intact.
fn scope_net_mut<'n>(root: &'n mut NodeNetwork, scope: &[u64]) -> Option<&'n mut NodeNetwork> {
    let mut network = root;
    for owner_id in scope {
        network = network.nodes.get_mut(owner_id)?.zone_mut()?;
    }
    Some(network)
}

/// Result of an edit operation.
///
/// Every node name reported here is a **full path** (`m1/a`), not a bare name
/// (D10): with bodies in play `m1/a` and `m2/a` are different nodes, and a
/// bare name cannot tell the AI — or a name-keyed diff — which one moved.
#[derive(Debug, Clone, Serialize)]
pub struct EditResult {
    /// Whether the edit operation succeeded overall.
    pub success: bool,
    /// Paths of nodes that were created.
    pub nodes_created: Vec<String>,
    /// Paths of nodes that were updated.
    pub nodes_updated: Vec<String>,
    /// Paths of nodes that were deleted.
    pub nodes_deleted: Vec<String>,
    /// Descriptions of wire connections that were made.
    pub connections_made: Vec<String>,
    /// Network description if it was set.
    pub description_set: Option<String>,
    /// Network summary if it was set.
    pub summary_set: Option<String>,
    /// Network output node if it was set.
    pub output_set: Option<String>,
    /// Error messages encountered during editing.
    pub errors: Vec<String>,
    /// Warning messages (non-fatal issues).
    pub warnings: Vec<String>,
}

impl EditResult {
    fn new() -> Self {
        Self {
            success: true,
            nodes_created: Vec::new(),
            nodes_updated: Vec::new(),
            nodes_deleted: Vec::new(),
            connections_made: Vec::new(),
            description_set: None,
            summary_set: None,
            output_set: None,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Record an error. Public so the caller can fold a post-edit
    /// `validate_network` verdict into the same result (D15) — an edit that
    /// parsed and applied but left the network broken is not a success.
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.success = false;
        self.errors.push(error.into());
    }

    /// Record a non-fatal issue. Public for the same reason as
    /// [`EditResult::add_error`].
    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }
}

/// What the scope in front of a reference asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefKind {
    /// A node's regular output pin.
    Node,
    /// A node's `-1` function pin (`@name`).
    Function,
    /// An enclosing HOF's inside-facing zone-input pin (`$name`).
    ZoneInput,
}

/// A source reference extracted from a property value.
#[derive(Debug, Clone)]
struct SourceRef {
    kind: RefKind,
    name: String,
    /// Output pin name for multi-output nodes (e.g., "diff" in `atom_edit.diff`)
    pin_name: Option<String>,
    /// The number of leading `^` as written. For a `ZoneInput` reference the
    /// encoded `source_scope_depth` is this **plus one** — see D4's table.
    depth: usize,
    /// `false` for a bare name, which resolves body-first then outward (D4).
    /// A `^`- or `$`-led reference is exact and never searches.
    explicit: bool,
}

/// Pending connection to be made after all nodes are created.
#[derive(Debug, Clone)]
struct PendingConnection {
    /// The scope the *destination* node lives in. Source scopes are derived
    /// from it by walking out `depth` frames.
    scope: ScopePath,
    dest_node_name: String,
    dest_node_path: String,
    param_name: String,
    source_refs: Vec<SourceRef>,
}

/// One comment anchor target, still in name form.
#[derive(Debug, Clone)]
enum PendingAnchorTarget {
    /// `on: mybox`
    Node(String),
    /// `on: mybox -> union.a`. The source output pin is deliberately not
    /// carried: a wire is identified by its destination slot plus its source
    /// *node*, so a pin qualifier on the way in is decoration. The source
    /// *scope* is carried, because a body wire may be sourced from a capture.
    Wire {
        source: String,
        source_depth: usize,
        dest: String,
        dest_param: String,
    },
}

/// A comment's `on:` anchors, resolved after the connection pass — names only
/// map to ids once every node exists, and a wire anchor additionally needs the
/// wire itself, which that pass is what creates.
#[derive(Debug, Clone)]
struct PendingAnchors {
    scope: ScopePath,
    comment_name: String,
    comment_path: String,
    targets: Vec<PendingAnchorTarget>,
}

/// A statement whose effect has to wait until every node exists and every wire
/// is drawn. Kept in source order so `delete` / `output` interleave exactly as
/// written.
#[derive(Debug, Clone)]
enum DeferredOp {
    /// `delete name`, in the scope it was written in.
    Delete {
        scope: ScopePath,
        name: String,
        path: String,
    },
    /// A network-level `output name` — only ever at the top level; inside a
    /// body the same keyword means [`DeferredOp::ZoneOutput`].
    Output { name: String },
    /// The zone-output wire of a `body { … }` block's owner.
    ///
    /// Pushed for **every** body block, including one with no `output`
    /// statement: a block is total over the parent's `zone_output_arguments`
    /// as well as over the body, so an absent `output` *clears* the wire (D5).
    /// That is the only reading under which `query` → `edit --replace` is
    /// exact.
    ZoneOutput {
        owner_scope: ScopePath,
        owner_id: u64,
        owner_path: String,
        body_scope: ScopePath,
        source_name: Option<String>,
    },
}

/// Names of the nodes in one scope, both directions.
#[derive(Debug, Default)]
struct ScopeNames {
    name_to_id: HashMap<String, u64>,
    id_to_name: HashMap<u64, String>,
}

/// Edits a node network based on text format commands.
pub struct NetworkEditor<'a> {
    network: &'a mut NodeNetwork,
    registry: &'a NodeTypeRegistry,
    /// Per-scope name↔id maps (existing + newly created).
    scopes: HashMap<ScopePath, ScopeNames>,
    /// Counter for placeholder positioning
    new_node_count: usize,
    /// Pending connections to be made in second pass
    pending_connections: Vec<PendingConnection>,
    /// Nodes that should be visible after edit, per scope.
    visible_nodes: Vec<(ScopePath, String)>,
    /// Comment anchors to resolve after the connection pass
    pending_anchors: Vec<PendingAnchors>,
    /// `delete` / `output` statements, in source order.
    deferred: Vec<DeferredOp>,
    /// Pre-edit `(name path) → position`, taken in Pass 0 (D8).
    positions: PositionSnapshot,
    /// Result tracking
    result: EditResult,
}

impl<'a> NetworkEditor<'a> {
    /// Create a new editor for the given network.
    pub fn new(network: &'a mut NodeNetwork, registry: &'a NodeTypeRegistry) -> Self {
        Self {
            network,
            registry,
            scopes: HashMap::new(),
            new_node_count: 0,
            pending_connections: Vec::new(),
            visible_nodes: Vec::new(),
            pending_anchors: Vec::new(),
            deferred: Vec::new(),
            positions: HashMap::new(),
            result: EditResult::new(),
        }
    }

    /// Apply edit commands to the network.
    ///
    /// # Arguments
    /// * `code` - The edit commands in text format
    /// * `replace` - If true, replace entire network; if false, incremental merge
    ///
    /// # Returns
    /// An `EditResult` describing what was changed.
    pub fn apply(mut self, code: &str, replace: bool) -> EditResult {
        // Step 1: Parse input
        let statements = match Parser::parse(code) {
            Ok(stmts) => stmts,
            Err(e) => {
                self.result.add_error(format!("Parse error: {}", e));
                return self.result;
            }
        };

        // Step 2 (Pass 0): the identity snapshot, taken *ahead of the clear* —
        // the one moment the old positions still exist.
        self.snapshot_positions();

        // Step 3: If replace mode, clear the network
        if replace {
            self.clear_network();
        } else {
            // Build name map from existing network
            self.build_existing_name_map(&[]);
        }

        // Step 4 (Pass 1): create/update nodes, collect connections, recurse
        // into `body { … }` blocks.
        self.process_statements(&statements, &[], &[]);

        // Step 5 (Pass 2): wire connections
        self.wire_pending_connections();

        // Step 5b: Comment anchors, which need the wires the previous step made
        self.apply_pending_anchors();

        // Step 6: Process visibility
        self.apply_visibility();

        // Step 7 (Pass 3): deferred delete / output statements, in source order
        self.apply_deferred_ops();

        // Ensure custom node types are updated for any nodes that need them
        // (this walk recurses into bodies).
        self.registry
            .initialize_custom_node_types_for_network(self.network);

        self.result
    }

    // ------------------------------------------------------------------
    // Pass 0: the identity snapshot
    // ------------------------------------------------------------------

    /// Take the pre-edit identity snapshot (D8) via the shared walk, which
    /// `doc/design_ai_edit_history.md` D9 reuses so its layout measurement is
    /// keyed identically to the match performed here.
    fn snapshot_positions(&mut self) {
        self.positions = snapshot_node_positions(self.network, self.registry);
    }

    /// Clear the entire network (for replace mode).
    fn clear_network(&mut self) {
        // Get all node IDs
        let node_ids: Vec<u64> = self.network.nodes.keys().copied().collect();

        // Remove all nodes
        for node_id in node_ids {
            self.network.nodes.remove(&node_id);
        }

        // Clear other state
        self.network.return_node_id = None;
        self.network.displayed_nodes.clear();
        self.network.selected_node_ids.clear();
        self.network.active_node_id = None;
        self.network.selected_wires.clear();

        // Clear name maps
        self.scopes.clear();
    }

    /// Build the name→id mapping for one scope from the nodes it currently
    /// holds. Called for the top level in incremental mode, and for every body
    /// the moment the editor descends into it.
    ///
    /// Since all nodes now have persistent names assigned at creation time,
    /// this simply iterates through nodes and uses their custom_name directly.
    fn build_existing_name_map(&mut self, scope: &[u64]) {
        let mut names = ScopeNames::default();
        if let Some(network) = scope_net(self.network, scope) {
            for (&node_id, node) in &network.nodes {
                if let Some(ref name) = node.custom_name {
                    names.name_to_id.insert(name.clone(), node_id);
                    names.id_to_name.insert(node_id, name.clone());
                }
            }
        }
        self.scopes.insert(scope.to_vec(), names);
    }

    fn lookup(&self, scope: &[u64], name: &str) -> Option<u64> {
        self.scopes
            .get(scope)
            .and_then(|names| names.name_to_id.get(name))
            .copied()
    }

    fn scope_names_mut(&mut self, scope: &[u64]) -> &mut ScopeNames {
        self.scopes.entry(scope.to_vec()).or_default()
    }

    // ------------------------------------------------------------------
    // Pass 1: nodes
    // ------------------------------------------------------------------

    /// Apply a run of statements in one scope.
    ///
    /// `scope` is the chain of zone-owning node ids the statements live in and
    /// `name_path` is the same chain by name — the first addresses the
    /// network, the second is the identity key and the reporting path.
    fn process_statements(
        &mut self,
        statements: &[Statement],
        scope: &[u64],
        name_path: &[String],
    ) {
        for stmt in statements {
            match stmt {
                Statement::Assignment {
                    scope_path,
                    name,
                    node_type,
                    properties,
                    body,
                } => {
                    // A path prefix only moves the statement into another
                    // scope; everything after that is the ordinary path (D7).
                    // An assignment *merges* into the body it addresses — it
                    // is the block form, not the path form, that is total.
                    match self.resolve_path_scope(scope, name_path, scope_path) {
                        Ok((scope, name_path)) => {
                            if let Err(e) = self.process_assignment(
                                &scope,
                                &name_path,
                                name,
                                node_type,
                                properties,
                                body.as_deref(),
                            ) {
                                self.result.add_error(e);
                            }
                        }
                        Err(e) => self.result.add_error(e),
                    }
                }
                Statement::Description { text } => {
                    if scope.is_empty() {
                        self.network.node_type.description = text.to_string();
                        self.result.description_set = Some(text.to_string());
                    } else {
                        self.result.add_warning(
                            "`description` describes a network, not a body; ignored inside `body { }`",
                        );
                    }
                }
                Statement::Summary { text } => {
                    if scope.is_empty() {
                        self.network.node_type.summary = Some(text.to_string());
                        self.result.summary_set = Some(text.to_string());
                    } else {
                        self.result.add_warning(
                            "`summary` describes a network, not a body; ignored inside `body { }`",
                        );
                    }
                }
                Statement::Comment(_) => {}
                Statement::Delete {
                    scope_path,
                    node_name,
                } => match self.resolve_path_scope(scope, name_path, scope_path) {
                    Ok((scope, name_path)) => self.deferred.push(DeferredOp::Delete {
                        scope,
                        name: node_name.clone(),
                        path: path_string(&name_path, node_name),
                    }),
                    Err(e) => self.result.add_error(e),
                },
                Statement::Output {
                    scope_path,
                    node_name,
                } => {
                    // The same keyword means two things, and the position is
                    // what disambiguates them: at the top level it names the
                    // network's return node; inside a body it names the source
                    // of the **owning node's** `zone_output_arguments`, which
                    // `apply_body` records — it has to see the block as a
                    // whole, because an *absent* `output` is meaningful too.
                    //
                    // A path prefix is the second meaning written from
                    // outside: `output m1/x` re-points `m1`'s zone-output wire
                    // and touches nothing else — unlike a block, it is not
                    // total over the body (D7).
                    if !scope_path.is_empty() {
                        match self.resolve_path_scope(scope, name_path, scope_path) {
                            Ok((body_scope, body_name_path)) => {
                                // `resolve_path_scope` walked at least one
                                // segment, so both are non-empty.
                                let (owner_id, owner_scope) = body_scope
                                    .split_last()
                                    .expect("a non-empty path prefix yields a body scope");
                                self.deferred.push(DeferredOp::ZoneOutput {
                                    owner_scope: owner_scope.to_vec(),
                                    owner_id: *owner_id,
                                    owner_path: body_name_path.join(&PATH_SEPARATOR.to_string()),
                                    body_scope: body_scope.clone(),
                                    source_name: Some(node_name.clone()),
                                });
                            }
                            Err(e) => self.result.add_error(e),
                        }
                    } else if scope.is_empty() {
                        self.deferred.push(DeferredOp::Output {
                            name: node_name.clone(),
                        });
                    }
                }
            }
        }
    }

    /// Process an assignment statement (create or update node).
    fn process_assignment(
        &mut self,
        scope: &[u64],
        name_path: &[String],
        name: &str,
        node_type_name: &str,
        properties: &[(String, PropertyValue)],
        body: Option<&[Statement]>,
    ) -> Result<(), String> {
        let path = path_string(name_path, name);

        // Check if node already exists *in this scope*
        let node_id = if let Some(existing_id) = self.lookup(scope, name) {
            // Update existing node
            self.update_node(scope, existing_id, &path, properties)?;
            existing_id
        } else {
            // Create new node
            self.create_node(scope, name_path, name, &path, node_type_name, properties)?
        };

        // Collect pending connections and visibility
        self.collect_connections(scope, name, &path, node_id, properties);

        // The body comes last, always. A closure's `params: ["x", "y"]` has to
        // be in place before `$x` can bind to parameter 0, and the author must
        // not have to remember to write `params:` first (D13) — so the parser
        // holds the body outside `properties` and the editor applies it here.
        if let Some(body_statements) = body {
            self.apply_body(scope, name_path, name, &path, node_id, body_statements)?;
        }

        Ok(())
    }

    /// Create a new node in `scope`.
    fn create_node(
        &mut self,
        scope: &[u64],
        name_path: &[String],
        name: &str,
        path: &str,
        node_type_name: &str,
        properties: &[(String, PropertyValue)],
    ) -> Result<u64, String> {
        // Validate the user-provided name before using it as a custom_name.
        if let Err(reason) = crate::identifier::is_valid_user_name(name) {
            return Err(format!("Invalid node name '{}': {}", path, reason));
        }
        // Look up node type
        let node_type = self
            .registry
            .get_node_type(node_type_name)
            .ok_or_else(|| format!("Unknown node type: '{}'", node_type_name))?;

        // Create node data using the factory
        let node_data = (node_type.node_data_creator)();
        let num_params = node_type.parameters.len();

        // Extract input connections from properties for smart layout positioning
        let input_connections = self.extract_input_connections_for_layout(scope, properties);

        // A node whose (scope, name) existed before the edit inherits its
        // position; only a genuinely new name is laid out (D8).
        let mut identity_key = name_path.to_vec();
        identity_key.push(name.to_string());
        let previous = self.positions.get(&identity_key).copied();
        let position = match previous {
            Some(state) => state.position,
            None => {
                let network = scope_net(self.network, scope)
                    .ok_or_else(|| format!("Scope for '{}' no longer exists", path))?;
                auto_layout::calculate_new_node_position(
                    network,
                    self.registry,
                    node_type_name,
                    &input_connections,
                )
            }
        };
        self.new_node_count += 1;

        // Add node to the scope's network
        let node_id = {
            let network = scope_net_mut(self.network, scope)
                .ok_or_else(|| format!("Scope for '{}' no longer exists", path))?;
            let node_id = network.add_node(node_type_name, position, num_params, node_data);

            // Assign param_id for parameter nodes (for wire preservation across renames)
            if node_type_name == "parameter" {
                let param_id = network.next_param_id;
                network.next_param_id += 1;
                if let Some(node) = network.nodes.get_mut(&node_id)
                    && let Some(param_data) = node.data.as_any_mut().downcast_mut::<ParameterData>()
                {
                    param_data.param_id = Some(param_id);
                }
            }

            // Set node as NOT displayed by default (will be set if visible: true)
            network.set_node_display(node_id, false);

            // Store the user-specified custom name on the node for persistence
            if let Some(node) = network.nodes.get_mut(&node_id) {
                node.custom_name = Some(name.to_string());
            }
            node_id
        };

        // Update name maps
        let names = self.scope_names_mut(scope);
        names.name_to_id.insert(name.to_string(), node_id);
        names.id_to_name.insert(node_id, name.to_string());

        // Apply literal properties
        self.apply_literal_properties(scope, node_id, path, properties)?;

        // Initialize custom node type cache (for expr, parameter nodes, etc.).
        // This is also what initializes a zone-bearing node's body, via
        // `ensure_zone_init` — see `apply_body`.
        if let Some(node) =
            scope_net_mut(self.network, scope).and_then(|network| network.nodes.get_mut(&node_id))
        {
            self.registry.populate_custom_node_type_cache(node, true);
        }

        // Re-apply the rest of the identity snapshot (D14). Position was
        // applied at creation; body size, collapse mode and `hand_moved` are
        // applied *after* `populate_custom_node_type_cache`, whose
        // `ensure_zone_init` is what installs a zone-bearing node's body and
        // its default dimensions. Applied unconditionally: on a node with no
        // zone these three fields are inert, so there is nothing to gate on.
        if let Some(state) = previous
            && let Some(node) = scope_net_mut(self.network, scope)
                .and_then(|network| network.nodes.get_mut(&node_id))
        {
            node.body_width = state.body_width;
            node.body_height = state.body_height;
            node.collapse_mode = state.collapse_mode;
            node.hand_moved = state.hand_moved;
        }

        self.result.nodes_created.push(path.to_string());
        Ok(node_id)
    }

    /// Extract source node IDs from properties for layout positioning.
    ///
    /// Returns a list of (source_node_id, output_pin_index) for each connection
    /// reference found in the properties. Only returns connections to nodes in
    /// the **same scope** — a capture's source node is positioned in a
    /// different coordinate frame, so it says nothing about where this node
    /// should sit.
    fn extract_input_connections_for_layout(
        &self,
        scope: &[u64],
        properties: &[(String, PropertyValue)],
    ) -> Vec<(u64, i32)> {
        let mut connections = Vec::new();

        for (_prop_name, prop_value) in properties {
            self.collect_source_refs_for_layout(scope, prop_value, &mut connections);
        }

        connections
    }

    /// Recursively collect source node references from a property value.
    fn collect_source_refs_for_layout(
        &self,
        scope: &[u64],
        prop_value: &PropertyValue,
        connections: &mut Vec<(u64, i32)>,
    ) {
        match prop_value {
            PropertyValue::NodeRef(name, _pin_name) => {
                if let Some(node_id) = self.lookup(scope, name) {
                    connections.push((node_id, 0)); // Regular output pin (pin index doesn't matter for layout)
                }
            }
            PropertyValue::FunctionRef(name) => {
                if let Some(node_id) = self.lookup(scope, name) {
                    connections.push((node_id, -1)); // Function pin
                }
            }
            PropertyValue::Array(items) => {
                for item in items {
                    self.collect_source_refs_for_layout(scope, item, connections);
                }
            }
            // An anchor is documentation, never a dependency edge (D7), so it
            // must not pull a comment towards its target during layout either.
            // A cross-scope reference names a node in another coordinate
            // frame, which is no guide to where this one goes.
            PropertyValue::WireRef { .. }
            | PropertyValue::Literal(_)
            | PropertyValue::ScopedRef { .. }
            | PropertyValue::ZoneInputRef { .. } => {}
        }
    }

    /// Update an existing node.
    fn update_node(
        &mut self,
        scope: &[u64],
        node_id: u64,
        path: &str,
        properties: &[(String, PropertyValue)],
    ) -> Result<(), String> {
        // Apply literal properties
        self.apply_literal_properties(scope, node_id, path, properties)?;

        // Re-initialize custom node type cache in case properties changed
        if let Some(node) =
            scope_net_mut(self.network, scope).and_then(|network| network.nodes.get_mut(&node_id))
        {
            self.registry.populate_custom_node_type_cache(node, true);
        }

        self.result.nodes_updated.push(path.to_string());
        Ok(())
    }

    // ------------------------------------------------------------------
    // Pass 1: bodies
    // ------------------------------------------------------------------

    /// Make sure a zone-owning node's body network exists, and refuse a node
    /// that owns no body.
    ///
    /// Zone init is **eager** here. `ensure_zone_init` also runs during
    /// validation (`node_type_registry`), but that is long after this editor
    /// returns, and body statements need somewhere to land *now* (D13).
    /// `populate_custom_node_type_cache` already called it during
    /// create/update; this repeats it because it is idempotent and because the
    /// requirement belongs where the body is used.
    fn ensure_zone(&mut self, scope: &[u64], node_id: u64, path: &str) -> Result<(), String> {
        let resolved_type = {
            let network = scope_net(self.network, scope)
                .ok_or_else(|| format!("Scope for '{}' no longer exists", path))?;
            let node = network
                .nodes
                .get(&node_id)
                .ok_or_else(|| format!("Node '{}' not found", path))?;
            self.registry.get_node_type_for_node(node).cloned()
        };
        let Some(resolved_type) = resolved_type else {
            return Err(format!("Node type for '{}' could not be resolved", path));
        };
        if !resolved_type.has_zone() {
            return Err(format!(
                "Node '{}' (type '{}') has no body: only map, filter, fold, foreach, zip_with and closure own one",
                path, resolved_type.name
            ));
        }
        if let Some(node) =
            scope_net_mut(self.network, scope).and_then(|network| network.nodes.get_mut(&node_id))
        {
            node.ensure_zone_init(&resolved_type);
        }
        Ok(())
    }

    /// Walk a statement's path prefix (`m1/`, `outer/inner/`) down from the
    /// scope the statement was written in, and return the scope it addresses
    /// (D7).
    ///
    /// Every segment must already name a zone-owning node — created earlier in
    /// the same script or already present. A path never *creates* the HOF it
    /// addresses: there would be no way to guess its type.
    ///
    /// The prefix only selects a scope. Everything inside the statement —
    /// bare names, `$…`, `^…` — is then relative to the scope it lands on,
    /// which is what makes a path statement and the same statement written
    /// inside a `body { … }` block mean exactly the same thing.
    fn resolve_path_scope(
        &mut self,
        scope: &[u64],
        name_path: &[String],
        prefix: &[String],
    ) -> Result<(ScopePath, NamePath), String> {
        let mut scope = scope.to_vec();
        let mut name_path = name_path.to_vec();
        for segment in prefix {
            let owner_path = path_string(&name_path, segment);
            let node_id = self.lookup(&scope, segment).ok_or_else(|| {
                format!(
                    "Cannot address '{}': no such node — a path may only address a body that already exists",
                    owner_path
                )
            })?;
            self.ensure_zone(&scope, node_id, &owner_path)?;
            scope.push(node_id);
            name_path.push(segment.clone());
            // A body the edit has not descended into yet has no name map.
            if !self.scopes.contains_key(&scope) {
                self.build_existing_name_map(&scope);
            }
        }
        Ok((scope, name_path))
    }

    /// Apply a `body { … }` block to the zone of the node it was written on.
    ///
    /// The block is **total** over the body: names it mentions are created or
    /// updated in place — keeping their node ids and their positions — and
    /// every other body node is removed (D6/D8). That is what makes
    /// `query` → `edit --replace` exact, and what stops a one-node change from
    /// scrambling the other twenty-nine.
    fn apply_body(
        &mut self,
        scope: &[u64],
        name_path: &[String],
        name: &str,
        path: &str,
        node_id: u64,
        body_statements: &[Statement],
    ) -> Result<(), String> {
        self.ensure_zone(scope, node_id, path)?;

        let mut body_scope = scope.to_vec();
        body_scope.push(node_id);
        let mut body_name_path = name_path.to_vec();
        body_name_path.push(name.to_string());

        self.build_existing_name_map(&body_scope);

        // Totality: a name the block does not mention is gone. Everything it
        // does mention is matched by name below and keeps its id and position.
        // A path-addressed statement inside the block mentions the *owner* of
        // the body it reaches into (`m1/x = …` mentions `m1`), which is what
        // keeps the block from deleting the very node the next statement
        // addresses.
        let mentioned: HashSet<&str> = body_statements
            .iter()
            .filter_map(|stmt| match stmt {
                Statement::Assignment {
                    scope_path, name, ..
                } => Some(scope_path.first().unwrap_or(name).as_str()),
                _ => None,
            })
            .collect();
        let mut stale: Vec<String> = self
            .scopes
            .get(&body_scope)
            .map(|names| {
                names
                    .name_to_id
                    .keys()
                    .filter(|existing| !mentioned.contains(existing.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        // Deterministic order so the reported `nodes_deleted` list is stable.
        stale.sort();
        for stale_name in stale {
            let stale_path = path_string(&body_name_path, &stale_name);
            if let Err(e) = self.delete_node(&body_scope, &stale_name, &stale_path) {
                self.result.add_warning(e);
            }
        }

        self.process_statements(body_statements, &body_scope, &body_name_path);

        // The block's own `output`, or its absence — which clears the wire.
        // A path-addressed `output m1/x` written inside the block names
        // *another* node's zone output, not this one's, and so must not count
        // as this block's — nor as its absence.
        let source_name = body_statements.iter().rev().find_map(|stmt| match stmt {
            Statement::Output {
                scope_path,
                node_name,
            } if scope_path.is_empty() => Some(node_name.clone()),
            _ => None,
        });
        self.deferred.push(DeferredOp::ZoneOutput {
            owner_scope: scope.to_vec(),
            owner_id: node_id,
            owner_path: path.to_string(),
            body_scope,
            source_name,
        });

        Ok(())
    }

    // ------------------------------------------------------------------
    // Literal properties
    // ------------------------------------------------------------------

    /// Recursively converts a PropertyValue to a TextValue if all nested values are literals.
    /// Returns None if any nested value is a reference (these are handled in the connection pass).
    fn property_value_to_text_value(pv: &PropertyValue) -> Option<TextValue> {
        match pv {
            PropertyValue::Literal(tv) => Some(tv.clone()),
            PropertyValue::Array(items) => {
                let converted: Option<Vec<TextValue>> = items
                    .iter()
                    .map(Self::property_value_to_text_value)
                    .collect();
                converted.map(TextValue::Array)
            }
            PropertyValue::NodeRef(..)
            | PropertyValue::FunctionRef(_)
            | PropertyValue::WireRef { .. }
            | PropertyValue::ScopedRef { .. }
            | PropertyValue::ZoneInputRef { .. } => None,
        }
    }

    /// Apply literal properties to a node's data.
    fn apply_literal_properties(
        &mut self,
        scope: &[u64],
        node_id: u64,
        path: &str,
        properties: &[(String, PropertyValue)],
    ) -> Result<(), String> {
        // Get valid parameter names for this node type (for validation), plus
        // the subset that is array-typed — those are the multi-input pins,
        // whose only literal form is the empty array.
        let (valid_params, multi_params, node_type_name): (
            Vec<String>,
            std::collections::HashSet<String>,
            String,
        ) = scope_net(self.network, scope)
            .and_then(|network| network.nodes.get(&node_id))
            .and_then(|node| {
                self.registry.get_node_type_for_node(node).map(|node_type| {
                    let params: Vec<String> = node_type
                        .parameters
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();
                    let multi = node_type
                        .parameters
                        .iter()
                        .filter(|p| p.data_type.is_array())
                        .map(|p| p.name.clone())
                        .collect();
                    (params, multi, node.node_type_name.clone())
                })
            })
            .unwrap_or_else(|| (Vec::new(), std::collections::HashSet::new(), String::new()));

        // Get text property names (for literal-only properties that aren't in parameters)
        let text_prop_names: std::collections::HashSet<String> = scope_net(self.network, scope)
            .and_then(|network| network.nodes.get(&node_id))
            .map(|node| {
                node.data
                    .get_text_properties()
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default();

        // Check if this is a custom node (user-defined node network)
        // Custom nodes can accept literal values for ALL their parameters
        let is_custom_node = self.registry.is_custom_node_type(&node_type_name);

        // Collect literal properties into a HashMap
        let mut literal_props: HashMap<String, TextValue> = HashMap::new();

        for (prop_name, prop_value) in properties {
            // Skip special properties. Neither is `NodeData` state: `visible`
            // lives in `NodeNetwork.displayed_nodes` and `on` (comment anchors)
            // is a list of node ids, so both are handled in the
            // connection-collection pass, which can see the whole network.
            if prop_name == "visible" || prop_name == ANCHOR_PROPERTY {
                continue;
            }

            // `shapes: []` on an array pin is an instruction for the connection
            // pass ("no inbound wires"), not a stored value. Array pins have no
            // stored-value backing, so without this it would be reported as an
            // ignored wire-only literal — a warning on the one spelling that
            // does exactly what the user asked. A pin that *does* back its
            // array with a text property keeps the normal path and stores `[]`.
            if multi_params.contains(prop_name)
                && !text_prop_names.contains(prop_name)
                && matches!(prop_value, PropertyValue::Array(items) if items.is_empty())
            {
                continue;
            }

            // Try to convert PropertyValue to TextValue (handles literals and arrays of literals)
            if let Some(text_value) = Self::property_value_to_text_value(prop_value) {
                // Warn about unknown properties (only for values we're actually applying)
                // A property is "known" if it's either a wirable parameter OR a text-only property
                if !valid_params.is_empty()
                    && !valid_params.contains(prop_name)
                    && !text_prop_names.contains(prop_name)
                {
                    self.result.add_warning(format!(
                        "Unknown property '{}' on node type '{}'",
                        prop_name, node_type_name
                    ));
                }
                // Warn if trying to set a literal on a wire-only parameter
                // (a parameter that exists but has no text property backing)
                // BUT: Custom nodes can accept literals for all parameters
                else if !valid_params.is_empty()
                    && valid_params.contains(prop_name)
                    && !text_prop_names.contains(prop_name)
                    && !is_custom_node
                {
                    self.result.add_warning(format!(
                        "Parameter '{}' on '{}' is wire-only; literal value ignored (connect a node instead)",
                        prop_name, node_type_name
                    ));
                    continue; // Don't add to literal_props since it will be ignored anyway
                }
                literal_props.insert(prop_name.clone(), text_value);
            }
            // Skip references and arrays containing them - handled in connection pass
        }

        // Apply to node data
        if !literal_props.is_empty()
            && let Some(node) = scope_net_mut(self.network, scope)
                .and_then(|network| network.nodes.get_mut(&node_id))
        {
            // A `zip_with` lane-list shrink through the text path must also
            // disconnect body wires referencing the dropped tail indices —
            // including nested depth ≥ 2 wires that validation rule 3 only
            // flags red and `repair_zone_body` deliberately skips. The data
            // struct cannot reach the body, so the cleanup runs here, at
            // mutation time (`doc/design_zip_with.md` Phase 3). Text edits
            // are positional, so a tail drop is the only shrink shape.
            let old_zip_lane_count = zip_with_lane_count(node);
            node.data
                .set_text_properties(&literal_props)
                .map_err(|e| format!("Error setting properties on '{}': {}", path, e))?;
            if let (Some(old_count), Some(new_count)) =
                (old_zip_lane_count, zip_with_lane_count(node))
                && new_count < old_count
            {
                crate::nodes::zip_with::disconnect_zip_body_wires_to_dropped_lanes(node, new_count);
            }
        }

        Ok(())
    }

    // ------------------------------------------------------------------
    // Pass 2: wires
    // ------------------------------------------------------------------

    /// Collect pending connections from properties.
    fn collect_connections(
        &mut self,
        scope: &[u64],
        dest_node_name: &str,
        dest_node_path: &str,
        node_id: u64,
        properties: &[(String, PropertyValue)],
    ) {
        for (prop_name, prop_value) in properties {
            // Handle visibility. `displayed_nodes` is per-`NodeNetwork`, so
            // this is per-scope for free.
            if prop_name == "visible" {
                if let PropertyValue::Literal(TextValue::Bool(true)) = prop_value {
                    self.visible_nodes
                        .push((scope.to_vec(), dest_node_name.to_string()));
                }
                continue;
            }

            // Handle comment anchors. Like `visible`, this has to be taken out
            // before the generic reference handling below, which would
            // otherwise try to wire `on` to a parameter that does not exist.
            if prop_name == ANCHOR_PROPERTY {
                self.collect_anchors(scope, dest_node_name, dest_node_path, node_id, prop_value);
                continue;
            }

            // Collect connection references. A property that names no source
            // is still an assignment of that pin — `radius: 5.0` says the pin
            // is a stored value now, not a wire — so it is queued too, with an
            // empty source list, and `wire_connection` clears the pin. Queuing
            // nothing here is what used to let a literal silently coexist with
            // a live wire: the literal landed in the node's data, the wire kept
            // winning at evaluation, and the serializer hid the dead value.
            let source_refs = Self::extract_source_refs(prop_value);
            if !source_refs.is_empty()
                || self.property_disconnects_pin(scope, node_id, prop_name, prop_value)
            {
                self.pending_connections.push(PendingConnection {
                    scope: scope.to_vec(),
                    dest_node_name: dest_node_name.to_string(),
                    dest_node_path: dest_node_path.to_string(),
                    param_name: prop_name.clone(),
                    source_refs,
                });
            }
        }
    }

    /// Whether a source-free property should disconnect the pin it names.
    ///
    /// Only a property that is *entirely* literal counts: a `WireRef` outside
    /// `on:` names no value at all, and an array that still mentions nodes goes
    /// down the ordinary wiring path. Beyond that there are two pins a literal
    /// must **not** clear:
    ///
    /// - a name that is not a wirable parameter (a text-only property such as
    ///   `polygon.vertices`) — there is no pin to clear;
    /// - a wire-only scalar pin, whose literal `apply_literal_properties`
    ///   rejects with a warning — clearing its wire would leave it with
    ///   neither a wire nor a stored value.
    fn property_disconnects_pin(
        &self,
        scope: &[u64],
        node_id: u64,
        prop_name: &str,
        prop_value: &PropertyValue,
    ) -> bool {
        if Self::property_value_to_text_value(prop_value).is_none() {
            return false;
        }
        let Ok((_, is_multi)) = self.get_param_index(scope, node_id, prop_name) else {
            return false;
        };
        if is_multi {
            // An array pin's literal form *is* its wire list, so `[]` — and
            // only `[]` — means "no inbound wires". A non-empty literal array
            // on such a pin is a type error the literal pass already warns
            // about; it must not silently drop the wires as well.
            return matches!(prop_value, PropertyValue::Array(items) if items.is_empty());
        }
        self.pin_accepts_stored_value(scope, node_id, prop_name)
    }

    /// Whether a pin on this node can hold a stored literal value, i.e. the
    /// node's data exposes a text property of that name — or the node is a
    /// custom node type, which accepts literals for all of its parameters.
    fn pin_accepts_stored_value(&self, scope: &[u64], node_id: u64, prop_name: &str) -> bool {
        let Some(node) = scope_net(self.network, scope).and_then(|net| net.nodes.get(&node_id))
        else {
            return false;
        };
        if self.registry.is_custom_node_type(&node.node_type_name) {
            return true;
        }
        node.data
            .get_text_properties()
            .iter()
            .any(|(name, _)| name == prop_name)
    }

    /// Extract source node references from a property value.
    ///
    /// This is where D4's four spellings become wire shapes-to-be. The caret
    /// count is carried verbatim; the `+ 1` that a `ZoneInput` depth needs is
    /// applied in [`NetworkEditor::resolve_source`], next to the arithmetic it
    /// belongs with.
    fn extract_source_refs(prop_value: &PropertyValue) -> Vec<SourceRef> {
        match prop_value {
            PropertyValue::NodeRef(name, pin_name) => vec![SourceRef {
                kind: RefKind::Node,
                name: name.clone(),
                pin_name: pin_name.clone(),
                depth: 0,
                explicit: false,
            }],
            PropertyValue::FunctionRef(name) => vec![SourceRef {
                kind: RefKind::Function,
                name: name.clone(),
                pin_name: None,
                depth: 0,
                explicit: false,
            }],
            PropertyValue::ScopedRef {
                depth,
                name,
                pin_name,
                is_function_ref,
            } => vec![SourceRef {
                kind: if *is_function_ref {
                    RefKind::Function
                } else {
                    RefKind::Node
                },
                name: name.clone(),
                pin_name: pin_name.clone(),
                depth: *depth,
                explicit: true,
            }],
            PropertyValue::ZoneInputRef { depth, name } => vec![SourceRef {
                kind: RefKind::ZoneInput,
                name: name.clone(),
                pin_name: None,
                depth: *depth,
                explicit: true,
            }],
            PropertyValue::Array(items) => {
                items.iter().flat_map(Self::extract_source_refs).collect()
            }
            // A wire reference names an existing wire; it never creates one.
            PropertyValue::WireRef { .. } | PropertyValue::Literal(_) => vec![],
        }
    }

    /// Wire all pending connections.
    fn wire_pending_connections(&mut self) {
        let connections = std::mem::take(&mut self.pending_connections);

        for conn in connections {
            if let Err(e) = self.wire_connection(&conn) {
                self.result.add_warning(format!(
                    "Connection warning for {}.{}: {}",
                    conn.dest_node_path, conn.param_name, e
                ));
            }
        }
    }

    /// Wire a single connection.
    fn wire_connection(&mut self, conn: &PendingConnection) -> Result<(), String> {
        // Resolve destination node
        let dest_node_id = self
            .lookup(&conn.scope, &conn.dest_node_name)
            .ok_or_else(|| format!("Destination node '{}' not found", conn.dest_node_path))?;

        // Get destination node's parameter index
        let (param_index, _is_multi) =
            self.get_param_index(&conn.scope, dest_node_id, &conn.param_name)?;

        // Mentioning a property assigns that pin's whole inbound wire set:
        // what the statement names replaces what was there. This clears for
        // array pins too, which is what lets `shapes: [a, b, c]` shrink to
        // `shapes: [a]` — appending only, as this used to do for multi pins,
        // made an array pin able to grow but never shrink. An empty source
        // list (a literal, or `[]`) therefore disconnects the pin; see
        // `property_disconnects_pin`.
        let removed_wires = {
            let dest_node = scope_net_mut(self.network, &conn.scope)
                .and_then(|network| network.nodes.get_mut(&dest_node_id))
                .ok_or_else(|| {
                    format!(
                        "Destination node '{}' not found in network",
                        conn.dest_node_path
                    )
                })?;

            // Ensure arguments vector is large enough
            while dest_node.arguments.len() <= param_index {
                dest_node.arguments.push(Argument::new());
            }

            let removed = dest_node.arguments[param_index].incoming_wires.len();
            dest_node.arguments[param_index].clear();
            removed
        };
        if conn.source_refs.is_empty() && removed_wires > 0 {
            self.result.connections_made.push(format!(
                "{}.{} disconnected ({} wire{} removed)",
                conn.dest_node_path,
                conn.param_name,
                removed_wires,
                if removed_wires == 1 { "" } else { "s" }
            ));
        }

        // Wire each source
        for source_ref in &conn.source_refs {
            let (wire, description) = self.resolve_source(&conn.scope, source_ref)?;

            // We need to re-borrow dest_node since we released it above
            if let Some(dest_node) = scope_net_mut(self.network, &conn.scope)
                .and_then(|network| network.nodes.get_mut(&dest_node_id))
            {
                dest_node.arguments[param_index].set_source_full(
                    wire.source_node_id,
                    wire.source_pin,
                    wire.source_scope_depth,
                );
            }

            self.result.connections_made.push(format!(
                "{}.{} <- {}",
                conn.dest_node_path, conn.param_name, description
            ));
        }

        Ok(())
    }

    /// Turn one written reference into the wire it encodes, relative to the
    /// scope the destination lives in. Also returns the reference as written,
    /// for reporting.
    ///
    /// The whole of D4's table is these two arms:
    ///
    /// - `k` carets → `NodeOutput` at `source_scope_depth = k`. A depth counts
    ///   *networks*, so `k = 0` is this scope.
    /// - `k` carets plus `$` → `ZoneInput` at `source_scope_depth = k + 1`,
    ///   naming the **owning HOF node** (never a body node). A `ZoneInput`
    ///   depth counts *owning-HOF body frames*, so `1` is this body's own
    ///   owner — hence the offset. It is the only arithmetic in the format.
    fn resolve_source(
        &self,
        scope: &[u64],
        source_ref: &SourceRef,
    ) -> Result<(IncomingWire, String), String> {
        let carets = "^".repeat(source_ref.depth);

        if source_ref.kind == RefKind::ZoneInput {
            // The owning HOF's body frame sits at `stack_len - depth`, i.e.
            // `$x` is the innermost owner and each caret steps one owner out.
            if source_ref.depth >= scope.len() {
                return Err(format!(
                    "'{}${}' reaches past the outermost enclosing body",
                    carets, source_ref.name
                ));
            }
            let owner_index = scope.len() - 1 - source_ref.depth;
            let owner_id = scope[owner_index];
            let owner_scope = &scope[..owner_index];
            let pin_index = self.zone_input_pin_index(owner_scope, owner_id, &source_ref.name)?;
            return Ok((
                IncomingWire {
                    source_node_id: owner_id,
                    source_pin: SourcePin::ZoneInput { pin_index },
                    source_scope_depth: (source_ref.depth + 1) as u8,
                },
                format!("{}${}", carets, source_ref.name),
            ));
        }

        let (depth, source_node_id) = if source_ref.explicit {
            if source_ref.depth > scope.len() {
                return Err(format!(
                    "'{}{}' reaches past the top-level network",
                    carets, source_ref.name
                ));
            }
            let source_scope = &scope[..scope.len() - source_ref.depth];
            let id = self
                .lookup(source_scope, &source_ref.name)
                .ok_or_else(|| format!("Source node '{}{}' not found", carets, source_ref.name))?;
            (source_ref.depth, id)
        } else {
            // A bare name resolves this body first, then outward; inner
            // shadows outer (D4). The serializer always writes the explicit
            // `^` form, so a round-trip never depends on this.
            (0..=scope.len())
                .find_map(|d| {
                    let source_scope = &scope[..scope.len() - d];
                    self.lookup(source_scope, &source_ref.name)
                        .map(|id| (d, id))
                })
                .ok_or_else(|| format!("Source node '{}' not found", source_ref.name))?
        };

        // Determine output pin index
        let output_pin_index = if source_ref.kind == RefKind::Function {
            -1
        } else if let Some(ref pin_name) = source_ref.pin_name {
            let source_scope = &scope[..scope.len() - depth];
            self.resolve_output_pin_index(source_scope, source_node_id, pin_name)?
        } else {
            0
        };

        let ref_type = if source_ref.kind == RefKind::Function {
            "@"
        } else {
            ""
        };
        let pin_suffix = source_ref
            .pin_name
            .as_ref()
            .map(|p| format!(".{}", p))
            .unwrap_or_default();
        Ok((
            IncomingWire {
                source_node_id,
                source_pin: SourcePin::NodeOutput {
                    pin_index: output_pin_index,
                },
                source_scope_depth: depth as u8,
            },
            format!(
                "{}{}{}{}",
                "^".repeat(depth),
                ref_type,
                source_ref.name,
                pin_suffix
            ),
        ))
    }

    /// Resolve a `$name` against the zone-input pins of the owning HOF node.
    ///
    /// The names are **not** uniform across node types (`element` / `acc` /
    /// `element1…N` / a closure's own parameter names), and for a `closure`
    /// they are the very `params:` the same statement may have just set — so
    /// they are read off the owner's *resolved* node type, which
    /// `populate_custom_node_type_cache` has already rebuilt from the node's
    /// data by the time a body block is applied. That ordering is not
    /// incidental: it is why `apply_body` runs after the properties (D13).
    fn zone_input_pin_index(
        &self,
        owner_scope: &[u64],
        owner_id: u64,
        pin_name: &str,
    ) -> Result<usize, String> {
        let owner = scope_net(self.network, owner_scope)
            .and_then(|network| network.nodes.get(&owner_id))
            .ok_or_else(|| "Enclosing body owner not found".to_string())?;
        let node_type = self
            .registry
            .get_node_type_for_node(owner)
            .ok_or_else(|| format!("Node type '{}' not found", owner.node_type_name))?;

        node_type
            .zone_input_pins
            .iter()
            .position(|pin| pin.name == pin_name)
            .ok_or_else(|| {
                format!(
                    "'{}' has no zone input '${}' (available: {})",
                    owner.node_type_name,
                    pin_name,
                    node_type
                        .zone_input_pins
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
    }

    /// Resolve an output pin name to its index on a source node.
    fn resolve_output_pin_index(
        &self,
        scope: &[u64],
        source_node_id: u64,
        pin_name: &str,
    ) -> Result<i32, String> {
        let source_node = scope_net(self.network, scope)
            .and_then(|network| network.nodes.get(&source_node_id))
            .ok_or_else(|| "Source node not found".to_string())?;

        let node_type = self
            .registry
            .get_node_type_for_node(source_node)
            .ok_or_else(|| {
                format!(
                    "Node type '{}' not found for pin resolution",
                    source_node.node_type_name
                )
            })?;

        for (index, pin_def) in node_type.output_pins.iter().enumerate() {
            if pin_def.name == pin_name {
                return Ok(index as i32);
            }
        }

        Err(format!(
            "Output pin '{}' not found on node type '{}' (available pins: {})",
            pin_name,
            source_node.node_type_name,
            node_type
                .output_pins
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    /// Get the parameter index for a parameter name.
    /// Returns (index, is_multi) where is_multi is true if the parameter accepts multiple inputs.
    fn get_param_index(
        &self,
        scope: &[u64],
        node_id: u64,
        param_name: &str,
    ) -> Result<(usize, bool), String> {
        let node = scope_net(self.network, scope)
            .and_then(|network| network.nodes.get(&node_id))
            .ok_or_else(|| "Node not found".to_string())?;

        let node_type = self
            .registry
            .get_node_type_for_node(node)
            .ok_or_else(|| format!("Node type '{}' not found", node.node_type_name))?;

        for (index, param) in node_type.parameters.iter().enumerate() {
            if param.name == param_name {
                // Multi-input parameters have array data types
                let is_multi = param.data_type.is_array();
                return Ok((index, is_multi));
            }
        }

        Err(format!(
            "Parameter '{}' not found on node type '{}'",
            param_name, node.node_type_name
        ))
    }

    // ------------------------------------------------------------------
    // Pass 2b: comment anchors
    // ------------------------------------------------------------------

    /// Collect a comment's `on:` anchors for the post-wiring pass.
    ///
    /// Accepts a single reference or an array of them; anything else warns and
    /// is skipped, matching how unknown properties are reported.
    fn collect_anchors(
        &mut self,
        scope: &[u64],
        comment_name: &str,
        comment_path: &str,
        node_id: u64,
        prop_value: &PropertyValue,
    ) {
        let is_comment = scope_net(self.network, scope)
            .and_then(|network| network.nodes.get(&node_id))
            .is_some_and(|node| node.data.as_any_ref().is::<CommentData>());
        if !is_comment {
            self.result.add_warning(format!(
                "'{}' is only supported on comment nodes; ignored on '{}'",
                ANCHOR_PROPERTY, comment_path
            ));
            return;
        }

        let items: Vec<&PropertyValue> = match prop_value {
            PropertyValue::Array(items) => items.iter().collect(),
            single => vec![single],
        };

        let mut targets = Vec::new();
        for item in items {
            match item {
                PropertyValue::NodeRef(name, None) => {
                    targets.push(PendingAnchorTarget::Node(name.clone()))
                }
                PropertyValue::NodeRef(name, Some(pin)) => self.result.add_warning(format!(
                    "Comment anchor '{}.{}' on '{}' dropped: a node anchor takes no output pin",
                    name, pin, comment_path
                )),
                PropertyValue::WireRef {
                    source,
                    source_depth,
                    dest,
                    dest_param,
                    ..
                } => targets.push(PendingAnchorTarget::Wire {
                    source: source.clone(),
                    source_depth: *source_depth,
                    dest: dest.clone(),
                    dest_param: dest_param.clone(),
                }),
                _ => self.result.add_warning(format!(
                    "Comment anchor on '{}' dropped: expected a node reference or a wire reference (`source -> dest.param`)",
                    comment_path
                )),
            }
        }

        // Pushed even when empty: `on: []` is how a text edit clears anchors.
        self.pending_anchors.push(PendingAnchors {
            scope: scope.to_vec(),
            comment_name: comment_name.to_string(),
            comment_path: comment_path.to_string(),
            targets,
        });
    }

    /// Resolve collected comment anchors to ids and store them on the comments.
    ///
    /// Runs after wiring, so a wire anchor can be checked against the wire it
    /// names. An anchor that does not resolve warns and is dropped rather than
    /// re-pointed at whatever is nearby (D6). Anchors inside a body resolve
    /// within that body, which is already how `drop_dangling_anchors` recurses.
    fn apply_pending_anchors(&mut self) {
        let pending = std::mem::take(&mut self.pending_anchors);

        for entry in pending {
            let Some(comment_id) = self.lookup(&entry.scope, &entry.comment_name) else {
                self.result.add_warning(format!(
                    "Comment '{}' not found; its anchors were dropped",
                    entry.comment_path
                ));
                continue;
            };

            let mut anchors = Vec::new();
            for target in &entry.targets {
                match self.resolve_pending_anchor(&entry.scope, target) {
                    Ok(anchor) => anchors.push(anchor),
                    Err(e) => self.result.add_warning(format!(
                        "Comment anchor on '{}' dropped: {}",
                        entry.comment_path, e
                    )),
                }
            }

            if let Some(node) = scope_net_mut(self.network, &entry.scope)
                .and_then(|network| network.nodes.get_mut(&comment_id))
                && let Some(data) = node.data.as_any_mut().downcast_mut::<CommentData>()
            {
                data.anchors = anchors;
            }
        }
    }

    /// Turn one name-form anchor target into a `CommentAnchor`.
    fn resolve_pending_anchor(
        &self,
        scope: &[u64],
        target: &PendingAnchorTarget,
    ) -> Result<CommentAnchor, String> {
        match target {
            PendingAnchorTarget::Node(name) => {
                let node_id = self
                    .lookup(scope, name)
                    .ok_or_else(|| format!("unknown node '{}'", name))?;
                Ok(CommentAnchor::Node(node_id))
            }
            PendingAnchorTarget::Wire {
                source,
                source_depth,
                dest,
                dest_param,
            } => {
                if *source_depth > scope.len() {
                    return Err(format!(
                        "source '{}{}' reaches past the top-level network",
                        "^".repeat(*source_depth),
                        source
                    ));
                }
                let source_scope = &scope[..scope.len() - *source_depth];
                let source_id = self
                    .lookup(source_scope, source)
                    .ok_or_else(|| format!("unknown source node '{}'", source))?;
                let dest_id = self
                    .lookup(scope, dest)
                    .ok_or_else(|| format!("unknown destination node '{}'", dest))?;
                let (param_index, _) = self.get_param_index(scope, dest_id, dest_param)?;
                let network = scope_net(self.network, scope)
                    .ok_or_else(|| "enclosing scope not found".to_string())?;
                let dest_node = network
                    .nodes
                    .get(&dest_id)
                    .ok_or_else(|| format!("destination node '{}' not found", dest))?;
                let incoming = dest_node
                    .arguments
                    .get(param_index)
                    .and_then(|arg| {
                        arg.incoming_wires.iter().find(|w| {
                            w.source_node_id == source_id
                                && w.source_scope_depth as usize == *source_depth
                        })
                    })
                    .ok_or_else(|| {
                        format!("no wire from '{}' to '{}.{}'", source, dest, dest_param)
                    })?;
                let wire = Wire {
                    source_node_id: incoming.source_node_id,
                    source_pin: incoming.source_pin,
                    source_scope_depth: incoming.source_scope_depth,
                    destination_node_id: dest_id,
                    destination_argument_index: param_index,
                    destination_argument_kind: ArgumentKind::External,
                };
                Ok(CommentAnchor::Wire(WireAnchor::from_wire(&wire, network)))
            }
        }
    }

    // ------------------------------------------------------------------
    // Pass 3: visibility, deletes, outputs
    // ------------------------------------------------------------------

    /// Apply visibility settings to nodes, in the scope each was named in.
    fn apply_visibility(&mut self) {
        let visible_nodes = std::mem::take(&mut self.visible_nodes);

        for (scope, node_name) in visible_nodes {
            if let Some(node_id) = self.lookup(&scope, &node_name)
                && let Some(network) = scope_net_mut(self.network, &scope)
            {
                network.set_node_display(node_id, true);
            }
        }
    }

    /// Run the deferred `delete` / `output` statements and the zone-output
    /// wire of every body block, in source order.
    fn apply_deferred_ops(&mut self) {
        let deferred = std::mem::take(&mut self.deferred);

        for op in deferred {
            match op {
                DeferredOp::Delete { scope, name, path } => {
                    if let Err(e) = self.delete_node(&scope, &name, &path) {
                        self.result.add_error(e);
                    }
                }
                DeferredOp::Output { name } => {
                    if let Err(e) = self.process_output(&name) {
                        self.result.add_error(e);
                    }
                }
                DeferredOp::ZoneOutput {
                    owner_scope,
                    owner_id,
                    owner_path,
                    body_scope,
                    source_name,
                } => self.apply_zone_output(
                    &owner_scope,
                    owner_id,
                    &owner_path,
                    &body_scope,
                    source_name.as_deref(),
                ),
            }
        }
    }

    /// Delete one node from `scope`.
    fn delete_node(&mut self, scope: &[u64], name: &str, path: &str) -> Result<(), String> {
        let node_id = self
            .lookup(scope, name)
            .ok_or_else(|| format!("Cannot delete '{}': node not found", path))?;

        {
            let network = scope_net_mut(self.network, scope)
                .ok_or_else(|| format!("Cannot delete '{}': scope not found", path))?;

            // Remove outgoing wires within this scope. A *sibling* HOF's
            // `zone_output_arguments` is deliberately left alone: its wires
            // name nodes in *its* body, and per-body `next_node_id` counters
            // make a bare id match there a coincidence, not a reference.
            for node in network.nodes.values_mut() {
                for argument in node.arguments.iter_mut() {
                    argument.remove_source(node_id);
                }
            }

            // Remove from displayed nodes
            network.displayed_nodes.remove(&node_id);

            // Clear return node if this was it
            if network.return_node_id == Some(node_id) {
                network.return_node_id = None;
            }

            // Remove the node (its incoming wires go with it)
            network.nodes.remove(&node_id);
        }

        // The owning HOF's zone-output wire points *into* this scope, so a
        // body-node delete must clear it rather than leave it dangling.
        if let Some((owner_id, owner_scope)) = scope.split_last()
            && let Some(owner) = scope_net_mut(self.network, owner_scope)
                .and_then(|network| network.nodes.get_mut(owner_id))
        {
            for argument in owner.zone_output_arguments.iter_mut() {
                argument.remove_source(node_id);
            }
        }

        // Update name maps
        let names = self.scope_names_mut(scope);
        names.name_to_id.remove(name);
        names.id_to_name.remove(&node_id);

        self.result.nodes_deleted.push(path.to_string());
        Ok(())
    }

    /// Process a top-level output statement.
    fn process_output(&mut self, node_name: &str) -> Result<(), String> {
        let node_id = self
            .lookup(&[], node_name)
            .ok_or_else(|| format!("Cannot set output to '{}': node not found", node_name))?;

        self.network.set_return_node(node_id);
        self.result.output_set = Some(node_name.to_string());
        Ok(())
    }

    /// Point (or clear) an HOF's zone-output wire from a `body { … }` block.
    ///
    /// `source_name = None` is the block that carried no `output` statement,
    /// and it **clears** the wire: the block is total over the parent's
    /// `zone_output_arguments` as well as over the body (D5).
    fn apply_zone_output(
        &mut self,
        owner_scope: &[u64],
        owner_id: u64,
        owner_path: &str,
        body_scope: &[u64],
        source_name: Option<&str>,
    ) {
        let source_id = match source_name {
            Some(name) => match self.lookup(body_scope, name) {
                Some(id) => Some(id),
                None => {
                    self.result.add_error(format!(
                        "Cannot set the body output of '{}' to '{}': no such node in its body",
                        owner_path, name
                    ));
                    return;
                }
            },
            None => None,
        };

        let wrote = {
            let Some(owner) = scope_net_mut(self.network, owner_scope)
                .and_then(|network| network.nodes.get_mut(&owner_id))
            else {
                // The owning node was deleted later in the same script.
                return;
            };
            if owner.zone_output_arguments.is_empty() {
                false
            } else {
                // Every zone type today has exactly one zone-output pin, so
                // the bare `output <node>` form suffices (D5). The clear
                // covers every pin so a type that grows a second one fails
                // loudly rather than keeping a stale wire.
                for argument in owner.zone_output_arguments.iter_mut() {
                    argument.clear();
                }
                if let Some(id) = source_id {
                    owner.zone_output_arguments[0]
                        .incoming_wires
                        .push(IncomingWire::node_output(id, 0));
                }
                true
            }
        };

        match (wrote, source_name) {
            (true, Some(name)) => self
                .result
                .connections_made
                .push(format!("{} body output <- {}", owner_path, name)),
            (false, Some(name)) => self.result.add_warning(format!(
                "'{}' declares no body-output pin; `output {}` inside its body was ignored",
                owner_path, name
            )),
            _ => {}
        }
    }
}

/// Apply edit commands to a node network.
///
/// This is the main entry point for network editing.
///
/// # Arguments
/// * `network` - The node network to edit
/// * `registry` - The node type registry for looking up node types
/// * `code` - The edit commands in text format
/// * `replace` - If true, replace entire network; if false, incremental merge
///
/// # Returns
/// An `EditResult` describing what was changed.
///
/// # Example
/// ```rust,ignore
/// let result = edit_network(&mut network, &registry, r#"
///     sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
///     output sphere1
/// "#, true);
///
/// if result.success {
///     println!("Created {} nodes", result.nodes_created.len());
/// }
/// ```
pub fn edit_network(
    network: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    code: &str,
    replace: bool,
) -> EditResult {
    let editor = NetworkEditor::new(network, registry);
    editor.apply(code, replace)
}

/// The current lane count of a `zip_with` node, `None` for any other node.
/// Used to detect a lane-list shrink across a `set_text_properties` call so
/// body wires to the dropped tail indices can be disconnected at mutation
/// time (`doc/design_zip_with.md` Phase 3).
fn zip_with_lane_count(node: &crate::node_network::Node) -> Option<usize> {
    if node.node_type_name != "zip_with" {
        return None;
    }
    node.data
        .as_any_ref()
        .downcast_ref::<crate::nodes::zip_with::ZipWithData>()
        .map(|d| d.lanes.len())
}
