//! The pure splice planner (`doc/design_insert_node_on_wire.md`): which
//! `(input, output pin)` pairs — *arrangements* — a node offers between a
//! value arriving and a type it must produce.
//!
//! Every plan of that design asks this one question. A wire dragged off an
//! output and dropped in empty space has only the arriving side, one dragged
//! off an input only the produced side, and an insertion into a wire both.
//! Validity is decided **per pair** (D2): an output typed `SameAsInput` may fit
//! the destination only when the wire enters one particular input.
//!
//! The planner never sees the user's network. It builds a two-node scratch
//! network — the planned node and a typed stub standing in for the source —
//! wires the stub into each candidate input, and resolves the outputs there.
//! The caller resolves the live types; the stub carries them, so a capture or
//! a zone-input source needs no ancestor chain here.

use crate::data_type::DataType;
use crate::declared_inputs::{NameSource, next_declared_input_name};
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::NodeData;
use crate::node_network::{Node, NodeNetwork, SourcePin};
use crate::node_type_registry::NodeTypeRegistry;
use crate::nodes::value::add_typed_value_node;
use crate::structure_designer::{DragSource, create_node_data};
use glam::f64::DVec2;
use std::collections::HashSet;

/// The value arriving at the planned node: its resolved type, and what an
/// open port would name a new input after.
pub struct SpliceSource<'a> {
    pub data_type: DataType,
    /// What an open port names a new input after (`declared_inputs::NameSource`).
    pub name: NameSource<'a>,
}

/// Which input of the planned node takes the arriving value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputChoice {
    /// An existing input pin, by index.
    Pin(usize),
    /// The open port: a new declared input, named by
    /// `next_declared_input_name` (D9).
    OpenPort,
}

/// `name` is the pin's name; for `OpenPort`, the name the new input will get.
/// `data_type` is the pin's declared type; for `OpenPort`, the source's type,
/// which the new input is declared with.
#[derive(Clone, Debug, PartialEq)]
pub struct ArrangementInput {
    pub choice: InputChoice,
    pub name: String,
    pub data_type: DataType,
}

/// An output pin of the planned node and its resolved type. `pin ≥ 0`: the
/// function pin is never offered (D4).
#[derive(Clone, Debug, PartialEq)]
pub struct ArrangementOutput {
    pub pin: i32,
    pub name: String,
    pub data_type: DataType,
}

/// One valid pair. A side is `None` exactly when the plan had nothing on
/// that side (a wire drop); an insertion's arrangements have both.
#[derive(Clone, Debug, PartialEq)]
pub struct Arrangement {
    pub input: Option<ArrangementInput>,
    pub output: Option<ArrangementOutput>,
}

/// The node being planned for: a new node of a type, or an existing one.
pub struct PlannedNode {
    pub type_name: String,
    pub data: Box<dyn NodeData>,
}

impl PlannedNode {
    /// Default data, adapted to the drag ([`create_node_data`]). `None` for
    /// an unknown type. An insertion passes the source type `FromOutput`; a
    /// wire drop passes the dragged pin's type and direction.
    pub fn new_of_type(
        registry: &NodeTypeRegistry,
        type_name: &str,
        drag: Option<&DragSource>,
    ) -> Option<PlannedNode> {
        Some(PlannedNode {
            type_name: type_name.to_string(),
            data: create_node_data(type_name, drag, registry)?,
        })
    }

    /// A clone of an existing node's data, not adapted: the user has already
    /// configured it.
    pub fn existing(node: &Node) -> PlannedNode {
        PlannedNode {
            type_name: node.node_type_name.clone(),
            data: node.data.clone_box(),
        }
    }
}

/// The valid arrangements of `node` between `source` (the value arriving, if
/// any) and `target` (the type its output must convert to, if any), in the
/// order a dialog shows them: by input (pins in pin order, then the open
/// port), then by output pin.
///
/// The inputs tried are the pins the source converts to. The open port is
/// tried only when no pin takes the source: it is the fallback that declares
/// an input when none fits, as the open-port drop is. (A fresh `expr` comes
/// out of [`PlannedNode::new_of_type`] already adapted to an identity over
/// `x`, so offering the open port beside that pin would put a second,
/// equivalent row in front of every `expr` drop.) With `pass_through`, an
/// input declared through the open port is made the node's identity
/// (`DeclaredInputs::make_pass_through`, D9).
///
/// Without a source the node is planned once, with nothing wired; without a
/// target every input yields one arrangement whose output side is `None`.
pub fn plan_arrangements(
    registry: &NodeTypeRegistry,
    node: &PlannedNode,
    source: Option<&SpliceSource>,
    target: Option<&DataType>,
    pass_through: bool,
) -> Vec<Arrangement> {
    let Some(source) = source else {
        let Some(scratch) = Scratch::new(registry, node, None) else {
            return Vec::new();
        };
        return scratch.arrangements(registry, None, target);
    };

    let mut arrangements = Vec::new();
    for choice in input_choices(registry, node, source) {
        let Some(mut scratch) = Scratch::new(registry, node, Some(&source.data_type)) else {
            return Vec::new();
        };
        let Some(input) = scratch.connect(registry, &choice, source, pass_through) else {
            continue;
        };
        arrangements.extend(scratch.arrangements(registry, Some(input), target));
    }
    arrangements
}

/// The inputs of `node` the source may enter: every pin it converts to, else
/// the open port when the node's declared inputs accept it.
fn input_choices(
    registry: &NodeTypeRegistry,
    node: &PlannedNode,
    source: &SpliceSource,
) -> Vec<InputChoice> {
    let Some(scratch) = Scratch::new(registry, node, None) else {
        return Vec::new();
    };
    let Some(node_type) = registry.get_node_type_for_node(scratch.node()) else {
        return Vec::new();
    };
    let pins: Vec<InputChoice> = node_type
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, parameter)| {
            DataType::can_be_converted_to(&source.data_type, &parameter.data_type, registry)
        })
        .map(|(index, _)| InputChoice::Pin(index))
        .collect();
    if !pins.is_empty() {
        return pins;
    }
    let open_port = node
        .data
        .declared_inputs()
        .is_some_and(|declared| declared.accepts_type(&source.data_type).is_ok());
    if open_port {
        vec![InputChoice::OpenPort]
    } else {
        Vec::new()
    }
}

/// The two-node scratch network: the planned node and, when there is a
/// source, a stub `value` node declared as the source type. The stub's value
/// is irrelevant; only its declared output type is ever read.
struct Scratch {
    network: NodeNetwork,
    node_id: u64,
    stub_id: Option<u64>,
}

impl Scratch {
    fn new(
        registry: &NodeTypeRegistry,
        node: &PlannedNode,
        source_type: Option<&DataType>,
    ) -> Option<Scratch> {
        let num_parameters = registry.get_node_type(&node.type_name)?.parameters.len();
        let mut network = NodeNetwork::new_empty();
        let node_id = network.add_node(
            &node.type_name,
            DVec2::ZERO,
            num_parameters,
            node.data.clone_box(),
        );
        registry.populate_custom_node_type_cache(network.nodes.get_mut(&node_id)?, true);
        let stub_id = source_type.map(|data_type| {
            add_typed_value_node(
                &mut network,
                DVec2::ZERO,
                NetworkResult::None,
                data_type.clone(),
            )
        });
        Some(Scratch {
            network,
            node_id,
            stub_id,
        })
    }

    fn node(&self) -> &Node {
        &self.network.nodes[&self.node_id]
    }

    /// Wire the stub into `choice`, declaring the input first for the open
    /// port. `None` when the choice turns out not to exist.
    fn connect(
        &mut self,
        registry: &NodeTypeRegistry,
        choice: &InputChoice,
        source: &SpliceSource,
        pass_through: bool,
    ) -> Option<ArrangementInput> {
        let stub_id = self.stub_id?;
        let (index, name) = match choice {
            InputChoice::Pin(index) => {
                let node_type = registry.get_node_type_for_node(self.node())?;
                (*index, node_type.parameters.get(*index)?.name.clone())
            }
            InputChoice::OpenPort => {
                let taken: HashSet<String> = registry
                    .get_node_type_for_node(self.node())?
                    .parameters
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                let node = self.network.nodes.get_mut(&self.node_id)?;
                let declared = node.data.declared_inputs_mut()?;
                let name = next_declared_input_name(&*declared, Some(&source.name), &taken);
                let pin_id = declared
                    .append(name.clone(), source.data_type.clone())
                    .ok()?;
                if pass_through {
                    declared.make_pass_through(pin_id);
                }
                registry.populate_custom_node_type_cache(node, true);
                let index = registry
                    .get_node_type_for_node(self.node())?
                    .parameters
                    .iter()
                    .position(|p| p.id == Some(pin_id))?;
                (index, name)
            }
        };
        let data_type = registry
            .get_node_type_for_node(self.node())?
            .parameters
            .get(index)?
            .data_type
            .clone();
        let is_multi = data_type.is_array();
        self.network.connect_wire(
            stub_id,
            SourcePin::NodeOutput { pin_index: 0 },
            0,
            self.node_id,
            index,
            is_multi,
        );
        // The derived layouts the validator installs, so that an `apply` or a
        // `map` takes its pins from a function-typed source.
        registry.update_apply_pin_layouts_for_network_preserving_args(&mut self.network);
        registry.update_map_pin_layouts_for_network_preserving_args(&mut self.network);
        registry.update_zip_with_pin_layouts_for_network_preserving_args(&mut self.network);
        Some(ArrangementInput {
            choice: choice.clone(),
            name,
            data_type,
        })
    }

    /// The arrangements with `input` on the input side: one per output pin
    /// whose resolved type converts to `target`, or one with no output side
    /// when there is no target.
    fn arrangements(
        &self,
        registry: &NodeTypeRegistry,
        input: Option<ArrangementInput>,
        target: Option<&DataType>,
    ) -> Vec<Arrangement> {
        let Some(target) = target else {
            return vec![Arrangement {
                input,
                output: None,
            }];
        };
        let node = self.node();
        let Some(node_type) = registry.get_node_type_for_node(node) else {
            return Vec::new();
        };
        node_type
            .output_pins
            .iter()
            .enumerate()
            .filter_map(|(pin, definition)| {
                let pin = pin as i32;
                let data_type = registry.resolve_output_type(node, &self.network, pin)?;
                DataType::can_be_converted_to(&data_type, target, registry).then(|| Arrangement {
                    input: input.clone(),
                    output: Some(ArrangementOutput {
                        pin,
                        name: definition.name.clone(),
                        data_type,
                    }),
                })
            })
            .collect()
    }
}

/// What kind of source a spliced wire has, for the depth convention of
/// [`insertion_scope`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// A node's output pin, visible from the node's own scope down.
    NodeOutput,
    /// An HOF's zone-input pin (`element`, `acc`), visible only inside the
    /// HOF's body.
    ZoneInput,
}

/// Where a node spliced into a wire lives, and the depths of its two new
/// wires.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsertionScope {
    /// `S`: the scope the new node is added to.
    pub scope: Vec<u64>,
    /// The depth of the wire from the old source into the new node (stored
    /// in `S`).
    pub depth_in: u8,
    /// The depth of the wire from the new node into the old wire's slot,
    /// counted from the slot's effective scope `dst`.
    pub depth_out: u8,
}

/// D6 of `doc/design_insert_node_on_wire.md`: the scope a node spliced into a
/// wire lives in, clamped to lie between where the source is visible and the
/// destination's effective scope, and as close to `clicked` as that allows.
///
/// `frame` is the scope the wire's depth counts up to: where a `NodeOutput`
/// source lives, or the network containing the HOF of a `ZoneInput` source.
/// `dst` is the destination's effective scope (`dest scope ++ [dest id]` for a
/// body-return wire). The source is visible from `src` down — `frame` for a
/// node output, the HOF's body (`dst[..len(frame) + 1]`) for a zone input —
/// and every valid scope `S` satisfies `src ⊑ S ⊑ dst`:
///
/// - `S` is the longest prefix of `clicked` that is also a prefix of `dst`;
/// - if that is shorter than `src`, `S = src`.
///
/// Then `depth_in = len(S) − len(frame)` (never 0 for a zone input) and
/// `depth_out = len(dst) − len(S)`. `None` when the arguments describe no
/// wire: `frame` not a prefix of `dst`, or a zone input whose `dst` does not
/// reach into the HOF's body.
pub fn insertion_scope(
    kind: SourceKind,
    frame: &[u64],
    dst: &[u64],
    clicked: &[u64],
) -> Option<InsertionScope> {
    if !dst.starts_with(frame) {
        return None;
    }
    let src_len = match kind {
        SourceKind::NodeOutput => frame.len(),
        SourceKind::ZoneInput => frame.len() + 1,
    };
    if src_len > dst.len() {
        return None;
    }
    let common = dst.iter().zip(clicked).take_while(|(a, b)| a == b).count();
    let scope_len = common.max(src_len);
    Some(InsertionScope {
        scope: dst[..scope_len].to_vec(),
        depth_in: u8::try_from(scope_len - frame.len()).ok()?,
        depth_out: u8::try_from(dst.len() - scope_len).ok()?,
    })
}
