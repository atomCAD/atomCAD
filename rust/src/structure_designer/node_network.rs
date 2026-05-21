use crate::structure_designer::camera_settings::CameraSettings;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{NodeType, OutputPinDefinition};
use crate::structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;
use serde::{Deserialize, Serialize};
use std::cmp::max;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use super::data_type::DataType;
use super::node_layout;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeDisplayType {
    Normal,
    Ghost,
}

/// Display state for a single node. Bundles node-level visibility (Normal/Ghost)
/// with per-output-pin display control.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeDisplayState {
    pub display_type: NodeDisplayType,
    pub displayed_pins: HashSet<i32>,
}

impl NodeDisplayState {
    /// Default display state: Normal visibility, pin 0 only.
    pub fn normal() -> Self {
        Self {
            display_type: NodeDisplayType::Normal,
            displayed_pins: HashSet::from([0]),
        }
    }

    pub fn with_type(display_type: NodeDisplayType) -> Self {
        Self {
            display_type,
            displayed_pins: HashSet::from([0]),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub error_text: String,
    pub node_id: Option<u64>,
}

impl ValidationError {
    pub fn new(error_text: String, node_id: Option<u64>) -> Self {
        Self {
            error_text,
            node_id,
        }
    }
}

/// Scope-aware address of a node. The `scope_path` is the chain of HOF node
/// ids identifying the body the node lives in; an empty path means the
/// top-level network. Used by the change-tracking and dependency-analysis
/// machinery so body-internal node ids don't collide with top-level ids
/// across nested per-body `next_node_id` counters.
///
/// See `doc/design_zones_ui.md` §"Mutation APIs grow a `scope_path` parameter"
/// for the broader scope-chain convention.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NodeRef {
    pub scope_path: Vec<u64>,
    pub node_id: u64,
}

impl NodeRef {
    /// A node in the top-level network.
    pub fn top(node_id: u64) -> Self {
        Self {
            scope_path: Vec::new(),
            node_id,
        }
    }

    /// A node in the named body. `scope_path` is cloned.
    pub fn scoped(scope_path: &[u64], node_id: u64) -> Self {
        Self {
            scope_path: scope_path.to_vec(),
            node_id,
        }
    }

    /// True if this ref addresses a top-level node (empty scope path).
    pub fn is_top_level(&self) -> bool {
        self.scope_path.is_empty()
    }
}

/// Source pin kind for an incoming wire. Phase 1 only ever constructs
/// `NodeOutput` (zone-input sources arrive with the zone work in later phases).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourcePin {
    /// Pin on a regular output of a node. `pin_index = -1` is the legacy
    /// function pin; `0` is the primary output; `>= 1` are additional outputs.
    NodeOutput { pin_index: i32 },
    /// Inside-facing source pin on a zone-owning node. Used in later phases
    /// when zones land — Phase 1 never constructs this variant.
    ZoneInput { pin_index: usize },
}

/// One inbound wire on an argument pin. Carries enough information to identify
/// the source side under both today's flat-network semantics and the
/// zones-world's cross-scope sources. Phase 1 invariant: every wire has
/// `source_pin = NodeOutput { .. }` and `source_scope_depth = 0`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncomingWire {
    pub source_node_id: u64,
    pub source_pin: SourcePin,
    /// `0` = source lives in the same network as the destination argument's
    /// scope; `>= 1` = walk that many ancestor frames up the network stack.
    /// Always `0` in Phase 1.
    #[serde(default)]
    pub source_scope_depth: u8,
}

impl IncomingWire {
    /// Constructs an `IncomingWire` for a regular-output source in the same
    /// scope as the destination — the only shape Phase 1 ever produces.
    pub fn node_output(source_node_id: u64, pin_index: i32) -> Self {
        Self {
            source_node_id,
            source_pin: SourcePin::NodeOutput { pin_index },
            source_scope_depth: 0,
        }
    }

    /// Returns the legacy `(source_node_id, output_pin_index)` pair iff this
    /// wire is a regular-output, local-scope wire. Convenience for the many
    /// callsites that don't yet care about zone-input or cross-scope wires.
    pub fn as_legacy_pair(&self) -> Option<(u64, i32)> {
        if self.source_scope_depth != 0 {
            return None;
        }
        match self.source_pin {
            SourcePin::NodeOutput { pin_index } => Some((self.source_node_id, pin_index)),
            SourcePin::ZoneInput { .. } => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Argument {
    /// Inbound wires on this argument pin. Phase 1 invariant: every entry has
    /// `source_pin = NodeOutput { .. }` and `source_scope_depth = 0`, and at
    /// most one entry per `source_node_id` (mirrors the old HashMap's keying).
    /// Order is meaningful — callsites should preserve insertion order so
    /// serialized output stays byte-stable across runs.
    pub incoming_wires: Vec<IncomingWire>,
}

impl Default for Argument {
    fn default() -> Self {
        Self::new()
    }
}

impl Argument {
    pub fn new() -> Self {
        Self {
            incoming_wires: Vec::new(),
        }
    }

    /// Returns Some(node_id) for one of the incoming wires if not empty,
    /// otherwise returns None.
    pub fn get_node_id(&self) -> Option<u64> {
        self.incoming_wires.first().map(|w| w.source_node_id)
    }

    /// Returns Some((node_id, output_pin_index)) for one of the incoming wires
    /// if not empty, otherwise returns None. Phase 1: every wire is a
    /// `NodeOutput` so the legacy pair is always available.
    pub fn get_node_id_and_pin(&self) -> Option<(u64, i32)> {
        self.incoming_wires.first().and_then(|w| w.as_legacy_pair())
    }

    pub fn is_empty(&self) -> bool {
        self.incoming_wires.is_empty()
    }

    pub fn len(&self) -> usize {
        self.incoming_wires.len()
    }

    pub fn clear(&mut self) {
        self.incoming_wires.clear();
    }

    /// True if any incoming wire's source is `node_id` (Phase 1: there is at
    /// most one such wire — see [`Argument`] uniqueness invariant).
    pub fn has_source(&self, node_id: u64) -> bool {
        self.incoming_wires
            .iter()
            .any(|w| w.source_node_id == node_id)
    }

    /// Returns the regular-output pin index of the wire from `node_id`, if any.
    pub fn get_source_pin(&self, node_id: u64) -> Option<i32> {
        self.incoming_wires
            .iter()
            .find(|w| w.source_node_id == node_id)
            .and_then(|w| w.as_legacy_pair().map(|(_, pin)| pin))
    }

    /// Set the wire from `source_node_id` to point at `pin_index`. If a wire
    /// from this source already exists, its pin index is updated in place;
    /// otherwise a new wire is appended. Phase 1 keeps the per-source
    /// uniqueness invariant inherited from the old HashMap representation.
    pub fn set_source(&mut self, source_node_id: u64, pin_index: i32) {
        if let Some(existing) = self
            .incoming_wires
            .iter_mut()
            .find(|w| w.source_node_id == source_node_id)
        {
            existing.source_pin = SourcePin::NodeOutput { pin_index };
            existing.source_scope_depth = 0;
        } else {
            self.incoming_wires
                .push(IncomingWire::node_output(source_node_id, pin_index));
        }
    }

    /// General variant of [`set_source`] that installs an arbitrary wire
    /// shape — used by zones UI phase U5 to author captures (depth ≥ 1) and
    /// iteration-value references (`ZoneInput` source). Replaces any existing
    /// wire from the same source node id, matching `set_source`'s per-source
    /// uniqueness invariant.
    pub fn set_source_full(
        &mut self,
        source_node_id: u64,
        source_pin: SourcePin,
        source_scope_depth: u8,
    ) {
        if let Some(existing) = self
            .incoming_wires
            .iter_mut()
            .find(|w| w.source_node_id == source_node_id)
        {
            existing.source_pin = source_pin;
            existing.source_scope_depth = source_scope_depth;
        } else {
            self.incoming_wires.push(IncomingWire {
                source_node_id,
                source_pin,
                source_scope_depth,
            });
        }
    }

    /// Remove the wire (if any) whose source is `node_id`. Returns the pin
    /// index that was attached to it, or `None` if no such wire existed.
    pub fn remove_source(&mut self, node_id: u64) -> Option<i32> {
        let pos = self
            .incoming_wires
            .iter()
            .position(|w| w.source_node_id == node_id)?;
        let removed = self.incoming_wires.remove(pos);
        removed.as_legacy_pair().map(|(_, pin)| pin)
    }

    /// Iterate the wires as legacy `(source_node_id, output_pin_index)` pairs.
    /// Phase 1: every wire is a `NodeOutput`, so all wires are yielded. Order
    /// follows the underlying `Vec` (deterministic, unlike the old HashMap).
    pub fn iter_source_pins(&self) -> impl Iterator<Item = (u64, i32)> + '_ {
        self.incoming_wires
            .iter()
            .filter_map(|w| w.as_legacy_pair())
    }

    /// Returns the wires as a `HashMap<source_node_id, output_pin_index>` —
    /// the shape `Argument` used to store directly. Kept for tests and
    /// adapter code that still want HashMap access semantics. Phase 1
    /// invariant guarantees no duplicate source ids in the result.
    pub fn argument_output_pins(&self) -> HashMap<u64, i32> {
        self.iter_source_pins().collect()
    }
}

// Backward-compatible serialization for `Argument`. New saves emit
// `incoming_wires`; old saves and migration outputs (which still produce
// `argument_output_pins`) deserialize into the same internal shape.
impl Serialize for Argument {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Argument", 1)?;
        state.serialize_field("incoming_wires", &self.incoming_wires)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Argument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ArgumentSerde {
            #[serde(default)]
            incoming_wires: Option<Vec<IncomingWire>>,
            #[serde(default)]
            argument_output_pins: Option<HashMap<u64, i32>>,
        }

        let raw = ArgumentSerde::deserialize(deserializer)?;
        if let Some(wires) = raw.incoming_wires {
            return Ok(Argument {
                incoming_wires: wires,
            });
        }
        if let Some(map) = raw.argument_output_pins {
            // Sort by source_node_id for deterministic ordering — the old
            // HashMap had no defined iteration order, so locking the upgrade
            // path to sorted order keeps roundtrip output byte-stable.
            let mut entries: Vec<(u64, i32)> = map.into_iter().collect();
            entries.sort_by_key(|&(nid, _)| nid);
            let wires = entries
                .into_iter()
                .map(|(nid, pin)| IncomingWire::node_output(nid, pin))
                .collect();
            return Ok(Argument {
                incoming_wires: wires,
            });
        }
        Ok(Argument::new())
    }
}

/// Which argument list on the destination node a wire terminates at.
///
/// `External` is today's normal pin: the destination argument is in the
/// destination node's containing network. `ZoneOutput` is an inside-facing
/// zone-output pin on a zone-owning (HOF) node: the destination argument is
/// in the HOF's owned body. Phase 2 lands the enum and the `Wire` view field
/// but every wire built today is `External`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArgumentKind {
    /// Sourced from the destination's `arguments` (today's behavior).
    #[default]
    External,
    /// Sourced from the destination's `zone_output_arguments` (HOF body
    /// returns). Not produced in Phase 2.
    ZoneOutput,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wire {
    pub source_node_id: u64,
    pub source_pin: SourcePin,
    /// `0` in Phase 1 (added now so later phases can carry capture-wire depth
    /// without touching every Wire literal again).
    #[serde(default)]
    pub source_scope_depth: u8,
    pub destination_node_id: u64,
    pub destination_argument_index: usize,
    /// Which argument list on the destination this wire terminates at.
    /// Phase 2 lands the field; all wires built today use `External`.
    #[serde(default)]
    pub destination_argument_kind: ArgumentKind,
}

impl Wire {
    /// Construct a Wire for a regular-output source in the same scope as the
    /// destination — the only shape Phase 1 ever produces. Convenience for the
    /// many callsites that used to take a bare `source_output_pin_index: i32`.
    pub fn node_output(
        source_node_id: u64,
        source_output_pin_index: i32,
        destination_node_id: u64,
        destination_argument_index: usize,
    ) -> Self {
        Self {
            source_node_id,
            source_pin: SourcePin::NodeOutput {
                pin_index: source_output_pin_index,
            },
            source_scope_depth: 0,
            destination_node_id,
            destination_argument_index,
            destination_argument_kind: ArgumentKind::External,
        }
    }

    /// Legacy pin index, for Phase 1 callsites that still think in terms of a
    /// single `i32`. Returns `None` for `ZoneInput` sources or non-zero scope
    /// depth (neither of which are produced in Phase 1).
    pub fn source_pin_index(&self) -> Option<i32> {
        if self.source_scope_depth != 0 {
            return None;
        }
        match self.source_pin {
            SourcePin::NodeOutput { pin_index } => Some(pin_index),
            SourcePin::ZoneInput { .. } => None,
        }
    }

    /// Same as [`source_pin_index`] but panics on the future shapes — callers
    /// that statically know they're in Phase 1 use this to avoid threading
    /// `Option` through every site.
    pub fn expect_node_output_pin(&self) -> i32 {
        self.source_pin_index()
            .expect("Wire is not a regular-output, local-scope wire (Phase 1 invariant violated)")
    }
}

impl PartialEq for Wire {
    fn eq(&self, other: &Self) -> bool {
        self.source_node_id == other.source_node_id
            && self.source_pin == other.source_pin
            && self.source_scope_depth == other.source_scope_depth
            && self.destination_node_id == other.destination_node_id
            && self.destination_argument_index == other.destination_argument_index
            && self.destination_argument_kind == other.destination_argument_kind
    }
}

impl Eq for Wire {}

/// Information about what `delete_selected()` would delete.
/// Returned by `collect_deletion_info()` for the undo system.
#[derive(Default)]
pub struct DeletionInfo {
    /// IDs of nodes that would be deleted
    pub deleted_node_ids: Vec<u64>,
    /// All wires connected to deleted nodes (both incoming and outgoing)
    pub deleted_wires: Vec<Wire>,
    /// If the return node would be deleted, its ID
    pub was_return_node: Option<u64>,
    /// Display states of deleted nodes: (node_id, display_type)
    pub display_states: Vec<(u64, NodeDisplayType)>,
    /// Selected wires that would be deleted (when no nodes are selected)
    pub selected_wires: Vec<Wire>,
    /// True if this is a node deletion, false if wire-only deletion
    pub is_node_deletion: bool,
}

/// The user's choice for whether a collapsable HOF's inline body region is
/// shown. `Auto` (the default) derives the effective state from whether the
/// `f` pin is wired (compact when wired — the body is dead — expanded
/// otherwise); the two overrides force it. Meaningful only for collapsable
/// HOFs; inert (`Auto`) on every other node. See
/// `doc/design_hof_node_collapse.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CollapseMode {
    #[default]
    Auto,
    Collapsed,
    Expanded,
}

#[derive(Clone)]
pub struct Node {
    pub id: u64,
    pub node_type_name: String,
    /// User-specified name for this node (e.g., "mybox" from "mybox = cuboid {...}").
    /// If None, the node will be named using auto-generated names like "cuboid1".
    pub custom_name: Option<String>,
    pub position: DVec2,
    pub arguments: Vec<Argument>,
    pub data: Box<dyn NodeData>,
    pub custom_node_type: Option<NodeType>,
    /// The HOF body owned by this node. CoW-shared via `Arc` so `Node::clone`
    /// is a refcount bump rather than a deep walk; mutation flows through
    /// `zone_mut()` which wraps `Arc::make_mut`. Always `None` for non-HOF
    /// node types. Phase 2 lands the field; no built-in type populates it yet.
    pub zone: Option<Arc<NodeNetwork>>,
    /// Wires terminating at this node's zone-output (inside-right) pins. Each
    /// `Argument` here mirrors one zone-output pin on the node type. Empty for
    /// every non-HOF node. Phase 2 lands the field; no built-in type populates
    /// it yet.
    pub zone_output_arguments: Vec<Argument>,
    /// Stored body width in logical pixels for HOF nodes. The body's rendered
    /// width is `max(body_width, content_bbox + padding)` (zones UI design
    /// doc §"Body sizing"). Meaningful only when `zone.is_some()`; the value
    /// on non-HOF nodes is the default (`DEFAULT_BODY_WIDTH`) and unused.
    pub body_width: f64,
    /// Stored body height in logical pixels for HOF nodes. See [`body_width`].
    pub body_height: f64,
    /// The user's collapse choice for this node. Only meaningful for
    /// collapsable HOFs (`map`/`filter`/`fold`/`foreach`); `Auto` (the default)
    /// on every other node and inert there. See
    /// `doc/design_hof_node_collapse.md`.
    pub collapse_mode: CollapseMode,
}

/// Default stored body width for newly created HOF nodes (logical pixels).
/// See `doc/design_zones_ui.md` §"Body sizing".
pub const DEFAULT_BODY_WIDTH: f64 = 320.0;

/// Default stored body height for newly created HOF nodes (logical pixels).
pub const DEFAULT_BODY_HEIGHT: f64 = 180.0;

/// Built-in HOF type names that are collapsable. Equivalent to "has a zone and
/// declares an `f` parameter"; kept as a name list so the load path (which only
/// has the type name) and the runtime path (which has the `NodeType`) agree.
/// See `doc/design_hof_node_collapse.md`.
pub const COLLAPSABLE_HOF_TYPE_NAMES: &[&str] = &["map", "filter", "fold", "foreach"];

/// True iff `name` is one of the four collapsable HOF type names.
pub fn collapsable_type_name(name: &str) -> bool {
    COLLAPSABLE_HOF_TYPE_NAMES.contains(&name)
}

/// Returns true if `node` has an input pin named `f` of `Function` type that
/// carries at least one incoming wire.
///
/// HOFs gain an optional `f: Function` pin which, when wired, drives evaluation
/// in place of the inline body (so the "zone-output pin must have a wire" rule
/// is suspended for that HOF). `apply` has a *required* `f` pin. The `closure`
/// node has no `f` *input* pin (it exposes a `Function` *output*), so this is
/// always false for it. See `doc/design_closures.md` §"Validation".
pub fn function_input_pin_connected(node: &Node, node_type: &NodeType) -> bool {
    node_type
        .parameters
        .iter()
        .position(|p| p.name == "f" && matches!(p.data_type, DataType::Function(_)))
        .and_then(|idx| node.arguments.get(idx))
        .map(|arg| !arg.incoming_wires.is_empty())
        .unwrap_or(false)
}

/// Resolve a node's [`CollapseMode`] to the effective "body hidden + node
/// compact" bool. Always false for non-collapsable nodes (so a stray override
/// on a `closure` or a hand-edited file can never compact it). For collapsable
/// HOFs, `Auto` reads the `f`-connection; the two overrides force the result.
/// See `doc/design_hof_node_collapse.md`.
pub fn resolve_body_collapsed(node: &Node, node_type: &NodeType) -> bool {
    if !collapsable_type_name(&node.node_type_name) {
        return false;
    }
    match node.collapse_mode {
        CollapseMode::Auto => function_input_pin_connected(node, node_type),
        CollapseMode::Collapsed => true,
        CollapseMode::Expanded => false,
    }
}

impl Node {
    /// Mutable access to this node's owned zone body, lazily cloning under
    /// `Arc::make_mut`. Returns `None` for nodes that don't own a zone (every
    /// node today — Phase 2 lands the field without populating it). All body
    /// edits must go through this accessor so callers don't reach into the
    /// `Arc` directly and accidentally break sharing.
    pub fn zone_mut(&mut self) -> Option<&mut NodeNetwork> {
        self.zone.as_mut().map(|arc| Arc::make_mut(arc))
    }

    /// In debug builds, panic if this node's zone state is inconsistent with
    /// its declared `NodeType`: non-HOF types (no zone pins) must have
    /// `zone == None` and an empty `zone_output_arguments`. Phase 2 lands the
    /// fields but no built-in type populates them, so the invariant should
    /// hold trivially everywhere. Cheap in release (no-op).
    #[inline]
    pub fn debug_assert_zone_consistency(&self, node_type: &NodeType) {
        if !node_type.has_zone() {
            debug_assert!(
                self.zone.is_none(),
                "node {} ({}) has a populated zone but its type declares no zone pins",
                self.id,
                self.node_type_name
            );
            debug_assert!(
                self.zone_output_arguments.is_empty(),
                "node {} ({}) has zone_output_arguments but its type declares no zone-output pins",
                self.id,
                self.node_type_name
            );
        } else {
            debug_assert_eq!(
                self.zone_output_arguments.len(),
                node_type.zone_output_pins.len(),
                "node {} ({}) has {} zone_output_arguments but its type declares {} zone-output pins",
                self.id,
                self.node_type_name,
                self.zone_output_arguments.len(),
                node_type.zone_output_pins.len(),
            );
        }
    }

    /// Ensure this node's zone state matches the given `NodeType`'s zone
    /// declaration. Called from `populate_custom_node_type_cache_with_types`
    /// after the custom node type has been installed, and from
    /// `repair_node_network` when a zone-bearing node type's pin layout
    /// changes.
    ///
    /// For HOF (zone-bearing) types this:
    /// 1. Lazily initializes `self.zone` to an empty body network if it's
    ///    currently `None`.
    /// 2. Resizes `self.zone_output_arguments` to match the number of
    ///    zone-output pins, preserving existing wires where the index lines
    ///    up.
    ///
    /// For non-HOF types this is a no-op (the `debug_assert_zone_consistency`
    /// invariant guarantees `zone == None` / `zone_output_arguments.is_empty()`
    /// for those, set at construction time).
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn ensure_zone_init(&mut self, node_type: &NodeType) {
        if !node_type.has_zone() {
            return;
        }
        if self.zone.is_none() {
            // `Arc<NodeNetwork>` is not `Send + Sync` because `NodeNetwork`
            // transitively holds `Box<dyn NodeData>`. We use `Arc` rather
            // than `Rc` for forward-compat with multi-threaded evaluation,
            // matching the convention in `Walker::FromArray`.
            self.zone = Some(Arc::new(NodeNetwork::new_empty()));
        }
        let want = node_type.zone_output_pins.len();
        if self.zone_output_arguments.len() < want {
            self.zone_output_arguments.resize_with(want, Argument::new);
        } else if self.zone_output_arguments.len() > want {
            self.zone_output_arguments.truncate(want);
        }
    }

    /// Sets the custom node type and intelligently preserves existing argument connections
    /// when parameter IDs (primary) or names (fallback) match between old and new node types
    pub fn set_custom_node_type(&mut self, custom_node_type: Option<NodeType>, refresh_args: bool) {
        if let Some(ref new_node_type) = custom_node_type {
            // Check if we can preserve existing arguments (same parameters in same order)
            let can_preserve = if let Some(ref old_node_type) = self.custom_node_type {
                // Check if parameters match by ID (if both have IDs) or by name
                old_node_type.parameters.len() == new_node_type.parameters.len()
                    && old_node_type
                        .parameters
                        .iter()
                        .zip(new_node_type.parameters.iter())
                        .all(|(old_param, new_param)| {
                            // Match by ID if both have IDs, otherwise by name
                            match (old_param.id, new_param.id) {
                                (Some(old_id), Some(new_id)) => old_id == new_id,
                                _ => old_param.name == new_param.name,
                            }
                        })
            } else {
                false
            };

            if (!refresh_args) || can_preserve {
                // Parameters match exactly, keep existing arguments
                // (no changes to self.arguments)
            } else {
                // Parameters changed, need to rebuild arguments array
                let mut new_arguments = vec![Argument::new(); new_node_type.parameters.len()];

                // Try to preserve connections using ID-based matching (primary) or name-based (fallback)
                if let Some(ref old_node_type) = self.custom_node_type {
                    // Build ID map for old parameters
                    let old_id_map: std::collections::HashMap<u64, usize> = old_node_type
                        .parameters
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, p)| p.id.map(|id| (id, idx)))
                        .collect();

                    for (new_index, new_param) in new_node_type.parameters.iter().enumerate() {
                        // First try ID-based matching (handles renames)
                        let old_index = if let Some(new_id) = new_param.id {
                            if let Some(&idx) = old_id_map.get(&new_id) {
                                Some(idx)
                            } else {
                                // Fall back to name-based matching
                                old_node_type
                                    .parameters
                                    .iter()
                                    .position(|old_param| old_param.name == new_param.name)
                            }
                        } else {
                            // No ID, use name-based matching (backwards compatibility)
                            old_node_type
                                .parameters
                                .iter()
                                .position(|old_param| old_param.name == new_param.name)
                        };

                        // Copy argument connections from old position to new position
                        if let Some(old_idx) = old_index {
                            if old_idx < self.arguments.len() {
                                new_arguments[new_index] = self.arguments[old_idx].clone();
                            }
                        }
                    }
                }

                self.arguments = new_arguments;
            }
        }
        self.custom_node_type = custom_node_type;
    }
}

/*
 * A node network is a network of nodes used by users to create geometries and atomic structures.
 * A node network can also be an implementation of a non-built-in node type.
 * In this case it might or might not have parameters.
 */
#[derive(Clone)]
pub struct NodeNetwork {
    pub next_node_id: u64,
    pub next_param_id: u64, // Counter for generating unique parameter IDs within this network
    pub node_type: NodeType, // This is the node type when this node network is used as a node in another network. (analog to a function header in programming)
    pub nodes: HashMap<u64, Node>,
    pub return_node_id: Option<u64>, // Only node networks with a return node can be used as a node (a.k.a can be called)
    pub displayed_nodes: HashMap<u64, NodeDisplayState>, // Map of nodes that are currently displayed with their display state
    pub selected_node_ids: HashSet<u64>,                 // All selected nodes (multi-selection)
    pub active_node_id: Option<u64>, // Active node (for properties panel/gadget) - the last selected node
    pub selected_wires: Vec<Wire>,   // All selected wires (multi-selection)
    pub valid: bool,                 // Whether the node network is valid and can be evaluated
    pub validation_errors: Vec<ValidationError>, // List of validation errors if any
    /// Camera settings for this network's 3D viewport.
    /// When None, uses default camera position.
    pub camera_settings: Option<CameraSettings>,
}

/// Resolve the source side of an `IncomingWire` into a `NodeRef` against
/// the destination's `scope_path`. Used by [`NodeNetwork::build_scope_reverse_dependency_map`].
///
/// - `NodeOutput` wires: source lives `source_scope_depth` levels up from
///   the destination's scope. `depth = 0` keeps the same scope; `depth ≥ 1`
///   pops that many ids off the path (captures from ancestor scopes).
/// - `ZoneInput` wires: the value is supplied by the enclosing HOF's
///   zone-input pin, which derives from the HOF's own external inputs.
///   The body-node → HOF synthetic edge (added separately) already
///   captures this dependency; we return `None` here to avoid double-
///   counting and to skip the fragile "find the right ancestor HOF id"
///   resolution.
///
/// Returns `None` if `source_scope_depth` exceeds the scope path length —
/// in practice that's a validation error and would have been caught by
/// `validate_zones_recursive`, so it's safe to drop the edge silently.
fn resolve_wire_source(
    dest_scope_path: &[u64],
    source_scope_depth: u8,
    source_pin: SourcePin,
    source_node_id: u64,
) -> Option<NodeRef> {
    match source_pin {
        SourcePin::NodeOutput { .. } => {
            let depth = source_scope_depth as usize;
            if depth > dest_scope_path.len() {
                return None;
            }
            let source_scope = &dest_scope_path[..dest_scope_path.len() - depth];
            Some(NodeRef::scoped(source_scope, source_node_id))
        }
        SourcePin::ZoneInput { .. } => None,
    }
}

impl NodeNetwork {
    /// Builds a reverse dependency map (downstream connections)
    ///
    /// For each node, this returns the set of nodes that depend on it
    /// (i.e., nodes that have this node as an input in their arguments)
    ///
    /// # Returns
    /// A HashMap where:
    /// - Key: source node ID
    /// - Value: HashSet of node IDs that have the key node as an input
    ///
    /// # Example
    /// If node B depends on node A (A → B), then the map will contain:
    /// - Key: A, Value: {B}
    pub fn build_reverse_dependency_map(&self) -> HashMap<u64, HashSet<u64>> {
        let mut reverse_map: HashMap<u64, HashSet<u64>> = HashMap::new();

        for (&node_id, node) in &self.nodes {
            for arg in &node.arguments {
                for wire in &arg.incoming_wires {
                    // node_id depends on source_node_id
                    // So source_node_id has node_id as a downstream dependent
                    reverse_map
                        .entry(wire.source_node_id)
                        .or_default()
                        .insert(node_id);
                }
            }
        }

        reverse_map
    }

    /// Scope-aware reverse-dependency map across the entire zone tree.
    ///
    /// Returns a map from each node's `NodeRef` (top-level or body-internal) to
    /// the set of nodes that depend on it. Covers all four wire roles from
    /// `doc/design_zones.md` §"How wires represent each role under zones":
    ///
    /// 1. Intra-scope wires (in `node.arguments`, `source_scope_depth = 0`).
    /// 2. Captures (in body-internal `arguments`, `source_scope_depth ≥ 1`,
    ///    `source_pin = NodeOutput`) — source lives in an ancestor scope.
    /// 3. Zone-input references (`source_pin = ZoneInput`) — source is the
    ///    enclosing HOF's zone-input pin. The HOF's zone-input pins exist
    ///    because of the HOF's own external inputs (`xs`, `init`, …); a body
    ///    edit that consumes `ZoneInput` therefore depends on the HOF's
    ///    state via the synthetic edge below, so we don't add an explicit
    ///    edge for `ZoneInput` here.
    /// 4. Body-return wires (in `node.zone_output_arguments`,
    ///    `source_scope_depth = 0` relative to the body) — source is a
    ///    body-internal node, destination is the HOF in the parent scope.
    ///
    /// Plus a **synthetic body-node → enclosing-HOF edge** for every node
    /// inside a body: changing any node in a body changes the HOF's per-step
    /// output, which lifts the dirtiness out of the body into the parent
    /// scope. This is what makes "editing an `expr` inside a `map` body
    /// invalidates the `map`" work even when the body return wire happens
    /// to be wired through a different body-internal node.
    pub fn build_scope_reverse_dependency_map(&self) -> HashMap<NodeRef, HashSet<NodeRef>> {
        let mut reverse_map: HashMap<NodeRef, HashSet<NodeRef>> = HashMap::new();
        let mut scope_path: Vec<u64> = Vec::new();
        Self::walk_scope_reverse_deps(self, &mut scope_path, &mut reverse_map);
        reverse_map
    }

    fn walk_scope_reverse_deps(
        network: &NodeNetwork,
        scope_path: &mut Vec<u64>,
        reverse_map: &mut HashMap<NodeRef, HashSet<NodeRef>>,
    ) {
        for (&node_id, node) in &network.nodes {
            let dest_ref = NodeRef::scoped(scope_path, node_id);

            // (1) and (2): intra-scope wires + captures + zone-input references.
            for arg in &node.arguments {
                for wire in &arg.incoming_wires {
                    if let Some(source_ref) = resolve_wire_source(
                        scope_path,
                        wire.source_scope_depth,
                        wire.source_pin,
                        wire.source_node_id,
                    ) {
                        reverse_map
                            .entry(source_ref)
                            .or_default()
                            .insert(dest_ref.clone());
                    }
                }
            }

            // Synthetic edge: this node → enclosing HOF (if we're inside a body).
            if let Some((&hof_id, hof_scope)) = scope_path.split_last() {
                let hof_ref = NodeRef::scoped(hof_scope, hof_id);
                reverse_map
                    .entry(dest_ref.clone())
                    .or_default()
                    .insert(hof_ref);
            }

            // Recurse into this node's zone body, if any.
            if let Some(body) = node.zone.as_ref() {
                // (4): body-return wires — destination is THIS HOF node, sources
                // live inside the body that we're about to recurse into. Walk
                // them now while we still have `node` in hand.
                let hof_ref_for_returns = dest_ref.clone();
                scope_path.push(node_id);
                for arg in &node.zone_output_arguments {
                    for wire in &arg.incoming_wires {
                        if let Some(source_ref) = resolve_wire_source(
                            scope_path,
                            wire.source_scope_depth,
                            wire.source_pin,
                            wire.source_node_id,
                        ) {
                            reverse_map
                                .entry(source_ref)
                                .or_default()
                                .insert(hof_ref_for_returns.clone());
                        }
                    }
                }
                // Recurse into the body itself.
                Self::walk_scope_reverse_deps(body, scope_path, reverse_map);
                scope_path.pop();
            }
        }
    }

    /// Returns a HashSet of all node IDs that are directly connected to the given node
    /// This includes both nodes that provide input to this node and nodes that receive output from this node
    pub fn get_connected_node_ids(&self, node_id: u64) -> HashSet<u64> {
        let mut connected_ids = HashSet::new();

        // Check if the node exists
        if !self.nodes.contains_key(&node_id) {
            return connected_ids; // Return empty set if node doesn't exist
        }

        // Get nodes that provide input to this node (input connections)
        if let Some(node) = self.nodes.get(&node_id) {
            for argument in &node.arguments {
                // Add all node IDs that provide input to this node
                connected_ids.extend(argument.incoming_wires.iter().map(|w| w.source_node_id));
            }
        }

        // Get nodes that receive output from this node (output connections)
        for (other_id, other_node) in &self.nodes {
            // Skip the node itself
            if *other_id == node_id {
                continue;
            }

            // Check if any of this node's arguments reference the given node
            for argument in &other_node.arguments {
                if argument.has_source(node_id) {
                    connected_ids.insert(*other_id);
                    break; // No need to check other arguments of this node
                }
            }
        }

        connected_ids
    }

    pub fn new(node_type: NodeType) -> Self {
        Self {
            next_node_id: 1,
            next_param_id: 1, // Start parameter IDs at 1
            node_type,
            nodes: HashMap::new(),
            return_node_id: None,
            displayed_nodes: HashMap::new(),
            selected_node_ids: HashSet::new(),
            active_node_id: None,
            selected_wires: Vec::new(),
            valid: true,
            validation_errors: Vec::new(),
            camera_settings: None, // Will be populated on first use or from saved file
        }
    }

    /// Creates an empty NodeNetwork with a placeholder node type.
    /// Used for clipboard and other transient networks.
    pub fn new_empty() -> Self {
        use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
        use crate::structure_designer::data_type::DataType;
        use crate::structure_designer::node_data::NoData;
        use crate::structure_designer::node_type::{NodeType, no_data_loader, no_data_saver};

        let placeholder_type = NodeType {
            name: String::new(),
            description: String::new(),
            summary: None,
            category: NodeTypeCategory::OtherBuiltin,
            parameters: vec![],
            output_pins: OutputPinDefinition::single(DataType::None),
            zone_input_pins: vec![],
            zone_output_pins: vec![],
            public: false,
            node_data_creator: || Box::new(NoData {}),
            node_data_saver: no_data_saver,
            node_data_loader: no_data_loader,
        };
        Self::new(placeholder_type)
    }

    /// Copies nodes from another network into this network.
    ///
    /// Internal connections between copied nodes are preserved (with remapped IDs).
    /// External connections (to nodes not in source_node_ids) are dropped.
    /// Each pasted node gets a fresh ID, a unique display name, and is set to displayed.
    ///
    /// Returns the list of newly created node IDs.
    pub fn copy_nodes_from(
        &mut self,
        source: &NodeNetwork,
        source_node_ids: &HashSet<u64>,
        position_offset: DVec2,
    ) -> Vec<u64> {
        let mut old_to_new: HashMap<u64, u64> = HashMap::new();
        let mut new_ids: Vec<u64> = Vec::new();

        // Step 1 — Create all nodes
        for &old_id in source_node_ids {
            let source_node = match source.nodes.get(&old_id) {
                Some(node) => node,
                None => continue,
            };

            let new_id = self.next_node_id;
            self.next_node_id += 1;
            old_to_new.insert(old_id, new_id);

            let cloned_data = source_node.data.clone_box();
            let cloned_arguments = source_node.arguments.clone();
            let cloned_zone_output_arguments = source_node.zone_output_arguments.clone();
            let cloned_zone = source_node.zone.clone();
            let custom_node_type = source_node.custom_node_type.clone();
            let node_type_name = source_node.node_type_name.clone();
            let new_position = source_node.position + position_offset;
            let display_name = self.generate_unique_display_name(&node_type_name);

            let new_node = Node {
                id: new_id,
                node_type_name,
                custom_name: Some(display_name),
                position: new_position,
                arguments: cloned_arguments,
                data: cloned_data,
                custom_node_type,
                zone: cloned_zone,
                zone_output_arguments: cloned_zone_output_arguments,
                body_width: source_node.body_width,
                body_height: source_node.body_height,
                collapse_mode: source_node.collapse_mode,
            };

            self.nodes.insert(new_id, new_node);
            self.set_node_display(new_id, true);
            new_ids.push(new_id);
        }

        // Step 2 — Remap arguments (runs after all nodes are created)
        for &new_id in &new_ids {
            if let Some(node) = self.nodes.get_mut(&new_id) {
                for arg in &mut node.arguments {
                    let remapped: Vec<IncomingWire> = arg
                        .incoming_wires
                        .iter()
                        .filter_map(|wire| {
                            old_to_new
                                .get(&wire.source_node_id)
                                .map(|&mapped_id| IncomingWire {
                                    source_node_id: mapped_id,
                                    source_pin: wire.source_pin,
                                    source_scope_depth: wire.source_scope_depth,
                                })
                        })
                        .collect();
                    arg.incoming_wires = remapped;
                }
            }
        }

        new_ids
    }

    /// Generate a unique display name for a new node of the given type.
    ///
    /// Scans existing nodes to find the highest counter used for this type,
    /// then returns `{type}{max+1}`. Names are never reused even if nodes
    /// are deleted, ensuring stability for external references.
    pub fn generate_unique_display_name(&self, node_type: &str) -> String {
        let mut max_counter = 0;
        for node in self.nodes.values() {
            if let Some(ref name) = node.custom_name {
                if let Some(num_str) = name.strip_prefix(node_type) {
                    if let Ok(num) = num_str.parse::<u32>() {
                        max_counter = max_counter.max(num);
                    }
                }
            }
        }
        format!("{}{}", node_type, max_counter + 1)
    }

    pub fn add_node(
        &mut self,
        node_type_name: &str,
        position: DVec2,
        num_of_parameters: usize,
        node_data: Box<dyn NodeData>,
    ) -> u64 {
        let node_id = self.next_node_id;
        let display_name = self.generate_unique_display_name(node_type_name);
        let mut arguments: Vec<Argument> = Vec::new();
        for _i in 0..num_of_parameters {
            arguments.push(Argument::new());
        }

        let node = Node {
            id: node_id,
            node_type_name: node_type_name.to_string(),
            custom_name: Some(display_name),
            position,
            arguments,
            data: node_data,
            custom_node_type: None,
            zone: None,
            zone_output_arguments: Vec::new(),
            body_width: DEFAULT_BODY_WIDTH,
            body_height: DEFAULT_BODY_HEIGHT,
            collapse_mode: CollapseMode::Auto,
        };

        self.next_node_id += 1;
        self.nodes.insert(node_id, node);
        self.set_node_display(node_id, true);
        node_id
    }

    /// Add a node with a specific ID (used by undo/redo system).
    /// Updates `next_node_id` if the provided ID is >= current next_node_id.
    pub fn add_node_with_id(
        &mut self,
        node_id: u64,
        node_type_name: &str,
        position: DVec2,
        num_of_parameters: usize,
        node_data: Box<dyn NodeData>,
    ) {
        let mut arguments: Vec<Argument> = Vec::new();
        for _i in 0..num_of_parameters {
            arguments.push(Argument::new());
        }

        let display_name = self.generate_unique_display_name(node_type_name);
        let node = Node {
            id: node_id,
            node_type_name: node_type_name.to_string(),
            custom_name: Some(display_name),
            position,
            arguments,
            data: node_data,
            custom_node_type: None,
            zone: None,
            zone_output_arguments: Vec::new(),
            body_width: DEFAULT_BODY_WIDTH,
            body_height: DEFAULT_BODY_HEIGHT,
            collapse_mode: CollapseMode::Auto,
        };

        // Ensure next_node_id stays ahead of any manually assigned ID
        if node_id >= self.next_node_id {
            self.next_node_id = node_id + 1;
        }

        self.nodes.insert(node_id, node);
        self.set_node_display(node_id, true);
    }

    pub fn move_node(&mut self, node_id: u64, position: DVec2) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.position = position;
        }
    }

    pub fn can_connect_nodes(
        &self,
        source_node_id: u64,
        source_output_pin_index: i32,
        dest_node_id: u64,
        dest_param_index: usize,
        node_type_registry: &crate::structure_designer::node_type_registry::NodeTypeRegistry,
    ) -> bool {
        // Check if both nodes exist
        let source_node = match self.nodes.get(&source_node_id) {
            Some(node) => node,
            None => return false,
        };

        let dest_node = match self.nodes.get(&dest_node_id) {
            Some(node) => node,
            None => return false,
        };

        // Check if the destination parameter index is valid
        if dest_param_index >= dest_node.arguments.len() {
            return false;
        }

        // Get the expected input type for the destination parameter.
        // The destination pin's declared type may be abstract.
        let dest_param_type =
            node_type_registry.get_node_param_data_type(dest_node, dest_param_index);

        // Get the resolved concrete output type of the source pin. For a
        // polymorphic pin that cannot yet be resolved (e.g. its own input is
        // disconnected), the connection is considered invalid.
        let source_output_type = match node_type_registry.resolve_output_type(
            source_node,
            self,
            source_output_pin_index,
        ) {
            Some(t) => t,
            None => return false,
        };

        // Check if the data types are compatible using conversion rules
        DataType::can_be_converted_to(&source_output_type, &dest_param_type, node_type_registry)
    }

    pub fn connect_nodes(
        &mut self,
        source_node_id: u64,
        source_output_pin_index: i32,
        dest_node_id: u64,
        dest_param_index: usize,
        dest_param_is_multi: bool,
    ) {
        if let Some(dest_node) = self.nodes.get_mut(&dest_node_id) {
            let argument = &mut dest_node.arguments[dest_param_index];
            // In case of single parameters we need to disconnect the existing parameter first
            if (!dest_param_is_multi) && (!argument.is_empty()) {
                argument.clear();
            }
            argument.set_source(source_node_id, source_output_pin_index);
        }
    }

    /// General-shape variant of [`connect_nodes`] for zones UI phase U5.
    /// Stores an arbitrary [`SourcePin`] + `source_scope_depth` on the wire
    /// landing in `dest_node`'s `arguments[dest_param_index]`. The destination
    /// always lives in this network (External argument kind) — body-return
    /// wires (ZoneOutput) are handled separately via
    /// `StructureDesigner::connect_zone_output_wire`.
    pub fn connect_wire(
        &mut self,
        source_node_id: u64,
        source_pin: SourcePin,
        source_scope_depth: u8,
        dest_node_id: u64,
        dest_param_index: usize,
        dest_param_is_multi: bool,
    ) {
        if let Some(dest_node) = self.nodes.get_mut(&dest_node_id) {
            if dest_param_index >= dest_node.arguments.len() {
                return;
            }
            let argument = &mut dest_node.arguments[dest_param_index];
            if (!dest_param_is_multi) && (!argument.is_empty()) {
                argument.clear();
            }
            argument.set_source_full(source_node_id, source_pin, source_scope_depth);
        }
    }

    pub fn set_node_network_data(&mut self, node_id: u64, data: Box<dyn NodeData>) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.data = data;
        }
    }

    pub fn get_node_network_data(&self, node_id: u64) -> Option<&dyn NodeData> {
        self.nodes.get(&node_id).map(|node| node.data.as_ref())
    }

    pub fn get_node_network_data_mut(&mut self, node_id: u64) -> Option<&mut dyn NodeData> {
        self.nodes.get_mut(&node_id).map(|node| node.data.as_mut())
    }

    pub fn set_node_display(&mut self, node_id: u64, is_displayed: bool) {
        if self.nodes.contains_key(&node_id) {
            if is_displayed {
                // Preserve existing pin state if re-displaying, otherwise default
                self.displayed_nodes
                    .entry(node_id)
                    .or_insert_with(NodeDisplayState::normal);
            } else {
                self.displayed_nodes.remove(&node_id);
            }
        }
    }

    /// Sets a node to be displayed with the specified display type, or hides it if display_type is None
    pub fn set_node_display_type(&mut self, node_id: u64, display_type: Option<NodeDisplayType>) {
        if self.nodes.contains_key(&node_id) {
            match display_type {
                Some(dt) => {
                    self.displayed_nodes
                        .entry(node_id)
                        .and_modify(|s| s.display_type = dt)
                        .or_insert_with(|| NodeDisplayState::with_type(dt));
                }
                None => {
                    self.displayed_nodes.remove(&node_id);
                }
            }
        }
    }

    /// Check if a node is currently displayed
    pub fn is_node_displayed(&self, node_id: u64) -> bool {
        self.displayed_nodes.contains_key(&node_id)
    }

    /// Get the display type of a node if it is displayed
    pub fn get_node_display_type(&self, node_id: u64) -> Option<NodeDisplayType> {
        self.displayed_nodes.get(&node_id).map(|s| s.display_type)
    }

    /// Get the set of displayed output pins for a node.
    /// Returns None if the node is not displayed.
    pub fn get_displayed_pins(&self, node_id: u64) -> Option<&HashSet<i32>> {
        self.displayed_nodes
            .get(&node_id)
            .map(|s| &s.displayed_pins)
    }

    /// Toggle a specific output pin's display state for an already-displayed node.
    /// If the last pin is removed, the node is auto-removed from `displayed_nodes`.
    pub fn set_pin_displayed(&mut self, node_id: u64, pin_index: i32, displayed: bool) {
        if displayed {
            // Add pin — create the display entry if the node isn't currently displayed
            if self.nodes.contains_key(&node_id) {
                let state =
                    self.displayed_nodes
                        .entry(node_id)
                        .or_insert_with(|| NodeDisplayState {
                            display_type: NodeDisplayType::Normal,
                            displayed_pins: HashSet::new(),
                        });
                state.displayed_pins.insert(pin_index);
            }
        } else if let Some(state) = self.displayed_nodes.get_mut(&node_id) {
            state.displayed_pins.remove(&pin_index);
            if state.displayed_pins.is_empty() {
                self.displayed_nodes.remove(&node_id);
            }
        }
    }

    /// Get the full display state for a node.
    /// Returns None if the node is not displayed.
    pub fn get_node_display_state(&self, node_id: u64) -> Option<&NodeDisplayState> {
        self.displayed_nodes.get(&node_id)
    }

    // ===== NODE SELECTION =====

    /// Select a single node (clears existing selection including wires)
    /// Returns true if the node exists and was selected, false otherwise.
    pub fn select_node(&mut self, node_id: u64) -> bool {
        if self.nodes.contains_key(&node_id) {
            self.selected_wires.clear();
            self.selected_node_ids.clear();
            self.selected_node_ids.insert(node_id);
            self.active_node_id = Some(node_id);
            true
        } else {
            false
        }
    }

    /// Toggle node in selection (for Ctrl+click)
    /// Returns true if the node exists, false otherwise.
    /// Does not clear wire selection to allow mixed node+wire selections.
    pub fn toggle_node_selection(&mut self, node_id: u64) -> bool {
        if !self.nodes.contains_key(&node_id) {
            return false;
        }
        if self.selected_node_ids.contains(&node_id) {
            self.selected_node_ids.remove(&node_id);
            // Update active node if we removed it
            if self.active_node_id == Some(node_id) {
                self.active_node_id = self.selected_node_ids.iter().next().copied();
            }
        } else {
            self.selected_node_ids.insert(node_id);
            self.active_node_id = Some(node_id);
        }
        true
    }

    /// Add node to selection (for Shift+click)
    /// Returns true if the node exists, false otherwise.
    /// Does not clear wire selection to allow mixed node+wire selections.
    pub fn add_node_to_selection(&mut self, node_id: u64) -> bool {
        if !self.nodes.contains_key(&node_id) {
            return false;
        }
        self.selected_node_ids.insert(node_id);
        self.active_node_id = Some(node_id);
        true
    }

    /// Select multiple nodes (for rectangle selection)
    /// Returns true if at least one node was selected, false otherwise.
    pub fn select_nodes(&mut self, node_ids: Vec<u64>) -> bool {
        self.selected_wires.clear();
        self.selected_node_ids.clear();
        for id in &node_ids {
            if self.nodes.contains_key(id) {
                self.selected_node_ids.insert(*id);
            }
        }
        // Set active to last node in list (or none if empty)
        self.active_node_id = node_ids
            .last()
            .copied()
            .filter(|id| self.selected_node_ids.contains(id));
        !self.selected_node_ids.is_empty()
    }

    /// Toggle multiple nodes in selection (for Ctrl+rectangle)
    pub fn toggle_nodes_selection(&mut self, node_ids: Vec<u64>) {
        self.selected_wires.clear();
        for id in node_ids {
            if self.nodes.contains_key(&id) {
                if self.selected_node_ids.contains(&id) {
                    self.selected_node_ids.remove(&id);
                } else {
                    self.selected_node_ids.insert(id);
                    self.active_node_id = Some(id);
                }
            }
        }
        // Update active node if removed
        if let Some(active) = self.active_node_id {
            if !self.selected_node_ids.contains(&active) {
                self.active_node_id = self.selected_node_ids.iter().next().copied();
            }
        }
    }

    /// Add multiple nodes to selection (for Shift+rectangle)
    pub fn add_nodes_to_selection(&mut self, node_ids: Vec<u64>) {
        self.selected_wires.clear();
        for id in &node_ids {
            if self.nodes.contains_key(id) {
                self.selected_node_ids.insert(*id);
            }
        }
        // Set active to last node in list (if valid)
        if let Some(last_id) = node_ids.last() {
            if self.selected_node_ids.contains(last_id) {
                self.active_node_id = Some(*last_id);
            }
        }
    }

    /// Check if a node is selected
    pub fn is_node_selected(&self, node_id: u64) -> bool {
        self.selected_node_ids.contains(&node_id)
    }

    /// Check if a node is the active node
    pub fn is_node_active(&self, node_id: u64) -> bool {
        self.active_node_id == Some(node_id)
    }

    /// Get all selected node IDs
    pub fn get_selected_node_ids(&self) -> &HashSet<u64> {
        &self.selected_node_ids
    }

    // ===== WIRE SELECTION =====

    /// Select a single wire (clears existing selection including nodes)
    /// Returns true if both nodes exist and the wire was selected, false otherwise.
    pub fn select_wire(
        &mut self,
        source_node_id: u64,
        source_output_pin_index: i32,
        destination_node_id: u64,
        destination_argument_index: usize,
    ) -> bool {
        if self.nodes.contains_key(&source_node_id) && self.nodes.contains_key(&destination_node_id)
        {
            self.selected_node_ids.clear();
            self.active_node_id = None;
            self.selected_wires.clear();
            self.selected_wires.push(Wire::node_output(
                source_node_id,
                source_output_pin_index,
                destination_node_id,
                destination_argument_index,
            ));
            true
        } else {
            false
        }
    }

    /// Toggle wire in selection (for Ctrl+click)
    /// Returns true if both nodes exist, false otherwise.
    /// Does not clear node selection to allow mixed node+wire selections.
    pub fn toggle_wire_selection(
        &mut self,
        source_node_id: u64,
        source_output_pin_index: i32,
        destination_node_id: u64,
        destination_argument_index: usize,
    ) -> bool {
        if !self.nodes.contains_key(&source_node_id)
            || !self.nodes.contains_key(&destination_node_id)
        {
            return false;
        }

        let wire = Wire::node_output(
            source_node_id,
            source_output_pin_index,
            destination_node_id,
            destination_argument_index,
        );

        // Check if wire already selected
        if let Some(idx) = self.selected_wires.iter().position(|w| *w == wire) {
            self.selected_wires.remove(idx);
        } else {
            self.selected_wires.push(wire);
        }
        true
    }

    /// Add wire to selection (for Shift+click)
    /// Returns true if both nodes exist, false otherwise.
    /// Does not clear node selection to allow mixed node+wire selections.
    pub fn add_wire_to_selection(
        &mut self,
        source_node_id: u64,
        source_output_pin_index: i32,
        destination_node_id: u64,
        destination_argument_index: usize,
    ) -> bool {
        if !self.nodes.contains_key(&source_node_id)
            || !self.nodes.contains_key(&destination_node_id)
        {
            return false;
        }

        let wire = Wire::node_output(
            source_node_id,
            source_output_pin_index,
            destination_node_id,
            destination_argument_index,
        );

        // Only add if not already selected
        if !self.selected_wires.contains(&wire) {
            self.selected_wires.push(wire);
        }
        true
    }

    /// Check if a wire is selected
    pub fn is_wire_selected(
        &self,
        source_node_id: u64,
        source_output_pin_index: i32,
        destination_node_id: u64,
        destination_argument_index: usize,
    ) -> bool {
        let wire = Wire::node_output(
            source_node_id,
            source_output_pin_index,
            destination_node_id,
            destination_argument_index,
        );
        self.selected_wires.contains(&wire)
    }

    /// Get all selected wires
    pub fn get_selected_wires(&self) -> &Vec<Wire> {
        &self.selected_wires
    }

    /// Select multiple wires (replaces current selection)
    pub fn select_wires(&mut self, wires: Vec<Wire>) {
        self.selected_node_ids.clear();
        self.active_node_id = None;
        self.selected_wires.clear();
        for wire in wires {
            if self.nodes.contains_key(&wire.source_node_id)
                && self.nodes.contains_key(&wire.destination_node_id)
                && !self.selected_wires.contains(&wire)
            {
                self.selected_wires.push(wire);
            }
        }
    }

    /// Add multiple wires to selection (for Shift+rectangle)
    pub fn add_wires_to_selection(&mut self, wires: Vec<Wire>) {
        self.selected_node_ids.clear();
        self.active_node_id = None;
        for wire in wires {
            if self.nodes.contains_key(&wire.source_node_id)
                && self.nodes.contains_key(&wire.destination_node_id)
                && !self.selected_wires.contains(&wire)
            {
                self.selected_wires.push(wire);
            }
        }
    }

    /// Toggle multiple wires in selection (for Ctrl+rectangle)
    pub fn toggle_wires_selection(&mut self, wires: Vec<Wire>) {
        self.selected_node_ids.clear();
        self.active_node_id = None;
        for wire in wires {
            if self.nodes.contains_key(&wire.source_node_id)
                && self.nodes.contains_key(&wire.destination_node_id)
            {
                if let Some(idx) = self.selected_wires.iter().position(|w| *w == wire) {
                    self.selected_wires.remove(idx);
                } else {
                    self.selected_wires.push(wire);
                }
            }
        }
    }

    /// Select nodes and wires together (for rectangle selection)
    /// Clears existing selection and adds both nodes and wires.
    pub fn select_nodes_and_wires(&mut self, node_ids: Vec<u64>, wires: Vec<Wire>) {
        self.selected_node_ids.clear();
        self.selected_wires.clear();
        self.active_node_id = None;

        // Add nodes
        for id in &node_ids {
            if self.nodes.contains_key(id) {
                self.selected_node_ids.insert(*id);
            }
        }
        // Set active to last node in list (if valid)
        if let Some(last_id) = node_ids.last() {
            if self.selected_node_ids.contains(last_id) {
                self.active_node_id = Some(*last_id);
            }
        }

        // Add wires
        for wire in wires {
            if self.nodes.contains_key(&wire.source_node_id)
                && self.nodes.contains_key(&wire.destination_node_id)
                && !self.selected_wires.contains(&wire)
            {
                self.selected_wires.push(wire);
            }
        }
    }

    /// Add nodes and wires to existing selection (for Shift+rectangle)
    pub fn add_nodes_and_wires_to_selection(&mut self, node_ids: Vec<u64>, wires: Vec<Wire>) {
        // Add nodes without clearing existing selection
        for id in &node_ids {
            if self.nodes.contains_key(id) {
                self.selected_node_ids.insert(*id);
            }
        }
        // Set active to last node in list (if valid)
        if let Some(last_id) = node_ids.last() {
            if self.selected_node_ids.contains(last_id) {
                self.active_node_id = Some(*last_id);
            }
        }

        // Add wires without clearing existing selection
        for wire in wires {
            if self.nodes.contains_key(&wire.source_node_id)
                && self.nodes.contains_key(&wire.destination_node_id)
                && !self.selected_wires.contains(&wire)
            {
                self.selected_wires.push(wire);
            }
        }
    }

    /// Toggle nodes and wires in selection (for Ctrl+rectangle)
    pub fn toggle_nodes_and_wires_selection(&mut self, node_ids: Vec<u64>, wires: Vec<Wire>) {
        // Toggle nodes
        for id in node_ids {
            if self.nodes.contains_key(&id) {
                if self.selected_node_ids.contains(&id) {
                    self.selected_node_ids.remove(&id);
                } else {
                    self.selected_node_ids.insert(id);
                    self.active_node_id = Some(id);
                }
            }
        }
        // Update active node if removed
        if let Some(active) = self.active_node_id {
            if !self.selected_node_ids.contains(&active) {
                self.active_node_id = self.selected_node_ids.iter().next().copied();
            }
        }

        // Toggle wires
        for wire in wires {
            if self.nodes.contains_key(&wire.source_node_id)
                && self.nodes.contains_key(&wire.destination_node_id)
            {
                if let Some(idx) = self.selected_wires.iter().position(|w| *w == wire) {
                    self.selected_wires.remove(idx);
                } else {
                    self.selected_wires.push(wire);
                }
            }
        }
    }

    // ===== COMMON SELECTION =====

    /// Clears any existing selection (both nodes and wires).
    pub fn clear_selection(&mut self) {
        self.selected_node_ids.clear();
        self.active_node_id = None;
        self.selected_wires.clear();
    }

    /// Cleans up selection and active node state after nodes have been removed.
    /// Call this after removing nodes to ensure no dangling references remain.
    pub fn cleanup_selection_for_removed_nodes(&mut self, removed_ids: &[u64]) {
        for &id in removed_ids {
            self.selected_node_ids.remove(&id);
            if self.active_node_id == Some(id) {
                self.active_node_id = None;
            }
        }
        // Remove any selected wires that reference removed nodes
        self.selected_wires.retain(|w| {
            !removed_ids.contains(&w.source_node_id)
                && !removed_ids.contains(&w.destination_node_id)
        });
    }

    /// Provides gadget for the active node (used for property panels)
    pub fn provide_gadget(
        &self,
        structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        if let Some(node_id) = self.active_node_id {
            if let Some(node) = self.nodes.get(&node_id) {
                return node.data.provide_gadget(structure_designer);
            }
        }
        None
    }

    /// Returns information about what `delete_selected()` would delete, without mutating anything.
    /// Used by the undo system to capture state before deletion.
    pub fn collect_deletion_info(&self) -> DeletionInfo {
        let mut info = DeletionInfo::default();

        if !self.selected_node_ids.is_empty() {
            info.deleted_node_ids = self.selected_node_ids.iter().copied().collect();

            // Collect all wires connected to selected nodes
            for (&node_id, node) in &self.nodes {
                for (param_index, arg) in node.arguments.iter().enumerate() {
                    for wire in &arg.incoming_wires {
                        // Wire is affected if source or dest is being deleted
                        if self.selected_node_ids.contains(&wire.source_node_id)
                            || self.selected_node_ids.contains(&node_id)
                        {
                            info.deleted_wires.push(Wire {
                                source_node_id: wire.source_node_id,
                                source_pin: wire.source_pin,
                                source_scope_depth: wire.source_scope_depth,
                                destination_node_id: node_id,
                                destination_argument_index: param_index,
                                destination_argument_kind: ArgumentKind::External,
                            });
                        }
                    }
                }
            }

            // Check if return node is among deleted
            if let Some(return_id) = self.return_node_id {
                if self.selected_node_ids.contains(&return_id) {
                    info.was_return_node = Some(return_id);
                }
            }

            // Collect display states of deleted nodes
            for &node_id in &self.selected_node_ids {
                if let Some(state) = self.displayed_nodes.get(&node_id) {
                    info.display_states.push((node_id, state.display_type));
                }
            }

            info.is_node_deletion = true;
        } else if !self.selected_wires.is_empty() {
            info.selected_wires = self.selected_wires.clone();
            info.is_node_deletion = false;
        }

        info
    }

    /// Delete all selected nodes and wires
    pub fn delete_selected(&mut self) {
        // Handle selected nodes (delete all selected)
        if !self.selected_node_ids.is_empty() {
            let selected_ids: Vec<u64> = self.selected_node_ids.iter().cloned().collect();

            for node_id in selected_ids {
                // First remove any references to this node from all other nodes' arguments
                let nodes_to_process: Vec<u64> = self.nodes.keys().cloned().collect();
                for other_node_id in nodes_to_process {
                    if let Some(node) = self.nodes.get_mut(&other_node_id) {
                        for argument in node.arguments.iter_mut() {
                            argument.remove_source(node_id);
                        }
                    }
                }

                // If this was the return node, clear that reference
                if self.return_node_id == Some(node_id) {
                    self.return_node_id = None;
                }

                // Remove from displayed nodes if present
                self.displayed_nodes.remove(&node_id);

                // Remove the node itself
                self.nodes.remove(&node_id);
            }

            self.selected_node_ids.clear();
            self.active_node_id = None;
        }
        // Handle selected wires (delete all selected)
        else if !self.selected_wires.is_empty() {
            let wires_to_delete: Vec<Wire> = self.selected_wires.drain(..).collect();

            for wire in wires_to_delete {
                if let Some(dest_node) = self.nodes.get_mut(&wire.destination_node_id) {
                    if let Some(argument) =
                        dest_node.arguments.get_mut(wire.destination_argument_index)
                    {
                        argument.remove_source(wire.source_node_id);
                    }
                }
            }
        }
    }

    /// Move all selected nodes by delta
    pub fn move_selected_nodes(&mut self, delta: DVec2) {
        for &node_id in &self.selected_node_ids.clone() {
            if let Some(node) = self.nodes.get_mut(&node_id) {
                node.position += delta;
            }
        }
    }

    /// Sets a node as the return node for this network
    ///
    /// # Parameters
    /// * `node_id` - The ID of the node to set as the return node
    ///
    /// # Returns
    /// Returns true if the node exists and was set as the return node, false otherwise.
    pub fn set_return_node(&mut self, node_id: u64) -> bool {
        if self.nodes.contains_key(&node_id) {
            // Set this node as the return node
            self.return_node_id = Some(node_id);

            true
        } else {
            false
        }
    }

    /// Duplicates a node with all its data and arguments
    ///
    /// # Parameters
    /// * `node_id` - The ID of the node to duplicate
    ///
    /// # Returns
    /// Returns Some(new_node_id) if the node was successfully duplicated, None if the node doesn't exist.
    pub fn duplicate_node(&mut self, node_id: u64) -> Option<u64> {
        // Check if the node exists
        let original_node = self.nodes.get(&node_id)?;

        // Generate new node ID
        let new_node_id = self.next_node_id;
        self.next_node_id += 1;

        // Clone the node data using the clone_box method
        let cloned_data = original_node.data.clone_box();

        // Clone the arguments (connections)
        let cloned_arguments = original_node.arguments.clone();

        // Clone the node type name for display name generation
        let node_type_name = original_node.node_type_name.clone();

        // Use node_layout module for consistent size estimation across the codebase.
        // The subtitle parameter is set to true as most nodes display a subtitle.
        let vert_offset = node_layout::duplicate_node_vertical_offset(
            max(cloned_arguments.len(), 1),
            1,    // num_output_pins - conservative default
            true, // has_subtitle - assume yes for conservative spacing
        );
        let new_position = DVec2::new(
            original_node.position.x,
            original_node.position.y + vert_offset,
        );

        // Clone the custom node type
        let custom_node_type = original_node.custom_node_type.clone();

        // Carry the zone body and zone-output wires through duplication. The
        // body shares storage via `Arc::clone`; mutating either copy CoW-
        // forks lazily through `zone_mut()` / `Arc::make_mut`.
        let cloned_zone = original_node.zone.clone();
        let cloned_zone_output_arguments = original_node.zone_output_arguments.clone();

        // Generate a unique display name for the duplicated node
        let display_name = self.generate_unique_display_name(&node_type_name);

        // Create the duplicated node
        let duplicated_node = Node {
            id: new_node_id,
            node_type_name,
            custom_name: Some(display_name),
            position: new_position,
            arguments: cloned_arguments,
            data: cloned_data,
            custom_node_type,
            zone: cloned_zone,
            zone_output_arguments: cloned_zone_output_arguments,
            body_width: original_node.body_width,
            body_height: original_node.body_height,
            collapse_mode: original_node.collapse_mode,
        };

        // Insert the duplicated node into the network
        self.nodes.insert(new_node_id, duplicated_node);

        Some(new_node_id)
    }
}

/// Visit every node in `network`, recursing into HOF zone bodies at every
/// depth. The callback is invoked on each node before any descent into its
/// own body, so a body-internal HOF receives `f` after its parent HOF but
/// before its own nested children.
///
/// Use this anywhere a piece of code wants to do "per-node work for every
/// node in the network tree" — populating caches, looking up references,
/// counting things, rewriting names — without manually re-implementing the
/// zone descent at every call site. The recurring class of bug this avoids:
/// iterating `network.nodes.values()` and silently skipping nodes inside an
/// HOF's owned body. See `doc/design_zones.md`.
pub fn walk_all_nodes(network: &NodeNetwork, f: &mut impl FnMut(&Node)) {
    for node in network.nodes.values() {
        f(node);
        if let Some(body) = node.zone.as_deref() {
            walk_all_nodes(body, f);
        }
    }
}

/// Mutable counterpart to [`walk_all_nodes`]. The callback receives a
/// `&mut Node` and the function descends into `node.zone_mut()` after the
/// callback returns, so the borrow is released before recursion. Body
/// access goes through `zone_mut`, which CoW-clones the `Arc<NodeNetwork>`
/// on first mutation.
pub fn walk_all_nodes_mut(network: &mut NodeNetwork, f: &mut impl FnMut(&mut Node)) {
    for node in network.nodes.values_mut() {
        f(node);
        if let Some(body) = node.zone_mut() {
            walk_all_nodes_mut(body, f);
        }
    }
}
