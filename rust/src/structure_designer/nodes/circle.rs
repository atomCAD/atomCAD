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
use crate::util::serialization_utils::ivec2_serializer;
use crate::util::transform::Transform2D;
use glam::f64::{DMat2, DVec2};
use glam::i32::IVec2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircleData {
    #[serde(with = "ivec2_serializer")]
    pub center: IVec2,
    pub radius: i32,
}

impl NodeData for CircleData {
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
            NetworkResult::extract_ivec2,
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
            NetworkResult::extract_int,
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

        // Convert to 2D real-space coordinates using the effective unit cell.
        let uc = &drawing_plane.effective_unit_cell;
        let real_center = uc.ivec2_lattice_to_real(&center);

        // For radius <= 0 keep today's Euclidean emission verbatim: the fractional
        // disk |u - c₀| <= r is a single point for r == 0 (the radius-0 circle) and
        // empty for r < 0 (an everywhere-positive SDF). Building `basis = r·L₂` for a
        // negative radius would silently flip "empty" into a full-size shape, since
        // neither the circular-basis snap nor the ellipse membership test can see the
        // sign of the basis columns.
        let geo_tree_root = if radius <= 0 {
            GeoNode::circle(real_center, radius as f64 * uc.a.length())
        } else {
            // The lattice image of a circle: the disk in fractional coordinates
            // {u : |u - c₀| <= r} mapped through the drawing plane's effective 2×2
            // cell. On (near-)square cells the constructor's circular-basis fast path
            // snaps this back to a plain Circle, so the square case stays
            // byte-identical to today. `a`/`b` are embedded into the plane the same
            // way as `dvec2_lattice_to_real` (drop the z component).
            let a2 = DVec2::new(uc.a.x, uc.a.y);
            let b2 = DVec2::new(uc.b.x, uc.b.y);
            let basis = DMat2::from_cols(a2, b2) * (radius as f64);
            GeoNode::ellipse(real_center, basis)
        };

        EvalOutput::single(NetworkResult::Geometry2D(GeometrySummary2D {
            drawing_plane,
            frame_transform: Transform2D::new(real_center, 0.0),
            geo_tree_root,
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
                "c: ({},{}) r: {}",
                self.center.x, self.center.y, self.radius
            )),
            (true, false) => Some(format!("c: ({},{})", self.center.x, self.center.y)),
            (false, true) => Some(format!("r: {}", self.radius)),
            (false, false) => None,
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("center".to_string(), TextValue::IVec2(self.center)),
            ("radius".to_string(), TextValue::Int(self.radius)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("center") {
            self.center = v
                .as_ivec2()
                .ok_or_else(|| "center must be an IVec2".to_string())?;
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

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "circle".to_string(),
        description: "Outputs the lattice image of a circle: integer center and \
            radius in lattice cells; an ellipse on non-square effective cells."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::Geometry2D,
        parameters: vec![
            Parameter {
                id: None,
                name: "center".to_string(),
                data_type: DataType::IVec2,
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
        output_pins: OutputPinDefinition::single(DataType::Geometry2D),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || {
            Box::new(CircleData {
                center: IVec2::new(0, 0),
                radius: 1,
            })
        },
        node_data_saver: generic_node_data_saver::<CircleData>,
        node_data_loader: generic_node_data_loader::<CircleData>,
    }
}
