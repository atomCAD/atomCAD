use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::geo_tree::GeoNode;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::unit_cell_mismatch_error;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::util::transform::Transform2D;
use glam::f64::DVec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Union2DData {}

impl NodeData for Union2DData {
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
    ) -> NetworkResult {
        //let _timer = Timer::new("eval_union");
        let mut shapes: Vec<GeoNode> = Vec::new();
        let mut frame_translation = DVec2::ZERO;

        let shapes_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        if let NetworkResult::Error(_) = shapes_val {
            return shapes_val;
        }

        // Extract the array elements from shapes_val
        let shape_results = if let NetworkResult::Array(array_elements) = shapes_val {
            array_elements
        } else {
            return NetworkResult::Error("Expected array of geometry shapes".to_string());
        };

        let shape_count = shape_results.len();

        if shape_count == 0 {
            return NetworkResult::Error("Union requires at least one input geometry".to_string());
        }

        // Extract geometries and check unit cell compatibility
        let mut geometries: Vec<GeometrySummary2D> = Vec::new();
        for shape_val in shape_results {
            if let NetworkResult::Geometry2D(shape) = shape_val {
                geometries.push(shape);
            } else {
                return NetworkResult::Error("All inputs must be geometry objects".to_string());
            }
        }

        // Check drawing plane compatibility - compare all to the first geometry
        if !GeometrySummary2D::all_have_compatible_drawing_planes(&geometries) {
            return unit_cell_mismatch_error();
        }

        // All drawing planes are compatible, proceed with union
        // Take the first drawing plane by value before consuming the geometries vector
        let first_drawing_plane = geometries[0].drawing_plane.clone();
        for geometry in geometries.into_iter() {
            shapes.push(geometry.geo_tree_root);
            frame_translation += geometry.frame_transform.translation;
        }

        frame_translation /= shape_count as f64;

        NetworkResult::Geometry2D(GeometrySummary2D {
            drawing_plane: first_drawing_plane,
            frame_transform: Transform2D::new(frame_translation, 0.0),
            geo_tree_root: GeoNode::union_2d(shapes),
        })
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
        m.insert("shapes".to_string(), (true, None)); // required
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "union_2d".to_string(),
      description: "Computes the Boolean union of any number of 2D geometries. The `shapes` input accepts an array of `Geometry2D` values (array-typed input; you can connect multiple wires and they will be concatenated).".to_string(),
      summary: None,
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
          Parameter {
              id: None,
              name: "shapes".to_string(),
              data_type: DataType::Array(Box::new(DataType::Geometry2D)),
          },
      ],
      output_type: DataType::Geometry2D,
      public: true,
      node_data_creator: || Box::new(Union2DData {}),
      node_data_saver: generic_node_data_saver::<Union2DData>,
      node_data_loader: generic_node_data_loader::<Union2DData>,
    }
}
