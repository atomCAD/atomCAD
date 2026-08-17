use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::drawing_plane::DrawingPlane;
use crate::geo_tree::GeoNode;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::evaluator::network_result::first_array_element_error;
use crate::structure_designer::evaluator::network_result::input_missing_error;
use crate::structure_designer::evaluator::network_result::unit_cell_mismatch_error;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use atomcad_util::transform::Transform2D;
use glam::f64::DVec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diff2DData {}

impl NodeData for Diff2DData {
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
        //let _timer = Timer::new("eval_diff");
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        let base_input_name = registry.get_parameter_name(node, 0);
        let sub_input_name = registry.get_parameter_name(node, 1);

        if node.arguments[0].is_empty() {
            return EvalOutput::single(input_missing_error(&base_input_name));
        }

        let (mut geometry, mut frame_translation, result_drawing_plane) = match helper_union(
            network_evaluator,
            network_stack,
            node_id,
            0,
            registry,
            context,
        ) {
            Ok(parts) => parts,
            Err(HelperUnionError::Upstream(err)) => {
                return EvalOutput::single(*err);
            }
            Err(HelperUnionError::NoShapes) => {
                return EvalOutput::single(error_in_input(&base_input_name));
            }
            Err(HelperUnionError::PlaneMismatch) => {
                return EvalOutput::single(unit_cell_mismatch_error());
            }
        };

        if !node.arguments[1].is_empty() {
            let (sub_geometry, sub_frame_translation, sub_drawing_plane) = match helper_union(
                network_evaluator,
                network_stack,
                node_id,
                1,
                registry,
                context,
            ) {
                Ok(parts) => parts,
                Err(HelperUnionError::Upstream(err)) => {
                    return EvalOutput::single(*err);
                }
                Err(HelperUnionError::NoShapes) => {
                    return EvalOutput::single(error_in_input(&sub_input_name));
                }
                Err(HelperUnionError::PlaneMismatch) => {
                    return EvalOutput::single(unit_cell_mismatch_error());
                }
            };

            // Check drawing plane compatibility between base and sub
            if !result_drawing_plane.is_compatible(&sub_drawing_plane) {
                return EvalOutput::single(unit_cell_mismatch_error());
            }

            geometry = GeoNode::difference_2d(Box::new(geometry), Box::new(sub_geometry));

            frame_translation += sub_frame_translation;
            frame_translation *= 0.5;
        }

        EvalOutput::single(NetworkResult::Geometry2D(GeometrySummary2D {
            drawing_plane: result_drawing_plane,
            frame_transform: Transform2D::new(frame_translation, 0.0),
            geo_tree_root: geometry,
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

    fn get_parameter_metadata(&self) -> std::collections::HashMap<String, (bool, Option<String>)> {
        let mut m = std::collections::HashMap::new();
        m.insert("base".to_string(), (true, None)); // required
        m.insert("sub".to_string(), (true, None)); // required
        m
    }
}

/// Why `helper_union` could not produce a unioned geometry for one input pin.
enum HelperUnionError {
    /// The input, or one of its elements, evaluated to an `Error`. The payload
    /// is the (already localized) error to forward verbatim, so the upstream
    /// root cause survives instead of collapsing into a bare
    /// "error in <pin> input" (`doc/design_error_management.md` Phase 6 —
    /// chain hygiene).
    Upstream(Box<NetworkResult>),
    /// The input array was empty, missing, or contained a non-Geometry2D value.
    NoShapes,
    /// Two or more geometries in the array sit on incompatible drawing planes.
    PlaneMismatch,
}

fn helper_union<'a>(
    network_evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    parameter_index: usize,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
) -> Result<(GeoNode, DVec2, DrawingPlane), HelperUnionError> {
    let mut shapes: Vec<GeoNode> = Vec::new();
    let mut frame_translation = DVec2::ZERO;

    let shapes_val = network_evaluator.evaluate_arg_required(
        network_stack,
        node_id,
        registry,
        context,
        parameter_index,
    );

    if let NetworkResult::Error(_) = shapes_val {
        return Err(HelperUnionError::Upstream(Box::new(shapes_val)));
    }

    // Extract the array elements from shapes_val
    let shape_results = if let NetworkResult::Array(array_elements) = shapes_val {
        array_elements
    } else {
        return Err(HelperUnionError::NoShapes);
    };

    // Chain hygiene (`doc/design_error_management.md` Phase 6): forward a
    // failing element's own error instead of reporting it as `NoShapes`.
    if let Some(err) = first_array_element_error(
        &registry.get_parameter_name(
            NetworkStackElement::get_top_node(network_stack, node_id),
            parameter_index,
        ),
        &shape_results,
    ) {
        return Err(HelperUnionError::Upstream(Box::new(err)));
    }

    let shape_count = shape_results.len();

    if shape_count == 0 {
        return Err(HelperUnionError::NoShapes);
    }

    // Extract geometries and check unit cell compatibility
    let mut geometries: Vec<GeometrySummary2D> = Vec::new();
    for shape_val in shape_results {
        if let NetworkResult::Geometry2D(shape) = shape_val {
            geometries.push(shape);
        } else {
            return Err(HelperUnionError::NoShapes);
        }
    }

    // Check drawing plane compatibility - compare all to the first geometry
    if !GeometrySummary2D::all_have_compatible_drawing_planes(&geometries) {
        return Err(HelperUnionError::PlaneMismatch);
    }

    // All drawing planes are compatible, proceed with union
    let first_drawing_plane = geometries[0].drawing_plane.clone();
    for geometry in geometries.into_iter() {
        shapes.push(geometry.geo_tree_root);
        frame_translation += geometry.frame_transform.translation;
    }

    frame_translation /= shape_count as f64;
    Ok((
        GeoNode::union_2d(shapes),
        frame_translation,
        first_drawing_plane,
    ))
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "diff_2d".to_string(),
        description: "Computes the Boolean difference of two 2D geometries.".to_string(),
        summary: None,
        category: NodeTypeCategory::Geometry2D,
        parameters: vec![
            Parameter {
                id: None,
                name: "base".to_string(),
                data_type: DataType::Array(Box::new(DataType::Geometry2D)), // A set of shapes to subtract from
            },
            Parameter {
                id: None,
                name: "sub".to_string(),
                data_type: DataType::Array(Box::new(DataType::Geometry2D)), // A set of shapes to subtract from base
            },
        ],
        output_pins: OutputPinDefinition::single(DataType::Geometry2D),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(Diff2DData {}),
        node_data_saver: generic_node_data_saver::<Diff2DData>,
        node_data_loader: generic_node_data_loader::<Diff2DData>,
    }
}
