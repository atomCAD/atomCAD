//! The `apply` node: the minimal consumer of a function value — it calls one
//! `Function` *once*, on a single argument set, and returns the result.
//!
//! Where the four HOFs run a function *across a stream*, `apply` runs it once.
//! That single-value application is what makes a `Function` a genuinely
//! callable value rather than only fuel for iteration. `apply`'s `eval` is the
//! degenerate one-element, no-iterator case of an eager HOF drain loop: obtain
//! the `ZoneClosure` from the (required) `f` pin and run it once via
//! `run_closure_once`. It owns no inline body and so never falls back to one.
//!
//! `apply` shares the `{ kind, type_args }` data model with the `closure` node
//! (`nodes/closure.rs`), but expands it *outward*: a `Function(…)` input pin
//! `f`, one ordinary arg pin per parameter, and a value output of the return
//! type. See `doc/design_closures.md`.

use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::{DataType, FunctionType};
use crate::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::zone_closure::run_closure_once;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::nodes::closure::ClosureKind;
use crate::structure_designer::structure_designer::StructureDesigner;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stored state for an `apply` node. Identical in shape to `ClosureData`: the
/// shape template (which fixes arity and which slots are free) plus the free
/// type arguments and authored parameter names (used only by the `Custom`
/// kind). See `nodes/closure.rs` for the kind semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyData {
    pub kind: ClosureKind,
    pub type_args: Vec<DataType>,
    /// Authored parameter names. **Empty for preset kinds** and length-N
    /// for `Custom`. `#[serde(default)]` keeps older `.cnnd` files loadable.
    #[serde(default)]
    pub param_names: Vec<String>,
}

impl Default for ApplyData {
    fn default() -> Self {
        // Default to the map-like `(T) -> U` shape with `Float` slots.
        Self {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Float, DataType::Float],
            param_names: vec![],
        }
    }
}

impl NodeData for ApplyData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();

        let params = self.kind.param_types(&self.type_args, &self.param_names);
        let ret = self.kind.return_type(&self.type_args, &self.param_names);
        let param_names = self.kind.param_names(&self.param_names);

        // External pins: a required `f: Function(...)` followed by one ordinary
        // input pin per function parameter. `apply` owns no zone.
        let mut parameters = vec![Parameter {
            id: None,
            name: "f".to_string(),
            data_type: DataType::Function(FunctionType {
                parameter_types: params.clone(),
                output_type: Box::new(ret.clone()),
            }),
        }];
        for (i, t) in params.iter().enumerate() {
            let name = param_names
                .get(i)
                .cloned()
                .unwrap_or_else(|| "element".to_string());
            parameters.push(Parameter {
                id: None,
                name,
                data_type: t.clone(),
            });
        }
        custom.parameters = parameters;
        custom.output_pins = OutputPinDefinition::single_fixed(ret);

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
        // a. The `f` pin (index 0) is required — there is no inline body to
        // fall back to.
        let closure =
            match network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0) {
                NetworkResult::Function(zc) => zc,
                NetworkResult::None => {
                    return EvalOutput::single(NetworkResult::Error(
                        "apply: f not connected".to_string(),
                    ));
                }
                e @ NetworkResult::Error(_) => return EvalOutput::single(e),
                other => {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "apply: f is not a function (got {})",
                        other.to_display_string()
                    )));
                }
            };

        // b. Resolve the argument pins (indices 1..1+arity) against the outer
        // context, before the body is pushed.
        let arity = self
            .kind
            .param_types(&self.type_args, &self.param_names)
            .len();
        let mut args = Vec::with_capacity(arity);
        for i in 0..arity {
            let v = network_evaluator
                .evaluate_arg_required(network_stack, node_id, registry, context, 1 + i);
            if let NetworkResult::Error(_) = v {
                return EvalOutput::single(v);
            }
            args.push(v);
        }

        // c. Run the closure once — the eager drain loop's per-element step with
        // the loop removed. `apply` is an eager consumer holding its real
        // `network_stack`, so it passes it (a nested HOF inside the body can
        // resolve deep captures); the body runs against a fresh inner context
        // like the other eager HOFs (`fold`/`foreach`).
        let mut inner_ctx = context.fresh_inner_for_eager_body();
        let result = run_closure_once(
            network_evaluator,
            network_stack,
            registry,
            &mut inner_ctx,
            &closure,
            args,
        );
        context.drain_inner_context(inner_ctx);

        EvalOutput::single(result)
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

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        // `f` and every argument pin are required — `apply` has no inline body
        // and no per-parameter defaults.
        let mut m = HashMap::new();
        m.insert("f".to_string(), (true, None));
        for name in self.kind.param_names(&self.param_names) {
            m.insert(name, (true, None));
        }
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "apply".to_string(),
        description: "Calls a function value once on a single argument set and returns its result. Wire a `Function` (e.g. a `closure` output, or a function-factory subnetwork's `Function` output) into the required `f` pin and supply one argument per parameter; `apply` runs the function once and yields the return value. This is what makes a `Function` a callable value rather than only fuel for the iterating HOFs.".to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        // External interface is filled in by `calculate_custom_node_type`; the
        // default is the map-like `(Float) -> Float` shape: `f`, one arg, Float out.
        parameters: vec![
            Parameter {
                id: None,
                name: "f".to_string(),
                data_type: DataType::Function(FunctionType {
                    parameter_types: vec![DataType::Float],
                    output_type: Box::new(DataType::Float),
                }),
            },
            Parameter {
                id: None,
                name: "element".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Float),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(ApplyData::default()),
        node_data_saver: generic_node_data_saver::<ApplyData>,
        node_data_loader: generic_node_data_loader::<ApplyData>,
    }
}
