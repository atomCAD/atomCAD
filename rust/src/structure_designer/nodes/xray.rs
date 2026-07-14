//! `xray` — per-region semi-transparent atom display node.
//!
//! Phase 2 of `doc/design_xray_node.md`. A `HasAtoms`-polymorphic,
//! metadata-only pass-through (like `freeze`/`unfreeze`) that records a
//! display alpha on every atom inside an optional `region: Blueprint` volume
//! (all atoms when disconnected). `alpha == 1.0` removes the recording
//! (restores opacity), so chained `xray` nodes compose last-writer-wins.
//! The alpha is consumed by the impostor renderer (design Phases 3–5); in
//! `TriangleMesh` mode atoms render opaque (documented limitation).

use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::lattice_fill::DEFAULT_REGION_MARGIN;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::atom_op::map_atomic_in_region;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

fn default_alpha() -> f64 {
    0.5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XrayData {
    /// Display alpha applied to in-region atoms. Wired `alpha` pin overrides
    /// this stored value; clamped to `[0.0, 1.0]` at eval. `1.0` restores
    /// full opacity (removes the per-atom recording).
    #[serde(default = "default_alpha")]
    pub alpha: f64,
}

impl NodeData for XrayData {
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
        let input_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        if let NetworkResult::Error(_) = input_val {
            return EvalOutput::single(input_val);
        }

        // Alpha: wired pin 1 overrides the stored property; clamp to [0, 1].
        let alpha = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.alpha,
            NetworkResult::extract_float,
        ) {
            Ok(value) => value.clamp(0.0, 1.0),
            Err(error) => return EvalOutput::single(error),
        };

        // Optional `region` pin (param index 2). Disconnected → record the
        // alpha on every atom. Connected → only in-region atoms.
        let region_input =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 2);
        let region_geo = match region_input {
            NetworkResult::None => None,
            NetworkResult::Error(_) => return EvalOutput::single(region_input),
            NetworkResult::Blueprint(bp) => Some(bp.geo_tree_root),
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "xray.region: expected Blueprint, got {:?}",
                    other.infer_data_type()
                )));
            }
        };

        let output = map_atomic_in_region(
            input_val,
            region_geo.as_ref(),
            DEFAULT_REGION_MARGIN,
            |mut structure, in_region| {
                let ids: Vec<u32> = structure
                    .iter_atoms()
                    .filter(|(atom_id, _)| in_region(**atom_id))
                    .map(|(atom_id, _)| *atom_id)
                    .collect();
                for id in ids {
                    structure.set_atom_alpha(id, alpha as f32);
                }
                structure
            },
        );
        EvalOutput::single(output)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        if connected_input_pins.contains("alpha") {
            return None;
        }
        Some(format!("α = {:.2}", self.alpha))
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("molecule".to_string(), (true, None)); // required
        m.insert("alpha".to_string(), (false, None)); // optional
        m.insert("region".to_string(), (false, None)); // optional
        m
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![("alpha".to_string(), TextValue::Float(self.alpha))]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("alpha") {
            self.alpha = v.as_float().ok_or("alpha must be a float")?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "xray".to_string(),
        description: "Makes atoms semi-transparent in the viewport so internal features \
                      show through. With a region connected, only atoms inside the region \
                      volume are affected; otherwise all atoms are. The alpha (0 = invisible, \
                      1 = opaque) comes from the wired `alpha` pin or the stored property; \
                      `1.0` restores full opacity, so chained xray nodes compose \
                      last-writer-wins. Transparency renders in impostor atomic rendering \
                      mode only; in triangle-mesh mode atoms stay opaque."
            .to_string(),
        summary: Some("Ghost atoms semi-transparent".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::HasAtoms,
            },
            Parameter {
                id: None,
                name: "alpha".to_string(),
                data_type: DataType::Float,
            },
            Parameter {
                id: None,
                name: "region".to_string(),
                data_type: DataType::Blueprint,
            },
        ],
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(XrayData { alpha: 0.5 }),
        node_data_saver: generic_node_data_saver::<XrayData>,
        node_data_loader: generic_node_data_loader::<XrayData>,
    }
}
