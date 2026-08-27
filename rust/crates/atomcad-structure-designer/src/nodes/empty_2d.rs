//! The `empty_2d` node: a `Geometry2D` bounding **no area at all** — the 2D
//! counterpart of `empty`.
//!
//! See `empty.rs` for why emptiness is a first-class `GeoNode` rather than
//! something a boolean is expected to cancel into. The geo_tree root here is
//! `GeoNode::empty_2d`, whose SDF is `f64::MAX` everywhere, so `union_2d`
//! treats it as the identity, `intersect_2d` as the annihilator, and `diff_2d`
//! subtracts nothing.
//!
//! Data-free (`Empty2DData {}`): no gadget, no editor, no text properties. The
//! one input is the optional `d_plane` every 2D primitive carries (default
//! `DrawingPlane::default()`) — `intersect_2d` and friends compare it in
//! `GeometrySummary2D::all_have_compatible_drawing_planes`, so wire the same
//! plane the shapes it is combined with use. The frame transform is the
//! identity: an empty shape has no centroid to contribute to the averaged
//! frame the 2D booleans compute.

use crate::data_type::DataType;
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::GeometrySummary2D;
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use atomcad_crystolecule::drawing_plane::DrawingPlane;
use atomcad_geo_tree::GeoNode;
use atomcad_util::transform::Transform2D;
use glam::f64::DVec2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Empty2DData {}

impl NodeData for Empty2DData {
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
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let drawing_plane = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            DrawingPlane::default(),
            NetworkResult::extract_drawing_plane,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        EvalOutput::single(NetworkResult::Geometry2D(GeometrySummary2D {
            drawing_plane,
            frame_transform: Transform2D::new(DVec2::ZERO, 0.0),
            geo_tree_root: GeoNode::empty_2d(),
        }))
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
        let mut m = HashMap::new();
        m.insert("d_plane".to_string(), (false, None));
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "empty_2d".to_string(),
        description: "Outputs an empty 2D geometry — a shape containing no points at all. \
            It is the identity for `union_2d`, cancels any `intersect_2d` it takes part in, \
            and subtracts nothing in `diff_2d`. The 2D counterpart of `empty`."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::Geometry2D,
        parameters: vec![Parameter {
            id: None,
            name: "d_plane".to_string(),
            data_type: DataType::DrawingPlane,
        }],
        output_pins: OutputPinDefinition::single(DataType::Geometry2D),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(Empty2DData {}),
        node_data_saver: generic_node_data_saver::<Empty2DData>,
        node_data_loader: generic_node_data_loader::<Empty2DData>,
    }
}
