//! The `free_sphere` node: the **real-space (Å) float** analog of `sphere`
//! (issue #381, `doc/design_free_sphere_circle.md`).
//!
//! `center: Vec3` (`DVec3`, `dvec3_serializer`) + `radius: f64`, defaulting to
//! `ZERO` / `5.0`. This file mirrors `sphere.rs` **minus** the lattice→real
//! conversion (`ivec3_lattice_to_real` / `effective_unit_cell`): the geo_tree is
//! already real-space, so it builds `GeoNode::sphere` **directly** and stays
//! Euclidean — it is deliberately *not* touched by the lattice-covariant
//! semantics `sphere.rs` carries.
//!
//! It takes the same optional `structure` input (default diamond, for the
//! downstream `materialize`) and emits `BlueprintData { alignment:
//! Alignment::Aligned, .. }` — a fractional *cutter* does **not** taint
//! alignment (design §3.5, the opposite of `free_move`'s conservative taint).
//! There is **no gadget**, for parity with `sphere`. Text properties use
//! `as_vec3` / `as_float`, which accept whole-number `IVec3` from the parser.
//!
//! The API + FRB + Flutter editor plumbing mirrors `free_move`; the editor is
//! `lib/structure_designer/node_data/free_sphere_editor.dart`.

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
use atomcad_util::serialization_utils::dvec3_serializer;
use glam::f64::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeSphereData {
    #[serde(with = "dvec3_serializer")]
    pub center: DVec3,
    pub radius: f64,
}

impl NodeData for FreeSphereData {
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
            NetworkResult::extract_vec3,
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

        // No lattice→real conversion: center and radius are already real-space (Å).
        EvalOutput::single(NetworkResult::Blueprint(BlueprintData {
            structure,
            geo_tree_root: GeoNode::sphere(center, radius),
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
                "c: ({:.2}, {:.2}, {:.2}) r: {:.2}",
                self.center.x, self.center.y, self.center.z, self.radius
            )),
            (true, false) => Some(format!(
                "c: ({:.2}, {:.2}, {:.2})",
                self.center.x, self.center.y, self.center.z
            )),
            (false, true) => Some(format!("r: {:.2}", self.radius)),
            (false, false) => None,
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("center".to_string(), TextValue::Vec3(self.center)),
            ("radius".to_string(), TextValue::Float(self.radius)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("center") {
            self.center = v
                .as_vec3()
                .ok_or_else(|| "center must be a Vec3".to_string())?;
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
        m.insert(
            "structure".to_string(),
            (false, Some("diamond".to_string())),
        );
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "free_sphere".to_string(),
        description: "Outputs a sphere with real-space (Å) center coordinates and radius — \
            the non-lattice-aligned analog of `sphere`."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::Geometry3D,
        parameters: vec![
            Parameter {
                id: None,
                name: "center".to_string(),
                data_type: DataType::Vec3,
            },
            Parameter {
                id: None,
                name: "radius".to_string(),
                data_type: DataType::Float,
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
            Box::new(FreeSphereData {
                center: DVec3::ZERO,
                radius: 5.0,
            })
        },
        node_data_saver: generic_node_data_saver::<FreeSphereData>,
        node_data_loader: generic_node_data_loader::<FreeSphereData>,
    }
}
