use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_constants::ATOM_INFO;
use crate::structure_designer::data_type::{DataType, RecordType};
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
use glam::IVec2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AtomReplaceData {
    /// List of (from_atomic_number, to_atomic_number) replacement rules.
    /// Each pair maps atoms of element `from` to element `to`.
    pub replacements: Vec<(i16, i16)>,
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

        // Optional `rules` pin (param index 1). Disconnected → fall back to
        // stored replacements. Connected → wired rules entirely replace the
        // stored list (see design_atom_replace_rules_input.md §4).
        let rules_input =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        let replacements = match rules_input {
            NetworkResult::None => self.replacements.clone(),
            NetworkResult::Error(_) => return EvalOutput::single(rules_input),
            NetworkResult::Array(items) => match parse_rules_from_records(items) {
                Ok(r) => r,
                Err(e) => return EvalOutput::single(NetworkResult::Error(e)),
            },
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "atom_replace.rules: expected Array[Record], got {:?}",
                    other.infer_data_type()
                )));
            }
        };

        EvalOutput::single(map_atomic(molecule_input_val, move |mut structure| {
            if replacements.is_empty() {
                return structure;
            }

            // Build lookup map (last rule wins for duplicate sources)
            let replacement_map: HashMap<i16, i16> = replacements.iter().copied().collect();

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

            structure
        }))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        // When the `rules` pin is wired the stored list is overridden;
        // suppress the subtitle entirely (project convention — the upstream
        // source carries its own subtitle).
        if connected_input_pins.contains("rules") {
            return None;
        }

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
        if let Some(v) = props.get("replacements")
            && let TextValue::Array(items) = v
        {
            self.replacements = items
                .iter()
                .map(|item| {
                    let iv = item.as_ivec2().ok_or("each replacement must be an IVec2")?;
                    Ok((iv.x as i16, iv.y as i16))
                })
                .collect::<Result<Vec<_>, String>>()?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("molecule".to_string(), (true, None));
        m.insert("rules".to_string(), (false, None));
        m
    }
}

/// Parse a runtime `Array[Record]` value into the `(from, to)` rule list.
/// Each record must carry `from` and `to` `Int` fields whose values fit in
/// `i16` and lie in `0..=118` (atomic-number range).
fn parse_rules_from_records(items: Vec<NetworkResult>) -> Result<Vec<(i16, i16)>, String> {
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.into_iter().enumerate() {
        let from = item
            .extract_record_field("from")
            .ok_or_else(|| format!("atom_replace.rules[{}]: missing 'from' field", i))?
            .clone()
            .extract_int()
            .ok_or_else(|| format!("atom_replace.rules[{}].from: not an Int", i))?;
        let to = item
            .extract_record_field("to")
            .ok_or_else(|| format!("atom_replace.rules[{}]: missing 'to' field", i))?
            .clone()
            .extract_int()
            .ok_or_else(|| format!("atom_replace.rules[{}].to: not an Int", i))?;
        out.push((narrow_source(from, i)?, narrow_target(to, i)?));
    }
    Ok(out)
}

/// Narrow a source-side atomic number. Allows the unchanged-marker sentinel
/// `-1` and the delete-marker `0` in addition to the real range — these are
/// silently filtered out at apply time, matching the stored-rule path.
fn narrow_source(value: i32, index: usize) -> Result<i16, String> {
    if !(-1..=118).contains(&value) {
        return Err(format!(
            "atom_replace.rules[{}].from: atomic number {} out of range (expected -1..=118)",
            index, value
        ));
    }
    Ok(value as i16)
}

/// Narrow a target-side atomic number. Strict `0..=118`; `0` is the delete
/// sentinel, anything else outside the table is a user-visible bug.
fn narrow_target(value: i32, index: usize) -> Result<i16, String> {
    if !(0..=118).contains(&value) {
        return Err(format!(
            "atom_replace.rules[{}].to: atomic number {} out of range (expected 0..=118)",
            index, value
        ));
    }
    Ok(value as i16)
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
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::HasAtoms,
            },
            Parameter {
                id: None,
                name: "rules".to_string(),
                data_type: DataType::Array(Box::new(DataType::Record(RecordType::Named(
                    "ElementMapping".to_string(),
                )))),
            },
        ],
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(AtomReplaceData::default()),
        node_data_saver: generic_node_data_saver::<AtomReplaceData>,
        node_data_loader: generic_node_data_loader::<AtomReplaceData>,
    }
}
