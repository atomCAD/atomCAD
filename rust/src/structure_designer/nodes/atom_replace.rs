use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_constants::ATOM_INFO;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::structure_designer::data_type::DataType;
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
use glam::IVec2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomReplaceData {
    /// List of (from_atomic_number, to_atomic_number) replacement rules.
    /// Each pair maps atoms of element `from` to element `to`.
    pub replacements: Vec<(i16, i16)>,
}

impl Default for AtomReplaceData {
    fn default() -> Self {
        Self {
            replacements: vec![],
        }
    }
}

fn element_symbol(atomic_number: i16) -> String {
    if atomic_number == 0 {
        return "(del)".to_string();
    }
    ATOM_INFO
        .get(&(atomic_number as i32))
        .map(|info| info.symbol.clone())
        .unwrap_or_else(|| atomic_number.to_string())
}

impl NodeData for AtomReplaceData {
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
        let molecule_input_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        if let NetworkResult::Error(_) = molecule_input_val {
            return EvalOutput::single(molecule_input_val);
        }

        if let NetworkResult::Atomic(mut structure) = molecule_input_val {
            if self.replacements.is_empty() {
                return EvalOutput::single(NetworkResult::Atomic(structure));
            }

            // Build lookup map (last rule wins for duplicate sources)
            let replacement_map: HashMap<i16, i16> = self.replacements.iter().copied().collect();

            // Collect atoms to delete and atoms to replace
            let mut atoms_to_delete = Vec::new();
            let mut atoms_to_replace = Vec::new();

            for (atom_id, atom) in structure.iter_atoms() {
                let source = atom.atomic_number;
                // Skip delete markers and unchanged markers
                if source == 0 || source == -1 {
                    continue;
                }
                if let Some(&target) = replacement_map.get(&source) {
                    if target == 0 {
                        atoms_to_delete.push(*atom_id);
                    } else {
                        atoms_to_replace.push((*atom_id, target));
                    }
                }
            }

            // Apply replacements
            for (atom_id, target) in atoms_to_replace {
                structure.set_atom_atomic_number(atom_id, target);
            }

            // Delete atoms (handles bond cleanup)
            for atom_id in atoms_to_delete {
                structure.delete_atom(atom_id);
            }

            EvalOutput::single(NetworkResult::Atomic(structure))
        } else {
            EvalOutput::single(NetworkResult::Atomic(AtomicStructure::new()))
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        if self.replacements.is_empty() {
            return Some("(no replacements)".to_string());
        }

        let max_display = 3;
        let displayed: Vec<String> = self
            .replacements
            .iter()
            .take(max_display)
            .map(|(from, to)| format!("{}→{}", element_symbol(*from), element_symbol(*to)))
            .collect();

        let remaining = self.replacements.len().saturating_sub(max_display);
        if remaining > 0 {
            Some(format!("{}, … (+{} more)", displayed.join(", "), remaining))
        } else {
            Some(displayed.join(", "))
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        if self.replacements.is_empty() {
            return vec![];
        }
        let items: Vec<TextValue> = self
            .replacements
            .iter()
            .map(|(from, to)| TextValue::IVec2(IVec2::new(*from as i32, *to as i32)))
            .collect();
        vec![("replacements".to_string(), TextValue::Array(items))]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("replacements") {
            if let TextValue::Array(items) = v {
                self.replacements = items
                    .iter()
                    .map(|item| {
                        let iv = item.as_ivec2().ok_or("each replacement must be an IVec2")?;
                        Ok((iv.x as i16, iv.y as i16))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
            }
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("molecule".to_string(), (true, None));
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "atom_replace".to_string(),
        description: "Replaces elements in an atomic structure. Define replacement rules \
                      mapping source elements to target elements. Atoms not matching any \
                      rule pass through unchanged."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![Parameter {
            id: None,
            name: "molecule".to_string(),
            data_type: DataType::Atomic,
        }],
        output_pins: OutputPinDefinition::single(DataType::Atomic),
        public: true,
        node_data_creator: || Box::new(AtomReplaceData::default()),
        node_data_saver: generic_node_data_saver::<AtomReplaceData>,
        node_data_loader: generic_node_data_loader::<AtomReplaceData>,
    }
}
