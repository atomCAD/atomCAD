use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure_diff;
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
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Data structure for the atom_composediff node.
/// Composes multiple atomic diffs into a single diff that produces the same
/// result as applying each input diff in sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomComposeDiffData {
    pub tolerance: f64,
    pub error_on_stale: bool,
}

impl NodeData for AtomComposeDiffData {
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
        // 1. Evaluate diffs array (pin 0, required)
        let diffs_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        if let NetworkResult::Error(_) = diffs_val {
            return EvalOutput::single(diffs_val);
        }

        // 2. Extract the array elements
        let diffs_array = if let NetworkResult::Array(array_elements) = diffs_val {
            array_elements
        } else {
            return EvalOutput::single(NetworkResult::Error(
                "atom_composediff: 'diffs' input must be an array of atomic structures".to_string(),
            ));
        };

        // 3. Edge case: empty input
        if diffs_array.is_empty() {
            return EvalOutput::single(NetworkResult::Error(
                "atom_composediff: at least one diff required".to_string(),
            ));
        }

        // 4. Preserve the concrete variant of the first element (per OQ1, all
        //    elements are the same variant after validation) and extract atoms.
        let mut iter = diffs_array.into_iter();
        let mut output_wrapper = iter.next().expect("non-empty checked above");
        if !matches!(
            output_wrapper,
            NetworkResult::Crystal(_) | NetworkResult::Molecule(_)
        ) {
            return EvalOutput::single(NetworkResult::Error(
                "atom_composediff: input 0 is not an atomic structure".to_string(),
            ));
        }

        let mut owned_diffs = Vec::new();
        {
            let first_atoms = match &output_wrapper {
                NetworkResult::Crystal(c) => &c.atoms,
                NetworkResult::Molecule(m) => &m.atoms,
                _ => unreachable!(),
            };
            if !first_atoms.is_diff() {
                return EvalOutput::single(NetworkResult::Error(
                    "atom_composediff: input 0 is not a diff structure (is_diff = false)"
                        .to_string(),
                ));
            }
        }

        for (i, item) in iter.enumerate() {
            debug_assert_eq!(
                item.infer_data_type(),
                output_wrapper.infer_data_type(),
                "atom_composediff: mixed-phase array reached evaluator"
            );
            let atoms = match item {
                NetworkResult::Crystal(c) => c.atoms,
                NetworkResult::Molecule(m) => m.atoms,
                _ => {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "atom_composediff: input {} is not an atomic structure",
                        i + 1
                    )));
                }
            };
            if !atoms.is_diff() {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "atom_composediff: input {} is not a diff structure (is_diff = false)",
                    i + 1
                )));
            }
            owned_diffs.push(atoms);
        }

        // 5. Single diff: return the first wrapper unchanged.
        if owned_diffs.is_empty() {
            return EvalOutput::single(output_wrapper);
        }

        // 6. Get tolerance from pin 1 or property
        let tolerance = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.tolerance,
            NetworkResult::extract_float,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // 7. Compose diffs (first element's atoms + all subsequent atoms)
        let first_atoms_ref = match &output_wrapper {
            NetworkResult::Crystal(c) => &c.atoms,
            NetworkResult::Molecule(m) => &m.atoms,
            _ => unreachable!(),
        };
        let mut diff_refs: Vec<&_> = vec![first_atoms_ref];
        diff_refs.extend(owned_diffs.iter());

        match atomic_structure_diff::compose_diffs(&diff_refs, tolerance) {
            Some(result) => {
                // 8. Optionally check stats for stale entries
                if self.error_on_stale && result.stats.cancellations > 0 {
                    // Cancellations aren't errors per se, but stale entries could be flagged
                    // For now, error_on_stale is reserved for future use
                }
                let mut composed = result.composed;
                composed.decorator_mut().show_anchor_arrows = true;
                match &mut output_wrapper {
                    NetworkResult::Crystal(c) => c.atoms = composed,
                    NetworkResult::Molecule(m) => m.atoms = composed,
                    _ => unreachable!(),
                }
                EvalOutput::single(output_wrapper)
            }
            None => EvalOutput::single(NetworkResult::Error(
                "atom_composediff: composition failed".to_string(),
            )),
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        if connected_input_pins.contains("tolerance") {
            return None;
        }
        Some(format!("tol={:.3}", self.tolerance))
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("tolerance".to_string(), TextValue::Float(self.tolerance)),
            (
                "error_on_stale".to_string(),
                TextValue::Bool(self.error_on_stale),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("tolerance") {
            self.tolerance = v
                .as_float()
                .ok_or_else(|| "tolerance must be a Float".to_string())?;
        }
        if let Some(v) = props.get("error_on_stale") {
            self.error_on_stale = v
                .as_bool()
                .ok_or_else(|| "error_on_stale must be a Bool".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("diffs".to_string(), (true, None)); // required
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "atom_composediff".to_string(),
        description: "Composes multiple atomic diffs into a single diff. \
            The composed diff, when applied to a base structure, produces the same result \
            as applying each input diff in sequence."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "diffs".to_string(),
                data_type: DataType::Array(Box::new(DataType::HasAtoms)),
            },
            Parameter {
                id: None,
                name: "tolerance".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: OutputPinDefinition::single_same_as_array_elements("diffs"),
        public: true,
        node_data_creator: || {
            Box::new(AtomComposeDiffData {
                tolerance: 0.1,
                error_on_stale: false,
            })
        },
        node_data_saver: generic_node_data_saver::<AtomComposeDiffData>,
        node_data_loader: generic_node_data_loader::<AtomComposeDiffData>,
    }
}
