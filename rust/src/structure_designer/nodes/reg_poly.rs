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
use crate::util::mat_utils::consistent_round;
use crate::util::transform::Transform2D;
use glam::f64::DVec2;
use glam::i32::IVec2;
use serde::{Deserialize, Serialize};
use std::cmp::max;
use std::collections::HashMap;
use std::f64::consts::PI;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegPolyData {
    pub num_sides: i32, // Number of sides for the polygon
    pub radius: i32,    // Approximate radius in lattice units
}

impl NodeData for RegPolyData {
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
        let num_sides = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            self.num_sides,
            NetworkResult::extract_int,
        ) {
            Ok(value) => max(3, value),
            Err(error) => return error,
        };

        let radius = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.radius,
            NetworkResult::extract_int,
        ) {
            Ok(value) => max(1, value),
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

        let mut vertices: Vec<IVec2> = Vec::new();

        for i in 0..num_sides {
            // Calculate the ideal angle for this vertex
            let angle = kth_angle(i, num_sides);
            // Find the lattice point for this angle
            vertices.push(find_lattice_point(angle, radius));
        }

        // Convert lattice vertices to 2D real-space coordinates using effective unit cell
        let real_vertices = vertices
            .iter()
            .map(|v| drawing_plane.effective_unit_cell.ivec2_lattice_to_real(v))
            .collect();

        // Create a transform at the center of the polygon (origin)
        // No rotation is needed for this type of shape
        NetworkResult::Geometry2D(GeometrySummary2D {
            drawing_plane,
            frame_transform: Transform2D::new(
                DVec2::new(0.0, 0.0), // Center at origin
                0.0,                  // No rotation
            ),
            geo_tree_root: GeoNode::polygon(real_vertices),
        })
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let show_num_sides = !connected_input_pins.contains("num_sides");
        let show_radius = !connected_input_pins.contains("radius");

        match (show_num_sides, show_radius) {
            (true, true) => Some(format!("sides: {} r: {}", self.num_sides, self.radius)),
            (true, false) => Some(format!("sides: {}", self.num_sides)),
            (false, true) => Some(format!("r: {}", self.radius)),
            (false, false) => None,
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("num_sides".to_string(), TextValue::Int(self.num_sides)),
            ("radius".to_string(), TextValue::Int(self.radius)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("num_sides") {
            self.num_sides = v
                .as_int()
                .ok_or_else(|| "num_sides must be an integer".to_string())?;
        }
        if let Some(v) = props.get("radius") {
            self.radius = v
                .as_int()
                .ok_or_else(|| "radius must be an integer".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("d_plane".to_string(), (false, Some("XY plane".to_string())));
        m
    }
}

/// Calculates the closest lattice point to a given floating point position
fn closest_lattice_point(x: f64, y: f64) -> IVec2 {
    IVec2::new(consistent_round(x), consistent_round(y))
}

/// Finds a lattice point close to the ideal position but with reasonably small Miller indices
fn find_lattice_point(angle: f64, radius: i32) -> IVec2 {
    // Start with the ideal position
    let ideal_x = (radius as f64) * angle.cos();
    let ideal_y = (radius as f64) * angle.sin();

    // Find closest lattice point
    closest_lattice_point(ideal_x, ideal_y)
}

fn kth_angle(k: i32, num_sides: i32) -> f64 {
    2.0 * PI * (k as f64) / (num_sides as f64)
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "reg_poly".to_string(),
        description:
            "Outputs a regular polygon with integer radius. The number of sides is a property too.
Now that we have general polygon node this node is less used."
                .to_string(),
        summary: None,
        category: NodeTypeCategory::Geometry2D,
        parameters: vec![
            Parameter {
                id: None,
                name: "num_sides".to_string(),
                data_type: DataType::Int,
            },
            Parameter {
                id: None,
                name: "radius".to_string(),
                data_type: DataType::Int,
            },
            Parameter {
                id: None,
                name: "d_plane".to_string(),
                data_type: DataType::DrawingPlane,
            },
        ],
        output_type: DataType::Geometry2D,
        public: true,
        node_data_creator: || {
            Box::new(RegPolyData {
                num_sides: 3,
                radius: 3,
            })
        },
        node_data_saver: generic_node_data_saver::<RegPolyData>,
        node_data_loader: generic_node_data_loader::<RegPolyData>,
    }
}
