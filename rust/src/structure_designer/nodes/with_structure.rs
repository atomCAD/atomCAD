// WithStructure: Blueprint + Structure -> Blueprint. Replaces the `Structure`
// carried by a Blueprint value while preserving its geometry. Used (notably by
// the v2->v3 migration) to inject a patched Structure into a shape chain
// immediately before `materialize`. Blueprint-only by design: overriding the
// structure on a Crystal is not meaningful because its atoms are already
// materialized against a specific structure.
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, NetworkResult, runtime_type_error_in_input,
    worsen_alignment_with_reason,
};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WithStructureData {}

impl NodeData for WithStructureData {
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
        let shape_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = shape_val {
            return EvalOutput::single(shape_val);
        }
        let bp = match shape_val {
            NetworkResult::Blueprint(bp) => bp,
            _ => return EvalOutput::single(runtime_type_error_in_input(0)),
        };

        let structure_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 1);
        if let NetworkResult::Error(_) = structure_val {
            return EvalOutput::single(structure_val);
        }
        let structure = match structure_val {
            NetworkResult::Structure(s) => s,
            _ => return EvalOutput::single(runtime_type_error_in_input(1)),
        };

        // Alignment rule (see doc/design_blueprint_alignment.md §3.6, §10):
        // the geometry was authored against `bp.structure` but is now being
        // re-interpreted against `structure`. Whether that stays registered to
        // the lattice depends on which Structure fields changed.
        //
        // - All three fields approximately equal: no-op, pass-through.
        // - Only the motif differs (lattice_vecs and motif_offset equal):
        //   lattice lines are unchanged so atoms still land on lattice points,
        //   but the motif doesn't map to itself — `MotifUnaligned`.
        // - `lattice_vecs` or `motif_offset` changed: geometry is no longer
        //   registered to integer translations of the lattice
        //   (conservative rule per §10 motif_shift) — `LatticeUnaligned`.
        let lattice_vecs_changed = !bp
            .structure
            .lattice_vecs
            .is_approximately_equal(&structure.lattice_vecs);
        let motif_offset_changed = !bp
            .structure
            .motif_offset
            .abs_diff_eq(structure.motif_offset, 1e-9);
        let motif_changed = !bp
            .structure
            .motif
            .is_approximately_equal(&structure.motif, 1e-9);

        let mut alignment = bp.alignment;
        let mut alignment_reason = bp.alignment_reason;
        if lattice_vecs_changed || motif_offset_changed {
            worsen_alignment_with_reason(
                &mut alignment,
                &mut alignment_reason,
                Alignment::LatticeUnaligned,
                || {
                    "with_structure: replacement Structure differs in lattice_vecs or motif_offset; \
                     geometry is no longer registered to the lattice"
                        .to_string()
                },
            );
        } else if motif_changed {
            worsen_alignment_with_reason(
                &mut alignment,
                &mut alignment_reason,
                Alignment::MotifUnaligned,
                || {
                    "with_structure: replacement motif differs from input; \
                     lattice registration preserved, motif symmetry not"
                        .to_string()
                },
            );
        }

        EvalOutput::single(NetworkResult::Blueprint(BlueprintData {
            structure,
            geo_tree_root: bp.geo_tree_root,
            alignment,
            alignment_reason,
        }))
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
        m.insert("shape".to_string(), (true, None));
        m.insert("structure".to_string(), (true, None));
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "with_structure".to_string(),
        description: "Replaces the `Structure` carried by a Blueprint with the provided \
                      `Structure`, preserving the Blueprint's geometry. Blueprint-only: \
                      Crystal inputs are not accepted because a Crystal's atoms are already \
                      materialized against a specific structure."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::OtherBuiltin,
        parameters: vec![
            Parameter {
                id: None,
                name: "shape".to_string(),
                data_type: DataType::Blueprint,
            },
            Parameter {
                id: None,
                name: "structure".to_string(),
                data_type: DataType::Structure,
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Blueprint),
        public: true,
        node_data_creator: || Box::new(WithStructureData::default()),
        node_data_saver: generic_node_data_saver::<WithStructureData>,
        node_data_loader: generic_node_data_loader::<WithStructureData>,
    }
}
