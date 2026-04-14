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

/// Data structure for the apply_diff node.
/// Applies a diff structure onto a base structure, producing the merged result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyDiffData {
    pub tolerance: f64,
    pub error_on_stale: bool,
}

impl NodeData for ApplyDiffData {
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
        // 1. Evaluate base (pin 0, required)
        let base_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = base_val {
            return EvalOutput::single(base_val);
        }

        // 2. Evaluate diff (pin 1, required)
        let diff_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 1);
        if let NetworkResult::Error(_) = diff_val {
            return EvalOutput::single(diff_val);
        }

        // 3. Extract atomic structures (preserve base variant for the output)
        let (mut base_wrapper, base) = match base_val {
            NetworkResult::Crystal(mut c) => {
                let atoms = std::mem::take(&mut c.atoms);
                (NetworkResult::Crystal(c), atoms)
            }
            NetworkResult::Molecule(mut m) => {
                let atoms = std::mem::take(&mut m.atoms);
                (NetworkResult::Molecule(m), atoms)
            }
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "apply_diff: 'base' input must be an atomic structure".to_string(),
                ));
            }
        };

        let diff = match diff_val {
            NetworkResult::Crystal(c) => c.atoms,
            NetworkResult::Molecule(m) => m.atoms,
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "apply_diff: 'diff' input must be an atomic structure".to_string(),
                ));
            }
        };

        // 4. Validate: diff must be a diff structure
        if !diff.is_diff() {
            return EvalOutput::single(NetworkResult::Error(
                "apply_diff: input on 'diff' pin is not a diff structure (is_diff = false)"
                    .to_string(),
            ));
        }

        // 5. Get tolerance from pin 2 or property (default from self.tolerance)
        let tolerance = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            self.tolerance,
            NetworkResult::extract_float,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // 6. Apply the diff
        let diff_result = atomic_structure_diff::apply_diff(&base, &diff, tolerance);

        // 7. Check error_on_stale
        if self.error_on_stale {
            let stats = &diff_result.stats;
            let stale_count = stats.orphaned_tracked_atoms
                + stats.unmatched_delete_markers
                + stats.orphaned_bonds;
            if stale_count > 0 {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "apply_diff: stale entries detected — {} orphaned tracked atom(s), \
                     {} unmatched delete marker(s), {} orphaned bond(s)",
                    stats.orphaned_tracked_atoms,
                    stats.unmatched_delete_markers,
                    stats.orphaned_bonds,
                )));
            }
        }

        // 8. Return the result, preserving the base input's variant (Crystal or Molecule)
        match &mut base_wrapper {
            NetworkResult::Crystal(c) => c.atoms = diff_result.result,
            NetworkResult::Molecule(m) => m.atoms = diff_result.result,
            _ => unreachable!(),
        }
        EvalOutput::single(base_wrapper)
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
        m.insert("base".to_string(), (true, None));
        m.insert("diff".to_string(), (true, None));
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "apply_diff".to_string(),
        description: "Applies an atomic diff structure onto a base structure.\n\
            The 'base' input is the original atomic structure. \
            The 'diff' input must be a diff structure (is_diff = true) that encodes \
            additions, deletions, and modifications. \
            Atoms are matched by position within the given tolerance."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "base".to_string(),
                data_type: DataType::Atomic,
            },
            Parameter {
                id: None,
                name: "diff".to_string(),
                data_type: DataType::Atomic,
            },
            Parameter {
                id: None,
                name: "tolerance".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: OutputPinDefinition::single_same_as("base"),
        public: true,
        node_data_creator: || {
            Box::new(ApplyDiffData {
                tolerance: 0.1,
                error_on_stale: false,
            })
        },
        node_data_saver: generic_node_data_saver::<ApplyDiffData>,
        node_data_loader: generic_node_data_loader::<ApplyDiffData>,
    }
}
