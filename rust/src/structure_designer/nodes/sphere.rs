use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::structure::Structure;
use crate::geo_tree::GeoNode;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::Alignment;
use crate::structure_designer::evaluator::network_result::BlueprintData;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::serialization_utils::ivec3_serializer;
use glam::DMat3;
use glam::i32::IVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SphereData {
    #[serde(with = "ivec3_serializer")]
    pub center: IVec3,
    pub radius: i32,
}

impl NodeData for SphereData {
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
            NetworkResult::extract_ivec3,
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

        let structure = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            Structure::diamond(),
            NetworkResult::extract_structure,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let l = &structure.lattice_vecs;
        let real_center = l.ivec3_lattice_to_real(&center);

        // For radius <= 0 keep today's Euclidean emission verbatim: the fractional
        // ball |u - c₀| <= r is a single point for r == 0 (the radius-0 sphere) and
        // empty for r < 0 (an everywhere-positive SDF). Building `basis = r·L` for a
        // negative radius would silently flip "empty" into a full-size shape, since
        // neither the spherical-basis snap nor the ellipsoid membership test can see
        // the sign of the basis columns.
        let geo_tree_root = if radius <= 0 {
            GeoNode::sphere(real_center, radius as f64 * l.a.length())
        } else {
            // The lattice image of a sphere: the ball in fractional coordinates
            // {u : |u - c₀| <= r} mapped through the lattice matrix. On (near-)cubic
            // cells the constructor's spherical-basis fast path snaps this back to a
            // plain Sphere, so the cubic case stays byte-identical to today.
            let basis = DMat3::from_cols(l.a, l.b, l.c) * (radius as f64);
            GeoNode::ellipsoid(real_center, basis)
        };

        EvalOutput::single(NetworkResult::Blueprint(BlueprintData {
            structure,
            geo_tree_root,
            alignment: Alignment::Aligned,
            alignment_reason: None,
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
                "c: ({},{},{}) r: {}",
                self.center.x, self.center.y, self.center.z, self.radius
            )),
            (true, false) => Some(format!(
                "c: ({},{},{})",
                self.center.x, self.center.y, self.center.z
            )),
            (false, true) => Some(format!("r: {}", self.radius)),
            (false, false) => None,
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("center".to_string(), TextValue::IVec3(self.center)),
            ("radius".to_string(), TextValue::Int(self.radius)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("center") {
            self.center = v
                .as_ivec3()
                .ok_or_else(|| "center must be an IVec3".to_string())?;
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
        m.insert(
            "structure".to_string(),
            (false, Some("diamond".to_string())),
        );
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "sphere".to_string(),
        description: "Outputs the lattice image of a sphere: integer center and \
            radius in lattice cells; an ellipsoid on non-cubic cells."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::Geometry3D,
        parameters: vec![
            Parameter {
                id: None,
                name: "center".to_string(),
                data_type: DataType::IVec3,
            },
            Parameter {
                id: None,
                name: "radius".to_string(),
                data_type: DataType::Int,
            },
            Parameter {
                id: None,
                name: "structure".to_string(),
                data_type: DataType::Structure,
            },
        ],
        output_pins: OutputPinDefinition::single(DataType::Blueprint),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || {
            Box::new(SphereData {
                center: IVec3::new(0, 0, 0),
                radius: 1,
            })
        },
        node_data_saver: generic_node_data_saver::<SphereData>,
        node_data_loader: generic_node_data_loader::<SphereData>,
    }
}
