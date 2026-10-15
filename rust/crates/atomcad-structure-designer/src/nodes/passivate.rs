use crate::data_type::DataType;
use crate::evaluator::atom_op::map_atomic_in_region;
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::atomic_constants::{ALLOWED_PASSIVANTS, ATOM_INFO, is_allowed_passivant};
use atomcad_crystolecule::hydrogen_passivation::{
    AddHydrogensOptions, DEFAULT_PASSIVATION_CLASH_FACTOR, add_hydrogens_filtered,
};
use atomcad_crystolecule::lattice_fill::DEFAULT_REGION_MARGIN;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default passivation element: hydrogen. Used by serde so old files (data
/// `{}`) and freshly-added nodes both resolve to H.
fn default_element() -> i16 {
    1
}

fn default_clash_factor() -> f64 {
    DEFAULT_PASSIVATION_CLASH_FACTOR
}

fn is_default_clash_factor(factor: &f64) -> bool {
    *factor == DEFAULT_PASSIVATION_CLASH_FACTOR
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
    /// Leave an open valence uncapped when its terminator would sit in a
    /// steric clash (`doc/design_chemisorb_mirrors_and_passivation.md` §5).
    /// Not written while off, so a node that never used the option saves
    /// exactly as before it existed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_clashes: bool,
    /// The clash rule's factor on the van der Waals sum; used only with
    /// `skip_clashes`, and must be > 0. Not written at its default.
    #[serde(
        default = "default_clash_factor",
        skip_serializing_if = "is_default_clash_factor"
    )]
    pub clash_factor: f64,
}

impl Default for PassivateData {
    fn default() -> Self {
        Self {
            element: default_element(),
            skip_clashes: false,
            clash_factor: default_clash_factor(),
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

        let clash = if self.skip_clashes {
            if !(self.clash_factor > 0.0 && self.clash_factor.is_finite()) {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "passivate.clash_factor: must be > 0, got {}",
                    self.clash_factor
                )));
            }
            Some(self.clash_factor)
        } else {
            None
        };

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
                    clash,
                };
                let result = add_hydrogens_filtered(&mut structure, &options, in_region);

                if at_root {
                    let mut message = format!(
                        "Added {} {} atoms",
                        result.atoms_added,
                        element_symbol(element)
                    );
                    if clash.is_some() {
                        message.push_str(&format!(", skipped {} (steric clash)", result.skipped));
                    }
                    context.selected_node_eval_cache =
                        Some(Box::new(PassivateEvalCache { message }));
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
        let mut props = vec![("element".to_string(), TextValue::Int(self.element as i32))];
        // Written only when on, so saved networks without the option are
        // unchanged.
        if self.skip_clashes {
            props.push(("skip_clashes".to_string(), TextValue::Bool(true)));
            props.push((
                "clash_factor".to_string(),
                TextValue::Float(self.clash_factor),
            ));
        }
        props
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("element") {
            self.element = v
                .as_int()
                .ok_or_else(|| "element must be an integer".to_string())?
                as i16;
        }
        if let Some(v) = props.get("skip_clashes") {
            self.skip_clashes = v
                .as_bool()
                .ok_or_else(|| "skip_clashes must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("clash_factor") {
            self.clash_factor = v
                .as_float()
                .ok_or_else(|| "clash_factor must be a number".to_string())?;
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
