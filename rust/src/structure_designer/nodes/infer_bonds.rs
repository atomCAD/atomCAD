use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure_utils::{
    auto_create_bonds_with_tolerance, auto_create_bonds_with_tolerance_filtered,
};
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
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferBondsData {
    pub additive: bool,
    pub bond_tolerance: f64,
}

impl Default for InferBondsData {
    fn default() -> Self {
        Self {
            additive: false,
            bond_tolerance: 1.15,
        }
    }
}

impl NodeData for InferBondsData {
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

        let additive =
            match network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1) {
                NetworkResult::Bool(b) => b,
                _ => self.additive,
            };

        let bond_tolerance =
            match network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 2) {
                NetworkResult::Float(f) => f,
                _ => self.bond_tolerance,
            };

        // Optional `region` pin (param index 3). Disconnected → operate on every
        // atom (today's behavior). Connected → only bonds touching at least one
        // in-region atom are (re)inferred; bonds between two out-of-region atoms
        // are left untouched (see design_blueprint_region_atom_edits.md §A4).
        let region_input =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 3);
        let region_geo = match region_input {
            NetworkResult::None => None,
            NetworkResult::Error(_) => return EvalOutput::single(region_input),
            NetworkResult::Blueprint(bp) => Some(bp.geo_tree_root),
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "infer_bonds.region: expected Blueprint, got {:?}",
                    other.infer_data_type()
                )));
            }
        };
        let has_region = region_geo.is_some();

        EvalOutput::single(map_atomic_in_region(
            input_val,
            region_geo.as_ref(),
            DEFAULT_REGION_MARGIN,
            move |mut result, in_region| {
                // With a region wired, we must additionally preserve bonds whose
                // both endpoints are out-of-region (they are "untouched"), so we
                // always need the existing-bond snapshot in that case. In
                // additive mode we also restore touched bonds the inference
                // didn't recreate.
                let need_existing = additive || has_region;
                let existing_bonds: Vec<(u32, u32, u8)> = if need_existing {
                    let mut bonds = Vec::new();
                    let atom_ids: Vec<u32> = result.atom_ids().copied().collect();
                    for atom_id in atom_ids {
                        if let Some(atom) = result.get_atom(atom_id) {
                            for bond in atom.bonds.iter() {
                                if bond.is_delete_marker() {
                                    continue;
                                }
                                let other_id = bond.other_atom_id();
                                // Only record each bond once (smaller id first)
                                if atom_id < other_id {
                                    bonds.push((atom_id, other_id, bond.bond_order()));
                                }
                            }
                        }
                    }
                    bonds
                } else {
                    Vec::new()
                };
                // Always clear and re-infer. With a region, the filtered variant
                // only creates bonds touching an in-region atom.
                result.clear_all_bonds();
                if has_region {
                    auto_create_bonds_with_tolerance_filtered(
                        &mut result,
                        bond_tolerance,
                        in_region,
                    );
                } else {
                    auto_create_bonds_with_tolerance(&mut result, bond_tolerance);
                }
                // Restore bonds the inference dropped:
                // - untouched bonds (both endpoints out-of-region) are always
                //   restored — the region never disturbs them;
                // - touched bonds are restored only in additive mode.
                // With no region, `in_region` is always-true, so every bond is
                // "touched" and this reduces to the original additive behavior.
                for (id1, id2, order) in existing_bonds {
                    let touched = in_region(id1) || in_region(id2);
                    let restore = if touched { additive } else { true };
                    if restore && !result.has_bond_between(id1, id2) {
                        result.add_bond(id1, id2, order);
                    }
                }
                result
            },
        ))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let mut parts = Vec::new();
        if self.additive && !connected_input_pins.contains("additive") {
            parts.push("additive".to_string());
        }
        if !connected_input_pins.contains("bond_tolerance") {
            parts.push(format!("tolerance: {:.2}", self.bond_tolerance));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(", "))
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        let mut props = Vec::new();
        if self.additive {
            props.push(("additive".to_string(), TextValue::Bool(true)));
        }
        if (self.bond_tolerance - 1.15).abs() > 1e-10 {
            props.push((
                "bond_tolerance".to_string(),
                TextValue::Float(self.bond_tolerance),
            ));
        }
        props
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("additive") {
            self.additive = v
                .as_bool()
                .ok_or_else(|| "additive must be a bool".to_string())?;
        }
        if let Some(v) = props.get("bond_tolerance") {
            self.bond_tolerance = v
                .as_float()
                .ok_or_else(|| "bond_tolerance must be a float".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> std::collections::HashMap<String, (bool, Option<String>)> {
        let mut m = std::collections::HashMap::new();
        m.insert("molecule".to_string(), (true, None));
        m.insert("region".to_string(), (false, None)); // optional
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "infer_bonds".to_string(),
        description: "Infers covalent bonds between atoms based on interatomic distances \
                      and covalent radii, scaled by a tolerance multiplier."
            .to_string(),
        summary: Some("Infer bonds from distances".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::HasAtoms,
            },
            Parameter {
                id: None,
                name: "additive".to_string(),
                data_type: DataType::Bool,
            },
            Parameter {
                id: None,
                name: "bond_tolerance".to_string(),
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
        node_data_creator: || Box::new(InferBondsData::default()),
        node_data_saver: generic_node_data_saver::<InferBondsData>,
        node_data_loader: generic_node_data_loader::<InferBondsData>,
    }
}
