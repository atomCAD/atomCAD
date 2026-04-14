use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure_utils::auto_create_bonds_with_tolerance;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::atom_op::map_atomic;
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

        EvalOutput::single(map_atomic(input_val, |mut result| {
            // Save existing bonds if in additive mode
            let existing_bonds: Vec<(u32, u32, u8)> = if additive {
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
            // Always clear and re-infer
            result.clear_all_bonds();
            auto_create_bonds_with_tolerance(&mut result, bond_tolerance);
            // Re-add existing bonds that weren't inferred
            for (id1, id2, order) in existing_bonds {
                if !result.has_bond_between(id1, id2) {
                    result.add_bond(id1, id2, order);
                }
            }
            result
        }))
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
                data_type: DataType::Atomic,
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
        ],
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        public: true,
        node_data_creator: || Box::new(InferBondsData::default()),
        node_data_saver: generic_node_data_saver::<InferBondsData>,
        node_data_loader: generic_node_data_loader::<InferBondsData>,
    }
}
