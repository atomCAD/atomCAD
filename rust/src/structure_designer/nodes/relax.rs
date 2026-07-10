use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure_diff::extract_diff;
use crate::crystolecule::simulation::minimize_energy;
use crate::crystolecule::simulation::uff::VdwMode;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{MoleculeData, NetworkResult};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelaxData {
    /// Prune threshold (Å) for the `diff` output pin: an atom whose position
    /// moved by no more than this is treated as untouched and omitted from the
    /// diff. Default `0.0` = exact (every nudged atom is included). Pruning
    /// makes "apply the diff" differ from "relax directly" by up to this per
    /// atom. See `doc/design_diff_outputs_for_atom_ops.md` §2.2.
    #[serde(default)]
    pub diff_min_move: f64,
}

#[derive(Debug, Clone)]
pub struct RelaxEvalCache {
    pub relax_message: String,
}

impl NodeData for RelaxData {
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
            // Propagate the error on both pins (result + diff). Unlike
            // `EvalOutput::single`, this keeps diff consumers from silently
            // seeing `None` on pin 1.
            return EvalOutput::multi(vec![input_val.clone(), input_val]);
        }

        let mut wrapper = match input_val {
            NetworkResult::Crystal(_) | NetworkResult::Molecule(_) => input_val,
            other => {
                let err = NetworkResult::Error(format!(
                    "relax: expected atomic input, got {:?}",
                    other.infer_data_type()
                ));
                return EvalOutput::multi(vec![err.clone(), err]);
            }
        };

        let atoms_ref = match &mut wrapper {
            NetworkResult::Crystal(c) => &mut c.atoms,
            NetworkResult::Molecule(m) => &mut m.atoms,
            _ => unreachable!(),
        };

        // Snapshot the pre-minimization atoms so we can extract the diff after.
        // Frozen atoms are held exactly fixed by `minimize_energy`, so they fall
        // out of the id-keyed diff for free (§1.2).
        let before = atoms_ref.clone();

        let vdw_mode = if context.use_vdw_cutoff {
            VdwMode::Cutoff(6.0)
        } else {
            VdwMode::AllPairs
        };
        match minimize_energy(atoms_ref, vdw_mode) {
            Ok(result) => {
                // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
                // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
                if network_stack.len() == 1 {
                    let eval_cache = RelaxEvalCache {
                        relax_message: result.message.clone(),
                    };
                    context.selected_node_eval_cache = Some(Box::new(eval_cache));
                }

                let mut diff = extract_diff(&before, atoms_ref, self.diff_min_move);
                diff.decorator_mut().show_anchor_arrows = true;

                EvalOutput::multi(vec![
                    wrapper, // pin 0: relaxed structure, phase preserved
                    NetworkResult::Molecule(MoleculeData {
                        atoms: diff,
                        geo_tree_root: None,
                    }),
                ])
            }
            Err(error_msg) => {
                let err = NetworkResult::Error(error_msg);
                EvalOutput::multi(vec![err.clone(), err])
            }
        }
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
        let mut m = std::collections::HashMap::new();
        m.insert("molecule".to_string(), (true, None)); // required
        m
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![(
            "diff_min_move".to_string(),
            TextValue::Float(self.diff_min_move),
        )]
    }

    fn set_text_properties(
        &mut self,
        props: &std::collections::HashMap<String, TextValue>,
    ) -> Result<(), String> {
        if let Some(v) = props.get("diff_min_move") {
            self.diff_min_move = v.as_float().ok_or("diff_min_move must be a float")?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "relax".to_string(),
        description: "Relaxes an atomic structure toward a local energy minimum using the \
            UFF (Universal Force Field). Accepts a `Crystal` or `Molecule` and returns the \
            same kind with atom positions adjusted; bonds and elements are unchanged. Atoms \
            marked **frozen** are held fixed and act as boundary constraints for the rest.\n\
            \n\
            The `diff` output pin exposes the relaxation as a diff (the moved atoms only, \
            frozen atoms excluded) that can be composed with `atom_composediff` / `sequence` \
            and re-applied to another structure via `apply_diff`. The `diff_min_move` property \
            (Å) prunes atoms that moved less than the threshold from the diff (default `0.0` = \
            exact)."
            .to_string(),
        summary: Some("UFF energy minimization".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![Parameter {
            id: None,
            name: "molecule".to_string(),
            data_type: DataType::HasAtoms,
        }],
        output_pins: vec![
            // Keep relax's existing pin-0 resolution (plain same_as_input, no
            // disconnected fallback) — just split into the two-element vec.
            OutputPinDefinition::same_as_input("result", "molecule"),
            // The diff is always a free-floating Molecule regardless of the
            // input phase (matches atom_edit / atom_composediff conventions).
            OutputPinDefinition::fixed("diff", DataType::Molecule),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(RelaxData { diff_min_move: 0.0 }),
        node_data_saver: generic_node_data_saver::<RelaxData>,
        node_data_loader: generic_node_data_loader::<RelaxData>,
    }
}
