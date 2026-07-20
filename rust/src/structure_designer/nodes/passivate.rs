use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_constants::{ALLOWED_PASSIVANTS, ATOM_INFO, is_allowed_passivant};
use crate::crystolecule::hydrogen_passivation::{AddHydrogensOptions, add_hydrogens_filtered};
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

/// Default passivation element: hydrogen. Used by serde so old files (data
/// `{}`) and freshly-added nodes both resolve to H.
fn default_element() -> i16 {
    1
}

/// Node data for the `passivate` node (né `add_hydrogen`, issue #405). Places a
/// terminating atom at each open valence slot. The terminator element is
/// hydrogen by default; halogens (F/Cl/Br/I) are placed at the correct
/// host–halogen bond length so no relax pass is needed. See
/// `doc/design_halogen_passivation.md` D3/D4.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassivateData {
    /// Atomic number of the terminating element. Restricted to the monovalent
    /// passivant set `{1, 9, 17, 35, 53}` (validated at eval time, D1).
    /// serde-default `1` (hydrogen) so pre-halogen files load unchanged.
    #[serde(default = "default_element")]
    pub element: i16,
}

impl Default for PassivateData {
    fn default() -> Self {
        Self {
            element: default_element(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PassivateEvalCache {
    pub message: String,
}

/// The symbol shown in the subtitle / eval-cache message for an atomic number.
fn element_symbol(atomic_number: i16) -> String {
    ATOM_INFO
        .get(&(atomic_number as i32))
        .map(|info| info.symbol.clone())
        .unwrap_or_else(|| atomic_number.to_string())
}

impl NodeData for PassivateData {
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

        // Optional `region` pin (param index 1). Disconnected → passivate every
        // atom. Connected → only in-region host atoms are passivated (see
        // design_blueprint_region_atom_edits.md §A1/§A4).
        let region_input =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        let region_geo = match region_input {
            NetworkResult::None => None,
            NetworkResult::Error(_) => return EvalOutput::single(region_input),
            NetworkResult::Blueprint(bp) => Some(bp.geo_tree_root),
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "passivate.region: expected Blueprint, got {:?}",
                    other.infer_data_type()
                )));
            }
        };

        // Optional `element` pin (param index 2). Wired overrides the stored
        // property (evaluate_or_default precedence, D4).
        let element = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            self.element as i32,
            NetworkResult::extract_int,
        ) {
            Ok(value) => value as i16,
            Err(error) => return EvalOutput::single(error),
        };

        // D1: reject any non-monovalent passivant with a localized error naming
        // the allowed set (no validator rule — the eval error is the surfacing).
        if !is_allowed_passivant(element) {
            return EvalOutput::single(NetworkResult::Error(format!(
                "passivate.element: {} is not an allowed passivant; expected one of {:?} (H/F/Cl/Br/I)",
                element, ALLOWED_PASSIVANTS
            )));
        }

        let at_root = network_stack.len() == 1;
        let output = map_atomic_in_region(
            input_val,
            region_geo.as_ref(),
            DEFAULT_REGION_MARGIN,
            |mut structure, in_region| {
                let options = AddHydrogensOptions {
                    selected_only: false,
                    skip_already_passivated: true,
                    passivant_element: element,
                };
                let result = add_hydrogens_filtered(&mut structure, &options, in_region);

                if at_root {
                    context.selected_node_eval_cache = Some(Box::new(PassivateEvalCache {
                        message: format!(
                            "Added {} {} atoms",
                            result.atoms_added,
                            element_symbol(element)
                        ),
                    }));
                }
                structure
            },
        );
        EvalOutput::single(output)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        // A wired `element` pin overrides the stored value; suppress the
        // subtitle so it never reads a stale stored element (matches
        // atom_replace's wired-`rules` convention).
        if connected_input_pins.contains("element") {
            return None;
        }
        // Hydrogen is the unremarkable default — show the symbol only when the
        // node passivates with something else, so the network reads correctly
        // at a glance (D3).
        if self.element == 1 {
            None
        } else {
            Some(element_symbol(self.element))
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![("element".to_string(), TextValue::Int(self.element as i32))]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("element") {
            self.element = v
                .as_int()
                .ok_or_else(|| "element must be an integer".to_string())?
                as i16;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> std::collections::HashMap<String, (bool, Option<String>)> {
        let mut m = std::collections::HashMap::new();
        m.insert("molecule".to_string(), (true, None)); // required
        m.insert("region".to_string(), (false, None)); // optional
        m.insert("element".to_string(), (false, None)); // optional
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "passivate".to_string(),
        description: "Passivates undersaturated atoms by placing a terminating \
                       atom (hydrogen by default, or a halogen F/Cl/Br/I) at each \
                       open valence slot, at the correct host–terminator bond length."
            .to_string(),
        summary: Some("Cap open bonds (H/halogen)".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
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
            // Appended last: arguments are positional, so appending keeps
            // existing `[molecule, region]` wires valid. This knowingly breaks
            // the region-gated-op "region is last" convention (D4) — positional
            // wire compatibility wins.
            Parameter {
                id: None,
                name: "element".to_string(),
                data_type: DataType::Int,
            },
        ],
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(PassivateData::default()),
        node_data_saver: generic_node_data_saver::<PassivateData>,
        node_data_loader: generic_node_data_loader::<PassivateData>,
    }
}
