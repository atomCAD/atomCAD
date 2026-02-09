use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::drawing_plane::DrawingPlane;
use crate::geo_tree::GeoNode;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::serialization_utils::ivec2_serializer;
use crate::util::transform::Transform2D;
use glam::i32::IVec2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RectData {
    #[serde(with = "ivec2_serializer")]
    pub min_corner: IVec2,
    #[serde(with = "ivec2_serializer")]
    pub extent: IVec2,
}

impl NodeData for RectData {
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
        let min_corner = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            self.min_corner,
            NetworkResult::extract_ivec2,
        ) {
            Ok(value) => value,
            Err(error) => return error,
        };

        let extent = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.extent,
            NetworkResult::extract_ivec2,
        ) {
            Ok(value) => value,
            Err(error) => return error,
        };

        let drawing_plane = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            DrawingPlane::default(),
            NetworkResult::extract_drawing_plane,
        ) {
            Ok(value) => value,
            Err(error) => return error,
        };

        let geo_tree_root = create_parallelogram_on_plane(&drawing_plane, min_corner, extent);

        // Calculate center in 2D lattice coordinates, then convert to 2D real space
        let center_2d_lattice = min_corner.as_dvec2() + extent.as_dvec2() / 2.0;
        let center = drawing_plane
            .effective_unit_cell
            .dvec2_lattice_to_real(&center_2d_lattice);

        NetworkResult::Geometry2D(GeometrySummary2D {
            drawing_plane,
            frame_transform: Transform2D::new(center, 0.0),
            geo_tree_root,
        })
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let show_min_corner = !connected_input_pins.contains("min_corner");
        let show_extent = !connected_input_pins.contains("extent");

        match (show_min_corner, show_extent) {
            (true, true) => Some(format!(
                "mc: ({},{}) e: ({},{})",
                self.min_corner.x, self.min_corner.y, self.extent.x, self.extent.y
            )),
            (true, false) => Some(format!("mc: ({},{})", self.min_corner.x, self.min_corner.y)),
            (false, true) => Some(format!("e: ({},{})", self.extent.x, self.extent.y)),
            (false, false) => None,
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("min_corner".to_string(), TextValue::IVec2(self.min_corner)),
            ("extent".to_string(), TextValue::IVec2(self.extent)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("min_corner") {
            self.min_corner = v
                .as_ivec2()
                .ok_or_else(|| "min_corner must be an IVec2".to_string())?;
        }
        if let Some(v) = props.get("extent") {
            self.extent = v
                .as_ivec2()
                .ok_or_else(|| "extent must be an IVec2".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("d_plane".to_string(), (false, Some("XY plane".to_string())));
        m
    }
}

/// Creates a parallelogram on the drawing plane from 2D integer lattice coordinates.
/// The parallelogram is represented as an intersection of 4 half-planes (2 pairs of opposing edges).
/// Uses the drawing plane's effective_unit_cell to convert lattice → real 2D coordinates.
fn create_parallelogram_on_plane(
    drawing_plane: &DrawingPlane,
    min_corner: IVec2,
    extent: IVec2,
) -> GeoNode {
    // Calculate the four corners in 2D integer lattice space
    let corner_00 = min_corner; // min_corner
    let corner_10 = min_corner + IVec2::new(extent.x, 0);
    let corner_01 = min_corner + IVec2::new(0, extent.y);
    let corner_11 = min_corner + extent; // max_corner

    // Convert to 2D real-space coordinates using the effective unit cell
    let corner_00_real = drawing_plane
        .effective_unit_cell
        .ivec2_lattice_to_real(&corner_00);
    let corner_10_real = drawing_plane
        .effective_unit_cell
        .ivec2_lattice_to_real(&corner_10);
    let corner_01_real = drawing_plane
        .effective_unit_cell
        .ivec2_lattice_to_real(&corner_01);
    let corner_11_real = drawing_plane
        .effective_unit_cell
        .ivec2_lattice_to_real(&corner_11);

    // Create 4 half-planes defining the parallelogram edges
    let half_planes = vec![
        GeoNode::half_plane(corner_10_real, corner_00_real),
        GeoNode::half_plane(corner_01_real, corner_11_real),
        GeoNode::half_plane(corner_00_real, corner_01_real),
        GeoNode::half_plane(corner_11_real, corner_10_real),
    ];

    // Return the intersection of all half-planes
    GeoNode::intersection_2d(half_planes)
}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "rect".to_string(),
      description: "Outputs a rectangle with integer minimum corner coordinates and integer width and height.".to_string(),
      summary: None,
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
        Parameter {
            id: None,
            name: "min_corner".to_string(),
            data_type: DataType::IVec2,
        },
        Parameter {
          id: None,
          name: "extent".to_string(),
          data_type: DataType::IVec2,
        },
        Parameter {
          id: None,
          name: "d_plane".to_string(),
          data_type: DataType::DrawingPlane,
        },
      ],
      output_type: DataType::Geometry2D,
      public: true,
      node_data_creator: || Box::new(RectData {
        min_corner: IVec2::new(-1, -1),
        extent: IVec2::new(2, 2),
      }),
      node_data_saver: generic_node_data_saver::<RectData>,
      node_data_loader: generic_node_data_loader::<RectData>,
    }
}
