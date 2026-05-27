//! The `apply` node: calls a function value on a single argument set, with
//! support for **partial application** (wiring only a contiguous prefix of the
//! function's arg pins) and **recursive consumption** (a body that returns
//! another `Function` value continues to absorb remaining args).
//!
//! `apply` shares the `{ kind, type_args }` data model with the `closure` node
//! (`nodes/closure.rs`), but with one critical asymmetry: when its `f` pin is
//! wired, the wired source's **declared (canonical, flat) function type** —
//! not the stored `ApplyData` — drives the arg-pin enumeration and the
//! output-pin type. The pin layout is updated by a post-pass in
//! `NodeTypeRegistry::repair_node_network` after the wire connects/disconnects
//! and validation runs. When `f` is disconnected, `ApplyData.kind` /
//! `type_args` drive the default layout exactly as today (the user-set
//! kind-picker default — Open Question 2 in `doc/design_currying.md`).
//!
//! `apply.eval` consumes the underlying closure's **body** arity per step
//! (`ZoneClosure.param_types.len()`), which is *decoupled* from the closure's
//! declared (flat) function type when the body returns a `Function` value. The
//! loop recurses on the returned `Function` until either the args run out
//! (returning a partial closure or the final result) or the body returns a
//! non-function. See `doc/design_currying.md` §"`apply` semantics".

use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::{DataType, FunctionType};
use crate::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::zone_closure::{ZoneClosure, run_closure_once};
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
use std::sync::Arc;

/// Stored state for an `apply` node. Identical in shape to `ClosureData`: the
/// shape template (which fixes arity and which slots are free) plus the free
/// type arguments and authored parameter names (used only by the `Custom`
/// kind). See `nodes/closure.rs` for the kind semantics.
///
/// **Currying Phase 3 semantic shift**: when `f` is wired, `ApplyData` is
/// ignored by `calculate_custom_node_type` (the registry's repair post-pass
/// overrides the node's `custom_node_type` from the wired source). When `f` is
/// disconnected, `ApplyData` drives the default pin layout exactly as today.
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
        // ApplyData-driven layout (the disconnected-`f` default). When `f` is
        // wired, `NodeTypeRegistry::repair_node_network` runs a post-pass that
        // overrides this with a layout derived from the wired source's declared
        // (canonical, flat) function type and the count of wired arg pins.
        let mut custom = base_node_type.clone();

        let params = self.kind.param_types(&self.type_args, &self.param_names);
        let ret = self.kind.return_type(&self.type_args, &self.param_names);
        let param_names = self.kind.param_names(&self.param_names);

        // External pins: a required `f: Function(...)` followed by one ordinary
        // input pin per function parameter. `apply` owns no zone.
        let mut parameters = vec![Parameter {
            id: None,
            name: "f".to_string(),
            data_type: DataType::Function(FunctionType::new(params.clone(), ret.clone())),
        }];
        for (i, t) in params.iter().enumerate() {
            let name = param_names
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("arg{}", i));
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
        // 1. Resolve `f`. Required — no inline body fallback.
        let mut f_current: ZoneClosure =
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

        // 2. Count the contiguous prefix of wired arg pins, and resolve their
        // values against the outer context. The validator enforces prefix-only
        // wiring (a wired pin past an unwired one is an error attributed to
        // this `apply` node); at eval we simply stop at the first unwired pin.
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        let arg_pin_count = node.arguments.len().saturating_sub(1);

        let mut remaining: std::collections::VecDeque<NetworkResult> =
            std::collections::VecDeque::with_capacity(arg_pin_count);
        let mut k: usize = 0;
        for i in 0..arg_pin_count {
            // `f` lives at index 0; arg pins are 1..1+N.
            if node.arguments[1 + i].incoming_wires.is_empty() {
                break; // First unwired pin terminates the prefix.
            }
            let v = network_evaluator
                .evaluate_arg_required(network_stack, node_id, registry, context, 1 + i);
            if let NetworkResult::Error(_) = v {
                return EvalOutput::single(v);
            }
            remaining.push_back(v);
            k += 1;
        }

        // 3. Identity-partial guard. With k=0 and a non-thunk `f`, `apply` is
        // the identity: return `f` unchanged. Thunks (declared flat arity 0)
        // fall through to the loop and run their body below.
        if k == 0 && !f_current.function_type().parameter_types.is_empty() {
            return EvalOutput::single(NetworkResult::Function(f_current));
        }

        // 4. Walk the closure step-by-step, consuming body-arity args per
        // iteration. The recursive branch fires only when the wired closure's
        // body returns another `Function` and there are still args to consume.
        loop {
            let n_body = f_current.param_types.len();

            // Partial step: not enough args left to fill this body invocation.
            // Bind whatever args remain into `pre_supplied_args` and return a
            // shorter `ZoneClosure`.
            if remaining.len() < n_body {
                let drained: Vec<NetworkResult> = remaining.drain(..).collect();
                let drained_len = drained.len();
                #[allow(clippy::arc_with_non_send_sync)]
                let extended = {
                    let mut v = (*f_current.pre_supplied_args).clone();
                    v.extend(drained);
                    Arc::new(v)
                };
                let partial = ZoneClosure {
                    body: Arc::clone(&f_current.body),
                    captures: Arc::clone(&f_current.captures),
                    zone_output_wires: Arc::clone(&f_current.zone_output_wires),
                    owner_node_id: f_current.owner_node_id,
                    param_types: f_current.param_types[drained_len..].to_vec(),
                    return_type: f_current.return_type.clone(),
                    pre_supplied_args: extended,
                };
                return EvalOutput::single(NetworkResult::Function(partial));
            }

            // Defensive guard against a pathological 0-arity body that returns
            // a non-Function while more args remain. In that case the loop
            // would spin forever consuming no args; surface as an error.
            if n_body == 0 && remaining.is_empty() {
                // No work this iteration AND no args to consume — must be a
                // thunk-force. Run the body and return its result below.
            }

            // Full body step: consume n_body args, run the body.
            let step_args: Vec<NetworkResult> = remaining.drain(..n_body).collect();
            let mut inner_ctx = context.fresh_inner_for_eager_body();
            let result = run_closure_once(
                network_evaluator,
                network_stack,
                registry,
                &mut inner_ctx,
                &f_current,
                step_args,
            );
            context.drain_inner_context(inner_ctx);

            if remaining.is_empty() {
                return EvalOutput::single(result);
            }

            // More args to go — the body must have returned another Function.
            match result {
                NetworkResult::Function(next) => {
                    if n_body == 0 && Arc::ptr_eq(&next.body, &f_current.body) {
                        // Pathological: a 0-arity thunk returned itself. Each
                        // iteration with `n_body == 0` must advance `f_current`
                        // or the loop never terminates.
                        return EvalOutput::single(NetworkResult::Error(
                            "apply: 0-arity body returned itself with args still remaining"
                                .to_string(),
                        ));
                    }
                    f_current = next;
                }
                NetworkResult::Error(_) => return EvalOutput::single(result),
                other => {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "apply: expected Function for further application, got {}",
                        other.to_display_string()
                    )));
                }
            }
        }
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
        // `f` is the only required pin; arg pins are optional so that wiring
        // only a contiguous prefix is valid (the unwired tail rolls into the
        // resulting function's remaining parameter list). The prefix-only rule
        // is enforced by `network_validator::validate_apply_prefix_wiring`.
        let mut m = HashMap::new();
        m.insert("f".to_string(), (true, None));
        // Mirror the kind's param_names (used to skin the static-shape default
        // when `f` is disconnected). When `f` is wired, the registry's
        // repair post-pass overrides the custom node type and these names may
        // change — but `get_parameter_metadata` is read from `ApplyData`, not
        // from the custom type, so we stay in lock-step with the ApplyData-
        // driven layout (the only one the validator's "required pin missing"
        // rule would consult under).
        for name in self.kind.param_names(&self.param_names) {
            m.insert(name, (false, None));
        }
        // Also mark the generic `argN` fallback names as optional, so the
        // wire-derived layout (when `f` is wired) still has its arg pins
        // recognized as optional even if `get_parameter_metadata` is consulted
        // before the post-pass-driven name is installed. This belt-and-braces
        // entry is harmless — the lookup is by exact pin name.
        let arity = self
            .kind
            .param_types(&self.type_args, &self.param_names)
            .len();
        for i in 0..arity {
            m.insert(format!("arg{}", i), (false, None));
        }
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "apply".to_string(),
        description: "Calls a function value on a single argument set and returns the result, with support for partial application (wire only a contiguous prefix of the arg pins to get back a function of the remaining parameters). Wire a `Function` source — a `closure` output, a node's function pin, or a subnetwork's `Function` output — into the required `f` pin; the arg-pin layout is derived from `f`'s declared (canonical, flat) function type. With every arg wired, `apply` produces the return value; with k of N wired, it produces a `Function` carrying the k bound values and the remaining N-k parameters. A node returning a function (a 1-arg closure whose body returns a 1-arg closure) is consumed recursively against further wired args.".to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        // External interface is filled in by `calculate_custom_node_type`; the
        // default is the map-like `(Float) -> Float` shape: `f`, one arg, Float out.
        parameters: vec![
            Parameter {
                id: None,
                name: "f".to_string(),
                data_type: DataType::Function(FunctionType::new(
                    vec![DataType::Float],
                    DataType::Float,
                )),
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
