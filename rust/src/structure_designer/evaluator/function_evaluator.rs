use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::Closure;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::NoData;
use crate::structure_designer::node_type::OutputPinDefinition;
use crate::structure_designer::node_type::no_data_loader;
use crate::structure_designer::node_type::no_data_saver;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::nodes::value::ValueData;
use crate::structure_designer::{node_network::NodeNetwork, node_type::NodeType};
use glam::f64::DVec2;

/*
 * FunctionEvaluator is capable of evaluating a function, more precisely evaluating a closure.
 * The type of the function needs to be known at construction time,
 * and the constructed FunctionEvaluator instance can be reused for multiple evaluations
 * where the actual arguments can change.
 * Construction of the FunctionEvaluator instance is somewhat expensive,
 * but changing only the argument values is relativey cheap,
 * so a constructed FunctionEvaluator should be reused as much as possible.
 *
 * Internally the FunctionEvaluator is a bit of a hack now: it builds a little node network so that nodes
 * can be evaluated in a node network context.
 * In the future this could be refactored only by rethinking the evaluation philosophy of the
 * whole node network: if the evaluation would work by a framework evaluating the input
 * pins of a node and the node would only get the input values, nodes could be evaluated
 * without building a node network first. We need to think thorugh though: is it not too
 * restricting? Are there nodes which need to decide which input pins they evaluate and how many times?
 */
#[derive(Clone)]
pub struct FunctionEvaluator {
    node_network: NodeNetwork,
    main_node_id: u64,
    value_node_ids: Vec<u64>,
}

impl FunctionEvaluator {
    /// Construct a `FunctionEvaluator` after verifying the closure's source
    /// network and source node both resolve in `registry`. Returns `Err` with
    /// a human-readable message otherwise.
    ///
    /// `FunctionEvaluator::new` silently builds a degenerate FE on a missing
    /// source — fine for eager nodes that fail once and stop, but for lazy
    /// iterator pipelines a degenerate FE would multiply the same error
    /// across every pulled element. Callers in `map.eval()` / `filter.eval()`
    /// use `try_build` so that construction-time failures surface as a single
    /// `EvalOutput::single(Error(_))` instead of an exhausted-but-erroring
    /// walker. See `doc/design_iterators.md` ("Construction-time error
    /// handling").
    pub fn try_build(closure: Closure, registry: &NodeTypeRegistry) -> Result<Self, String> {
        let network = registry
            .node_networks
            .get(&closure.node_network_name)
            .ok_or_else(|| {
                format!(
                    "source network '{}' not found in registry",
                    closure.node_network_name
                )
            })?;
        if !network.nodes.contains_key(&closure.node_id) {
            return Err(format!(
                "source node {} missing in network '{}'",
                closure.node_id, closure.node_network_name
            ));
        }
        Ok(Self::new(closure, registry))
    }

    pub fn new(closure: Closure, registry: &NodeTypeRegistry) -> Self {
        let mut ret = Self {
      node_network: NodeNetwork::new(NodeType {
        name: "_tmp_".to_string(),
        description: "".to_string(),
        summary: None,
        category: crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory::OtherBuiltin,
        parameters: Vec::new(),
        output_pins: OutputPinDefinition::single(DataType::None),
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
        public: false,
      }),
      value_node_ids: Vec::new(),
      main_node_id: 0,
    };
        // Add the main node.

        let network = match registry.node_networks.get(&closure.node_network_name) {
            Some(network) => network,
            None => return ret,
        };

        let node = match network.nodes.get(&closure.node_id) {
            Some(node) => node,
            None => return ret,
        };

        let node_data = network.get_node_network_data(closure.node_id);

        let cloned_node_data = node_data.map(|data| data.clone_box());

        let main_node_id = ret.node_network.add_node(
            &node.node_type_name,
            DVec2::new(0.0, 0.0),
            closure.captured_argument_values.len(),
            cloned_node_data
                .unwrap_or_else(|| Box::new(crate::structure_designer::node_data::NoData {})),
        );

        // Populate the dynamic node-type cache so that nodes with dynamic
        // parameters (expr, parameter, sequence, map, ...) report their real
        // parameter list instead of the empty base parameter list.
        if let Some(main_node) = ret.node_network.nodes.get_mut(&main_node_id) {
            NodeTypeRegistry::populate_custom_node_type_cache_with_types(
                &registry.built_in_node_types,
                &registry.record_type_defs,
                &registry.built_in_record_type_defs,
                main_node,
                false,
            );
        }

        // Add value nodes corresponding to parameters.
        for i in 0..closure.captured_argument_values.len() {
            let value = closure.captured_argument_values[i].clone();
            let node_id = ret.node_network.add_node(
                "value",
                DVec2::new(0.0, 0.0),
                0,
                Box::new(ValueData { value }),
            );
            ret.value_node_ids.push(node_id);
            ret.node_network
                .connect_nodes(node_id, 0, main_node_id, i, false);
        }
        ret.main_node_id = main_node_id;
        ret
    }

    pub fn set_argument_value(&mut self, arg_index: usize, value: NetworkResult) {
        let node_id = self.value_node_ids[arg_index];
        self.node_network
            .set_node_network_data(node_id, Box::new(ValueData { value }));
    }

    /// Evaluate the closure against the most recently set argument values.
    ///
    /// `outer_context` is the calling pass's evaluation context. A fresh
    /// inner context is constructed for the body evaluation, but the
    /// per-pass flags that need to flow through (`execute`, `use_vdw_cutoff`)
    /// are inherited so effects nested inside `map`/`filter`/`fold`/
    /// `foreach` bodies fire correctly under Execute. The inner context's
    /// `print_buffer` is drained back into `outer_context.print_buffer` at
    /// end-of-call so prints from inner-body nodes aggregate into the single
    /// per-pass log instead of being silently dropped.
    ///
    /// `node_errors` / `node_output_strings` / `selected_node_eval_cache` /
    /// `top_level_parameters` are intentionally **not** inherited — they are
    /// per-pass scratch state scoped to the outer network and would be
    /// confusing if mixed with the inner closure body's nodes.
    ///
    /// See `doc/design_node_execution.md` (Phase 2 — propagation through
    /// FunctionEvaluator).
    pub fn evaluate(
        &self,
        evaluator: &NetworkEvaluator,
        registry: &NodeTypeRegistry,
        outer_context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
        // We assign the root node network zero node id. It is not used in the evaluation.
        let network_stack = vec![NetworkStackElement {
            node_network: &self.node_network,
            node_id: 0,
        }];

        let mut inner = NetworkEvaluationContext::new();
        inner.execute = outer_context.execute;
        inner.use_vdw_cutoff = outer_context.use_vdw_cutoff;
        // print_buffer is *not* inherited — each FE call starts with an empty
        // buffer and is drained back into the outer buffer below.
        let result = evaluator.evaluate(
            &network_stack,
            self.main_node_id,
            0,
            registry,
            false,
            &mut inner,
        );
        outer_context.print_buffer.append(&mut inner.print_buffer);
        result
    }
}
