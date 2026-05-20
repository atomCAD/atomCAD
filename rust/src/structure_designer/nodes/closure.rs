//! The `closure` node: a zone-bearing node that, instead of *consuming* its
//! inline body inline (like an HOF), exposes that body as a first-class
//! `Function`-typed value on an output pin.
//!
//! Conceptually a `closure` node is "one HOF's body, detached from its
//! consumer". Its `eval` is the first half of an HOF eval — `build_inline_closure`
//! — wrapped as a `NetworkResult::Function` value rather than fed into a walker.
//! Wherever that value flows (an HOF's `f` pin, an `apply` node, a subnetwork's
//! `Function` output) the body runs through the *same* `run_closure_once`
//! substrate. See `doc/design_closures.md`.
//!
//! The closure's interface is driven by a [`ClosureKind`] (a shape template
//! fixing the arity and which pin types are free vs. fixed) plus the user's
//! free type arguments. The four v1 kinds are exactly the four HOF body shapes,
//! so a closure of a given kind drops into the matching HOF's `f` pin by
//! construction. The same `{ kind, type_args }` data also drives the `apply`
//! node (expanded *outward* there) — see `nodes/apply.rs`.

use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::{DataType, FunctionType};
use crate::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::zone_closure::build_inline_closure;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use serde::{Deserialize, Serialize};

/// A shape template for a function value. Fixes the arity and decides, per pin,
/// whether the type is **free** (the user picks a `DataType`) or **fixed**
/// (the system supplies it). The four v1 kinds are exactly the four HOF body
/// shapes; the carried `type_args` fill the free slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClosureKind {
    /// `(T) -> U` — map-like. `type_args`: `[T, U]`.
    Map,
    /// `(T) -> Bool` — filter-like. `type_args`: `[T]`.
    Filter,
    /// `(A, T) -> A` — fold-like. `type_args`: `[A, T]`.
    Fold,
    /// `(T) -> Unit` — foreach-like. `type_args`: `[T]`.
    Foreach,
}

/// Read `type_args[i]`, defaulting to `DataType::None` when the stored vector
/// is shorter than the kind expects (a transient state during editing).
fn arg(type_args: &[DataType], i: usize) -> DataType {
    type_args.get(i).cloned().unwrap_or(DataType::None)
}

impl ClosureKind {
    /// Number of free type arguments the user supplies for this kind.
    pub fn num_type_args(&self) -> usize {
        match self {
            ClosureKind::Map => 2,     // T, U
            ClosureKind::Filter => 1,  // T
            ClosureKind::Fold => 2,    // A, T
            ClosureKind::Foreach => 1, // T
        }
    }

    /// The parameter types — i.e. the closure's zone-input pin types.
    pub fn param_types(&self, type_args: &[DataType]) -> Vec<DataType> {
        match self {
            ClosureKind::Map | ClosureKind::Filter | ClosureKind::Foreach => {
                vec![arg(type_args, 0)]
            }
            ClosureKind::Fold => vec![arg(type_args, 0), arg(type_args, 1)],
        }
    }

    /// The return type — i.e. the closure's single zone-output pin type.
    pub fn return_type(&self, type_args: &[DataType]) -> DataType {
        match self {
            ClosureKind::Map => arg(type_args, 1), // free U
            ClosureKind::Filter => DataType::Bool, // fixed
            ClosureKind::Fold => arg(type_args, 0), // derived = A
            ClosureKind::Foreach => DataType::Unit, // fixed
        }
    }

    /// Names for the parameter (zone-input) pins, mirroring the matching HOF so
    /// authored bodies read naturally.
    pub fn param_names(&self) -> &'static [&'static str] {
        match self {
            ClosureKind::Fold => &["acc", "element"],
            _ => &["element"],
        }
    }

    /// Name for the single result (zone-output) pin, mirroring the matching HOF.
    pub fn result_name(&self) -> &'static str {
        match self {
            ClosureKind::Fold => "new_acc",
            ClosureKind::Foreach => "out",
            _ => "result",
        }
    }

    /// The `DataType::Function` a value of this shape carries.
    pub fn function_type(&self, type_args: &[DataType]) -> FunctionType {
        FunctionType {
            parameter_types: self.param_types(type_args),
            output_type: Box::new(self.return_type(type_args)),
        }
    }
}

/// Stored state for a `closure` node: the shape template plus the free type
/// arguments that fill it. Identical in shape to `ApplyData` — the same data
/// drives both nodes, expanded *inward* (zone pins + `Function` output) here
/// and *outward* (a `Function` input + per-param arg pins) in `apply`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosureData {
    pub kind: ClosureKind,
    pub type_args: Vec<DataType>,
}

impl Default for ClosureData {
    fn default() -> Self {
        // Default to the map-like `(T) -> U` shape with `Float` slots.
        Self {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Float, DataType::Float],
        }
    }
}

impl NodeData for ClosureData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();

        let params = self.kind.param_types(&self.type_args);
        let ret = self.kind.return_type(&self.type_args);
        let param_names = self.kind.param_names();

        // External: no input pins — captures arrive as ordinary capture wires
        // drawn into the body. One output pin: the function value itself.
        custom.parameters = vec![];
        custom.output_pins = OutputPinDefinition::single_fixed(DataType::Function(FunctionType {
            parameter_types: params.clone(),
            output_type: Box::new(ret.clone()),
        }));

        // Inside-facing zone pins: one source per parameter, one destination
        // for the result.
        custom.zone_input_pins = params
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let name = param_names.get(i).copied().unwrap_or("element");
                OutputPinDefinition::fixed(name, t.clone())
            })
            .collect();
        custom.zone_output_pins = vec![Parameter {
            id: None,
            name: self.kind.result_name().to_string(),
            data_type: ret,
        }];

        Some(custom)
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        // The first half of an HOF eval — grab the body, freeze captures once
        // at this definition site — wrapped as a value instead of fed into a
        // walker. Capture-freeze timing follows from *this* node's eval running
        // once per evaluation: a `closure` outside a `fold` freezes once and is
        // shared across iterations; a `closure` inside a `fold` body re-freezes
        // per outer iteration. See `doc/design_closures.md`.
        let closure = match build_inline_closure(
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
            "closure",
        ) {
            Ok(c) => c,
            Err(e) => return EvalOutput::single(e),
        };
        EvalOutput::single(NetworkResult::Function(closure))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        None
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "closure".to_string(),
        description: "Exposes its inline zone body as a first-class `Function` value rather than consuming it inline. The body reads its parameters from the inside-facing zone-input pins and delivers its result to the inside-facing zone-output pin; the resulting function value can be wired into an HOF's `f` pin (reuse across call sites), called once by an `apply` node, or returned as a subnetwork's `Function` output (a function factory). Captures are frozen once, when this node is evaluated.".to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        // External interface is filled in by `calculate_custom_node_type`; the
        // default is the map-like `(Float) -> Float` shape.
        parameters: vec![],
        output_pins: OutputPinDefinition::single_fixed(DataType::Function(FunctionType {
            parameter_types: vec![DataType::Float],
            output_type: Box::new(DataType::Float),
        })),
        zone_input_pins: vec![OutputPinDefinition::fixed("element", DataType::Float)],
        zone_output_pins: vec![Parameter {
            id: None,
            name: "result".to_string(),
            data_type: DataType::Float,
        }],
        public: true,
        node_data_creator: || Box::new(ClosureData::default()),
        node_data_saver: generic_node_data_saver::<ClosureData>,
        node_data_loader: generic_node_data_loader::<ClosureData>,
    }
}
