//! [`JobInputs`]: what a node's `prepare_job` may read — its evaluated inputs
//! and the evaluation context — and nothing else.

use crate::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::evaluator::network_result::NetworkResult;
use crate::node_type_registry::NodeTypeRegistry;

/// A short-lived view of one node's inputs, built by `StructureDesigner`
/// inside `with_eval_context` for the duration of a `prepare_job` call.
///
/// Generic accessors, not one getter per field a particular node needs: a
/// node evaluates its own pins and reads the context. Inputs are evaluated
/// here, **on the UI thread** — it is fast, it is what reads the network, and
/// it is the only part of a job that must.
pub struct JobInputs<'a, 'n> {
    evaluator: &'a NetworkEvaluator,
    stack: &'a [NetworkStackElement<'n>],
    registry: &'a NodeTypeRegistry,
    context: &'a mut NetworkEvaluationContext,
    node_id: u64,
}

impl<'a, 'n> JobInputs<'a, 'n> {
    pub(crate) fn new(
        evaluator: &'a NetworkEvaluator,
        stack: &'a [NetworkStackElement<'n>],
        registry: &'a NodeTypeRegistry,
        context: &'a mut NetworkEvaluationContext,
        node_id: u64,
    ) -> Self {
        Self {
            evaluator,
            stack,
            registry,
            context,
            node_id,
        }
    }

    /// The node the job is prepared for.
    pub fn node_id(&self) -> u64 {
        self.node_id
    }

    /// Input pin `pin`, as `NetworkEvaluator::evaluate_arg` gives it
    /// (`NetworkResult::None` when disconnected).
    pub fn eval_input(&mut self, pin: usize) -> NetworkResult {
        self.evaluator
            .evaluate_arg(self.stack, self.node_id, self.registry, self.context, pin)
    }

    /// Input pin `pin`, as `NetworkEvaluator::evaluate_arg_required` gives it
    /// (an error when disconnected).
    pub fn eval_input_required(&mut self, pin: usize) -> NetworkResult {
        self.evaluator.evaluate_arg_required(
            self.stack,
            self.node_id,
            self.registry,
            self.context,
            pin,
        )
    }

    /// The evaluation context, read-only (preferences such as
    /// `use_vdw_cutoff` live here).
    pub fn context(&self) -> &NetworkEvaluationContext {
        self.context
    }
}
