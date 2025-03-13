use crate::kernel::node_network::NodeNetwork;
use crate::kernel::node_type_registry::NodeTypeRegistry;
use crate::util::transform::Transform;

pub struct FrameTransformEvaluator {

}

impl FrameTransformEvaluator {
    pub fn eval(&self, network: &NodeNetwork, node_id: u64, registry: &NodeTypeRegistry) -> Vec<Transform> {
    }
}
