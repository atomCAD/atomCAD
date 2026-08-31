use crate::data_type::DataType;
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network::{ArgumentKind, Node, NodeNetwork, SourcePin, Wire};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{
    NodeType, OutputPinDefinition, generic_node_data_loader, generic_node_data_saver,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// What a comment node documents: a node, or a single wire.
///
/// Anchors are **scope-local** (`doc/design_wire_annotations.md` D5): the
/// target lives in the same network as the comment itself (its own top-level
/// network, or its own HOF/closure body), so no scope path is stored. They are
/// **inert with respect to evaluation** (D7) — an anchor is documentation,
/// never a dependency edge, so it must not make its target count as "used"
/// anywhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommentAnchor {
    /// The comment documents a whole node.
    Node(u64),
    /// The comment documents one wire.
    Wire(WireAnchor),
}

/// Identity of an anchored wire.
///
/// Wires are not stored — they are assembled from the `IncomingWire`s on the
/// **destination** node — so a wire anchor is keyed from the destination side
/// (D4). Four fields suffice: within one argument slot `Argument` keeps at
/// most one wire per source node id (the per-source uniqueness invariant on
/// `Argument::set_source`), so `source_pin` and `source_scope_depth` are
/// properties of the resolved wire rather than part of its identity and are
/// deliberately **not** duplicated here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireAnchor {
    pub destination_node_id: u64,
    /// Which argument list on the destination the wire terminates at.
    #[serde(default)]
    pub destination_argument_kind: ArgumentKind,
    /// Positional slot in that argument list. Reliable for built-in node
    /// types, whose pin layouts are fixed; see `destination_param_id` for the
    /// layouts that can move.
    pub destination_argument_index: usize,
    /// Present iff the destination parameter carries a persistent id — that
    /// is, on the dynamic-arity node types (`expr`, `sequence`, `switch`,
    /// `zip_with`, `product`, `record_construct`) and network parameters,
    /// which are exactly the layouts whose slot indices can shift.
    /// **Authoritative over the index when set.**
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_param_id: Option<u64>,
    pub source_node_id: u64,
}

/// The live target a [`CommentAnchor`] currently designates.
///
/// `resolve` answers *identity*, not geometry: the caller derives coordinates
/// from the node positions it already has. It must never depend on layout
/// having run.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedAnchor {
    Node(u64),
    /// The assembled wire view struct.
    Wire(Wire),
}

impl CommentAnchor {
    /// Resolve this anchor against the network that contains the comment.
    /// `None` means the anchor is **dangling** — its target no longer exists.
    ///
    /// A dangling anchor is dropped, never re-resolved (D6): a leader line
    /// pointing at the wrong wire is documentation that actively lies.
    pub fn resolve(&self, network: &NodeNetwork) -> Option<ResolvedAnchor> {
        match self {
            CommentAnchor::Node(node_id) => network
                .nodes
                .contains_key(node_id)
                .then_some(ResolvedAnchor::Node(*node_id)),
            CommentAnchor::Wire(wire_anchor) => {
                wire_anchor.resolve(network).map(ResolvedAnchor::Wire)
            }
        }
    }
}

impl WireAnchor {
    /// Build an anchor addressing `wire` inside `network`, capturing the
    /// destination parameter's persistent id when it has one.
    pub fn from_wire(wire: &Wire, network: &NodeNetwork) -> Self {
        let destination_param_id = network
            .nodes
            .get(&wire.destination_node_id)
            .and_then(|node| {
                Self::destination_param_ids(node, wire.destination_argument_kind)
                    .and_then(|ids| ids.get(wire.destination_argument_index).copied())
            })
            .flatten();
        Self {
            destination_node_id: wire.destination_node_id,
            destination_argument_kind: wire.destination_argument_kind,
            destination_argument_index: wire.destination_argument_index,
            destination_param_id,
            source_node_id: wire.source_node_id,
        }
    }

    /// Assemble the `Wire` this anchor designates, or `None` if it is dangling
    /// (destination node gone, slot gone, or the wire disconnected).
    pub fn resolve(&self, network: &NodeNetwork) -> Option<Wire> {
        let destination = network.nodes.get(&self.destination_node_id)?;
        let index = self.slot_index(destination)?;
        let arguments = match self.destination_argument_kind {
            ArgumentKind::External => &destination.arguments,
            ArgumentKind::ZoneOutput => &destination.zone_output_arguments,
        };
        let incoming = arguments
            .get(index)?
            .incoming_wires
            .iter()
            .find(|w| w.source_node_id == self.source_node_id)?;

        // A same-scope regular-output source must still be present in this
        // network. Captures (`source_scope_depth >= 1`), zone-input sources,
        // and body-return wires (`ZoneOutput`, whose source lives inside the
        // HOF's body) all resolve against scopes this network alone cannot
        // see, so they are taken on trust.
        if self.destination_argument_kind == ArgumentKind::External
            && incoming.source_scope_depth == 0
            && matches!(incoming.source_pin, SourcePin::NodeOutput { .. })
            && !network.nodes.contains_key(&self.source_node_id)
        {
            return None;
        }

        Some(Wire {
            source_node_id: incoming.source_node_id,
            source_pin: incoming.source_pin,
            source_scope_depth: incoming.source_scope_depth,
            destination_node_id: self.destination_node_id,
            destination_argument_index: index,
            destination_argument_kind: self.destination_argument_kind,
        })
    }

    /// D4's precedence: the persistent parameter id wins when set, so a pin
    /// reorder on a dynamic-arity node **remaps** the anchor instead of
    /// dropping it, while a deleted parameter yields `None` and drops it.
    /// Falls back to the stored index when no id is recorded, or when the
    /// destination carries no custom node type to look ids up in.
    fn slot_index(&self, destination: &Node) -> Option<usize> {
        if let Some(param_id) = self.destination_param_id
            && let Some(ids) =
                Self::destination_param_ids(destination, self.destination_argument_kind)
        {
            return ids.iter().position(|id| *id == Some(param_id));
        }
        Some(self.destination_argument_index)
    }

    /// The destination's per-slot persistent parameter ids, or `None` if the
    /// node has no custom node type (a static built-in layout, which never
    /// carries ids).
    fn destination_param_ids(node: &Node, kind: ArgumentKind) -> Option<Vec<Option<u64>>> {
        let node_type = node.custom_node_type.as_ref()?;
        let params = match kind {
            ArgumentKind::External => &node_type.parameters,
            ArgumentKind::ZoneOutput => &node_type.zone_output_pins,
        };
        Some(params.iter().map(|p| p.id).collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentData {
    pub label: String,
    pub text: String,
    pub width: f64,
    pub height: f64,
    /// What this comment documents. Empty (the default) means a free-floating
    /// note, which is every comment authored before anchors existed —
    /// `#[serde(default)]` keeps those `.cnnd` files loading unchanged and
    /// `skip_serializing_if` keeps their serialized output byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<CommentAnchor>,
}

impl Default for CommentData {
    fn default() -> Self {
        Self {
            label: String::new(),
            text: String::new(),
            width: 200.0,
            height: 100.0,
            anchors: Vec::new(),
        }
    }
}

impl NodeData for CommentData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        _network_evaluator: &NetworkEvaluator,
        _network_stack: &[NetworkStackElement<'a>],
        _node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        EvalOutput::single(NetworkResult::None)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        if self.label.is_empty() {
            None
        } else {
            Some(self.label.clone())
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("label".to_string(), TextValue::String(self.label.clone())),
            ("text".to_string(), TextValue::String(self.text.clone())),
            ("width".to_string(), TextValue::Float(self.width)),
            ("height".to_string(), TextValue::Float(self.height)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("label") {
            self.label = v
                .as_string()
                .ok_or_else(|| "label must be a string".to_string())?
                .to_string();
        }
        if let Some(v) = props.get("text") {
            self.text = v
                .as_string()
                .ok_or_else(|| "text must be a string".to_string())?
                .to_string();
        }
        if let Some(v) = props.get("width") {
            self.width = v
                .as_float()
                .ok_or_else(|| "width must be a float".to_string())?;
        }
        if let Some(v) = props.get("height") {
            self.height = v
                .as_float()
                .ok_or_else(|| "height must be a float".to_string())?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "Comment".to_string(),
        description: "Add text annotations to document your node network.".to_string(),
        summary: None,
        category: NodeTypeCategory::Annotation,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::None),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(CommentData::default()),
        node_data_saver: generic_node_data_saver::<CommentData>,
        node_data_loader: generic_node_data_loader::<CommentData>,
    }
}
