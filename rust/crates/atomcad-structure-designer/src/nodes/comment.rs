use crate::data_type::DataType;
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network::{NodeNetwork, Wire, WireAnchor};
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

/// The text-format property name a comment's anchors round-trip through.
///
/// Anchors are network-level state, not a `NodeData` property: writing one
/// needs node *names*, and `set_text_properties` only ever sees a
/// `HashMap<String, TextValue>`. So `on` follows the `visible` precedent —
/// emitted by the serializer from resolved anchors, and intercepted by the
/// network editor before the literal-property pass.
pub const ANCHOR_PROPERTY: &str = "on";

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

    /// Re-derive this anchor's stored form from the target it currently
    /// designates, or `None` if it is dangling.
    ///
    /// The difference from [`resolve`](Self::resolve) is that the result is a
    /// `CommentAnchor` again, rebuilt through [`WireAnchor::from_wire`] — so a
    /// wire anchor written by a caller that knows only the slot *index*
    /// (the Flutter canvas, which never sees persistent parameter ids) comes
    /// back carrying `destination_param_id` when the destination has one, and
    /// an anchor whose slot moved comes back addressing the slot it moved to.
    /// Callers that store or publish anchors should canonicalize first,
    /// otherwise D4's remap-on-reorder precedence is silently unavailable.
    pub fn canonicalized(&self, network: &NodeNetwork) -> Option<Self> {
        match self.resolve(network)? {
            ResolvedAnchor::Node(node_id) => Some(CommentAnchor::Node(node_id)),
            ResolvedAnchor::Wire(wire) => {
                Some(CommentAnchor::Wire(WireAnchor::from_wire(&wire, network)))
            }
        }
    }

    /// Rewrite this anchor's node ids through a copy/paste/duplicate
    /// `old_to_new` map, or `None` if a referenced node was **not** part of the
    /// copied set.
    ///
    /// Dropping in that case is the point: pasting a lone comment must not
    /// leave it pointing at whatever unrelated node in the destination network
    /// happens to share the original's id. A wire anchor needs *both* of its
    /// endpoints copied — the destination alone is not enough, since the wire
    /// itself is dropped when its source was left behind.
    pub fn remap(&self, old_to_new: &HashMap<u64, u64>) -> Option<Self> {
        match self {
            CommentAnchor::Node(node_id) => {
                old_to_new.get(node_id).copied().map(CommentAnchor::Node)
            }
            CommentAnchor::Wire(wire_anchor) => Some(CommentAnchor::Wire(WireAnchor {
                destination_node_id: *old_to_new.get(&wire_anchor.destination_node_id)?,
                source_node_id: *old_to_new.get(&wire_anchor.source_node_id)?,
                ..wire_anchor.clone()
            })),
        }
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

/// Drop every anchor in `network` whose target no longer resolves
/// (`doc/design_wire_annotations.md` D6), and report what was lost.
///
/// The return value is one `(node_id, before, after)` entry per comment whose
/// anchor list actually changed — exactly what an undo command needs. Callers
/// that only want the cleanup (`repair_node_network`) ignore it.
///
/// Non-recursive: it repairs the comments of `network` itself. Body comments
/// are reached by the caller recursing into zones (`repair_node_network` does),
/// or ride a whole-body snapshot (the zone-scoped delete path does).
pub type AnchorChange = (u64, Vec<CommentAnchor>, Vec<CommentAnchor>);

pub fn drop_dangling_anchors(network: &mut NodeNetwork) -> Vec<AnchorChange> {
    // Resolution needs an immutable view of the whole network while the comment
    // being fixed lives inside it, so compute first, apply after.
    let mut changed: Vec<(u64, Vec<CommentAnchor>, Vec<CommentAnchor>)> = Vec::new();
    for (&node_id, node) in &network.nodes {
        let Some(comment) = node.data.as_any_ref().downcast_ref::<CommentData>() else {
            continue;
        };
        if comment.anchors.is_empty() {
            continue;
        }
        let kept: Vec<CommentAnchor> = comment
            .anchors
            .iter()
            .filter(|anchor| anchor.resolve(network).is_some())
            .cloned()
            .collect();
        if kept.len() != comment.anchors.len() {
            changed.push((node_id, comment.anchors.clone(), kept));
        }
    }

    let mut applied = Vec::with_capacity(changed.len());
    for (node_id, before, kept) in changed {
        if let Some(node) = network.nodes.get_mut(&node_id)
            && let Some(comment) = node.data.as_any_mut().downcast_mut::<CommentData>()
        {
            comment.anchors = kept.clone();
            applied.push((node_id, before, kept));
        }
    }
    applied
}
