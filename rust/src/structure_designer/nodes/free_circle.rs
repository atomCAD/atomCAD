use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::drawing_plane::DrawingPlane;
use crate::geo_tree::GeoNode;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::serialization_utils::dvec2_serializer;
use crate::util::transform::Transform2D;
use glam::f64::DVec2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeCircleData {
    #[serde(with = "dvec2_serializer")]
    pub center: DVec2,
    pub radius: f64,
}

impl NodeData for FreeCircleData {
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
        let center = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            self.center,
            NetworkResult::extract_vec2,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let radius = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.radius,
            NetworkResult::extract_float,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
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
            Err(error) => return EvalOutput::single(error),
        };

        // No lattice→real conversion: center and radius are already real-space
        // (Å) within the drawing-plane frame. The plane itself is still
        // lattice-derived; only the position/size within it is free.
        EvalOutput::single(NetworkResult::Geometry2D(GeometrySummary2D {
            drawing_plane,
            frame_transform: Transform2D::new(center, 0.0),
            geo_tree_root: GeoNode::circle(center, radius),
        }))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let show_center = !connected_input_pins.contains("center");
        let show_radius = !connected_input_pins.contains("radius");

        match (show_center, show_radius) {
            (true, true) => Some(format!(
                "c: ({:.2}, {:.2}) r: {:.2}",
                self.center.x, self.center.y, self.radius
            )),
            (true, false) => Some(format!("c: ({:.2}, {:.2})", self.center.x, self.center.y)),
            (false, true) => Some(format!("r: {:.2}", self.radius)),
            (false, false) => None,
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("center".to_string(), TextValue::Vec2(self.center)),
            ("radius".to_string(), TextValue::Float(self.radius)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("center") {
            self.center = v
                .as_vec2()
                .ok_or_else(|| "center must be a Vec2".to_string())?;
        }
        if let Some(v) = props.get("radius") {
            self.radius = v
                .as_float()
                .ok_or_else(|| "radius must be a float".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("d_plane".to_string(), (false, Some("XY plane".to_string())));
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "free_circle".to_string(),
        description: "Outputs a circle with real-space (Å) center coordinates and radius — \
            the non-lattice-aligned analog of `circle`."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::Geometry2D,
        parameters: vec![
            Parameter {
                id: None,
                name: "center".to_string(),
                data_type: DataType::Vec2,
            },
            Parameter {
                id: None,
                name: "radius".to_string(),
                data_type: DataType::Float,
            },
            Parameter {
                id: None,
                name: "d_plane".to_string(),
                data_type: DataType::DrawingPlane,
            },
        ],
        output_pins: OutputPinDefinition::single(DataType::Geometry2D),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || {
            Box::new(FreeCircleData {
                center: DVec2::ZERO,
                radius: 5.0,
            })
        },
        node_data_saver: generic_node_data_saver::<FreeCircleData>,
        node_data_loader: generic_node_data_loader::<FreeCircleData>,
    }
}
