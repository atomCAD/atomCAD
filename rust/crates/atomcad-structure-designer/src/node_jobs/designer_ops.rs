//! The UI-thread halves of a node job on one designer: prepare and install,
//! and the blocking run that is both with the work in between (D9).

use super::{JobInputs, JobResult, JobTarget, JobWork};
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::node_data::NodeData;
use crate::node_network::NodeNetwork;
use crate::structure_designer::StructureDesigner;

impl StructureDesigner {
    /// The guards every job action shares, then `f` over the node's data and
    /// its [`JobInputs`]. Errors are user-facing messages.
    ///
    /// Top-level only: a body node's inputs depend on per-iteration zone
    /// values that do not exist outside an iteration, as for Execute.
    pub(crate) fn with_job_inputs<R>(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        f: impl FnOnce(&dyn NodeData, &mut JobInputs) -> R,
    ) -> Result<(JobTarget, R), String> {
        // Read-only guard (`doc/design_library_linking.md` §6).
        self.ensure_active_editable()?;
        if !scope_path.is_empty() {
            return Err("Cannot run a node inside a higher-order-function body".to_string());
        }
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or("No active node network")?;
        if self
            .get_node_network_data_scoped(scope_path, node_id)
            .is_none()
        {
            return Err(format!("Node {node_id} does not exist"));
        }
        let target = JobTarget {
            document_id: self.document_id,
            network_name: network_name.clone(),
            scope_path: scope_path.to_vec(),
            node_id,
        };
        let r = self.with_eval_context(false, |evaluator, registry, _prefs, context| {
            let network = registry
                .node_networks
                .get(&network_name)
                .expect("active network");
            let data = network.nodes[&node_id].data.as_ref();
            let stack = vec![NetworkStackElement::root(network)];
            let mut inputs = JobInputs::new(evaluator, &stack, registry, context, node_id);
            f(data, &mut inputs)
        });
        Ok((target, r))
    }

    /// Guards, then the node's `prepare_job`: the target and the work to run
    /// off the UI thread. Errors are user-facing messages; a node without a
    /// job is refused ("Node N has no run action").
    pub fn prepare_node_job(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Result<(JobTarget, Box<dyn JobWork>), String> {
        let (target, work) =
            self.with_job_inputs(scope_path, node_id, |data, inputs| data.prepare_job(inputs))?;
        let work = work.ok_or_else(|| format!("Node {node_id} has no run action"))??;
        Ok((target, work))
    }

    /// Installs a finished job's result into `target`, which must be in *this*
    /// designer. Re-checks editability, because a network can become
    /// read-only while a job runs (a library refresh). Marks the node's data
    /// changed, so the next refresh re-evaluates it and its downstream cone —
    /// or the whole active network when the target is in another one (it may
    /// be used there as a custom node).
    ///
    /// Not an undo step, and does not dirty the file (D10): a stored result is
    /// runtime state keyed by an input fingerprint.
    pub fn install_job_result(
        &mut self,
        target: &JobTarget,
        result: Box<dyn JobResult>,
    ) -> Result<String, String> {
        if target.document_id != self.document_id {
            return Err("the document was closed".to_string());
        }
        self.ensure_editable(&target.network_name)?;
        let network = self
            .node_type_registry
            .node_networks
            .get_mut(&target.network_name)
            .ok_or("the network no longer exists")?;
        let node = scope_network_mut(network, &target.scope_path)
            .and_then(|n| n.nodes.get_mut(&target.node_id))
            .ok_or("the node no longer exists")?;
        let summary = result.install(node.data.as_mut())?;
        if self.active_node_network_name.as_deref() == Some(target.network_name.as_str()) {
            self.mark_node_data_changed_scoped(&target.scope_path, target.node_id);
        } else {
            self.mark_full_refresh();
        }
        Ok(summary)
    }

    /// D9: prepare → run on the calling thread (no control) → install, for
    /// any node with a job; returns the install summary. The CLI's `run`.
    /// Does not refresh.
    pub fn run_node_job_blocking(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Result<String, String> {
        let (target, work) = self.prepare_node_job(scope_path, node_id)?;
        let result = work.run(None)?;
        self.install_job_result(&target, result)
    }
}

/// The body `scope_path` names under `network` (CoW-unsharing each zone on
/// the way down), or `network` itself for an empty path.
fn scope_network_mut<'a>(
    network: &'a mut NodeNetwork,
    scope_path: &[u64],
) -> Option<&'a mut NodeNetwork> {
    let mut current = network;
    for hof_id in scope_path {
        current = current.nodes.get_mut(hof_id)?.zone_mut()?;
    }
    Some(current)
}
