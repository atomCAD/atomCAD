//! `freeze` / `unfreeze` — region-gated atom-metadata edit nodes.
//!
//! Phase A3 of `doc/design_blueprint_region_atom_edits.md`. Both are
//! `HasAtoms`-polymorphic, stateless ops that flip the per-atom frozen flag
//! (`Atom` bit 2) on the atoms inside an optional `region: Blueprint` volume —
//! `freeze` sets it, `unfreeze` clears it. With the `region` pin disconnected
//! they act on **all** atoms (consistent with every other Part A op). They share
//! all of Part A's machinery (`map_atomic_in_region`), so "freeze region A then
//! region B" composes into the union, just like the other region-gated ops.
//!
//! The frozen flag is honored by the `relax` node via `minimize_energy`, which
//! holds frozen atoms fixed (see `crystolecule/simulation/mod.rs`).

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
use serde::{Deserialize, Serialize};

/// Shared `eval` body for both nodes. Reads the required atomic input (pin 0)
/// and the optional `region` Blueprint (pin 1), then sets the frozen flag to
/// `frozen` on every in-region atom.
fn eval_set_frozen<'a>(
    node_label: &str,
    frozen: bool,
    network_evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
) -> EvalOutput {
    let input_val =
        network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

    if let NetworkResult::Error(_) = input_val {
        return EvalOutput::single(input_val);
    }

    // Optional `region` pin (param index 1). Disconnected → flip every atom's
    // frozen flag. Connected → only in-region atoms (see
    // design_blueprint_region_atom_edits.md §A1/§A5).
    let region_input = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
    let region_geo = match region_input {
        NetworkResult::None => None,
        NetworkResult::Error(_) => return EvalOutput::single(region_input),
        NetworkResult::Blueprint(bp) => Some(bp.geo_tree_root),
        other => {
            return EvalOutput::single(NetworkResult::Error(format!(
                "{}.region: expected Blueprint, got {:?}",
                node_label,
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
                structure.set_atom_frozen(id, frozen);
            }
            structure
        },
    );
    EvalOutput::single(output)
}

fn frozen_parameter_metadata() -> std::collections::HashMap<String, (bool, Option<String>)> {
    let mut m = std::collections::HashMap::new();
    m.insert("molecule".to_string(), (true, None)); // required
    m.insert("region".to_string(), (false, None)); // optional
    m
}

fn frozen_parameters() -> Vec<Parameter> {
    vec![
        Parameter {
            id: None,
            name: "molecule".to_string(),
            data_type: DataType::HasAtoms,
        },
        Parameter {
            id: None,
            name: "region".to_string(),
            data_type: DataType::Blueprint,
        },
    ]
}

// ============================================================================
// freeze
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreezeData {}

impl NodeData for FreezeData {
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
        eval_set_frozen(
            "freeze",
            true,
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
        )
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
        frozen_parameter_metadata()
    }
}

pub fn freeze_get_node_type() -> NodeType {
    NodeType {
        name: "freeze".to_string(),
        description: "Marks atoms as frozen so the relax node holds them fixed. \
                      With a region connected, only atoms inside the region volume \
                      are frozen; otherwise all atoms are frozen."
            .to_string(),
        summary: Some("Freeze atoms".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: frozen_parameters(),
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(FreezeData {}),
        node_data_saver: generic_node_data_saver::<FreezeData>,
        node_data_loader: generic_node_data_loader::<FreezeData>,
    }
}

// ============================================================================
// unfreeze
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnfreezeData {}

impl NodeData for UnfreezeData {
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
        eval_set_frozen(
            "unfreeze",
            false,
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
        )
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
        frozen_parameter_metadata()
    }
}

pub fn unfreeze_get_node_type() -> NodeType {
    NodeType {
        name: "unfreeze".to_string(),
        description: "Clears the frozen flag on atoms so the relax node can move them. \
                      With a region connected, only atoms inside the region volume \
                      are unfrozen; otherwise all atoms are unfrozen."
            .to_string(),
        summary: Some("Unfreeze atoms".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: frozen_parameters(),
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(UnfreezeData {}),
        node_data_saver: generic_node_data_saver::<UnfreezeData>,
        node_data_loader: generic_node_data_loader::<UnfreezeData>,
    }
}
