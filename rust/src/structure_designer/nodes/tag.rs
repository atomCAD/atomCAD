//! `tag` / `untag` — region-gated named per-atom group nodes.
//!
//! Phase 3 of `doc/design_atom_tags.md`. Both are `HasAtoms`-polymorphic,
//! metadata-only pass-throughs in the `freeze`/`unfreeze`/`xray` family, built
//! on `evaluator::atom_op::map_atomic_in_region` with the standard
//! `DEFAULT_REGION_MARGIN` membership test. With the `region` pin disconnected
//! they act on **all** atoms; connected, only on in-region atoms. Multiple
//! regions = chained nodes.
//!
//! `tag` adds `name` to every in-region atom; `untag` removes it, and an empty
//! `name` on `untag` clears **all** tags from in-region atoms (the
//! blanket-restore analog of `unfreeze` / `xray` α = 1.0). An empty name on
//! `tag`, or exceeding the 32-name tag limit, surfaces as a localized
//! `NetworkResult::Error` on the node.
//!
//! Tags are inert, durable selector metadata — they carry no behavior of their
//! own and have no viewport effect until a downstream consumer interprets them
//! (see `doc/design_atom_tags.md` §Future work). The atom hover popup (Phase 5)
//! is the inspection surface.

use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
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
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// Reads the input's `tag_names()` table (empty for non-atomic inputs) so the
/// Flutter property editor can offer existing names as suggestions (Phase 5).
fn snapshot_tag_names(input: &NetworkResult) -> Vec<String> {
    match input {
        NetworkResult::Crystal(c) => c.atoms.tag_names().to_vec(),
        NetworkResult::Molecule(m) => m.atoms.tag_names().to_vec(),
        _ => Vec::new(),
    }
}

/// Shared parameter/metadata plumbing for both nodes.
fn tag_parameter_metadata() -> HashMap<String, (bool, Option<String>)> {
    let mut m = HashMap::new();
    m.insert("molecule".to_string(), (true, None)); // required
    m.insert("name".to_string(), (false, None)); // optional
    m.insert("region".to_string(), (false, None)); // optional
    m
}

fn tag_parameters() -> Vec<Parameter> {
    vec![
        Parameter {
            id: None,
            name: "molecule".to_string(),
            data_type: DataType::HasAtoms,
        },
        Parameter {
            id: None,
            name: "name".to_string(),
            data_type: DataType::String,
        },
        Parameter {
            id: None,
            name: "region".to_string(),
            data_type: DataType::Blueprint,
        },
    ]
}

/// Shared `eval` body for both nodes. Reads the required atomic input (pin 0),
/// the optional `name` (pin 1, wired overrides the stored property), and the
/// optional `region` Blueprint (pin 2), then applies the tag operation to every
/// in-region atom.
#[allow(clippy::too_many_arguments)]
fn eval_tag_op<'a>(
    node_label: &str,
    is_untag: bool,
    stored_name: &str,
    available_tags: &RefCell<Vec<String>>,
    network_evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
) -> EvalOutput {
    let input_val =
        network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

    if let NetworkResult::Error(_) = input_val {
        return EvalOutput::single(input_val);
    }

    // Snapshot the input's tag table for the editor's suggestion list (§Existing
    // -names suggestions). Must read before `map_atomic_in_region` consumes the
    // value below.
    *available_tags.borrow_mut() = snapshot_tag_names(&input_val);

    // Name: wired pin 1 overrides the stored property.
    let name = match network_evaluator.evaluate_or_default(
        network_stack,
        node_id,
        registry,
        context,
        1,
        stored_name.to_string(),
        NetworkResult::extract_string,
    ) {
        Ok(value) => value,
        Err(error) => return EvalOutput::single(error),
    };
    let name = name.trim().to_string();

    // `tag` requires a non-empty name; `untag` treats empty as "clear all tags".
    if !is_untag && name.is_empty() {
        return EvalOutput::single(NetworkResult::Error(format!(
            "{}: tag name is empty",
            node_label
        )));
    }

    // Optional `region` pin (param index 2). Disconnected → operate on every
    // atom. Connected → only in-region atoms.
    let region_input = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 2);
    let region_geo = match region_input {
        NetworkResult::None => None,
        NetworkResult::Error(_) => return EvalOutput::single(region_input),
        NetworkResult::Blueprint(bp) => Some(bp.geo_tree_root),
        other => {
            return EvalOutput::single(NetworkResult::Error(format!(
                "{}.region: expected Blueprint, got {:?}",
                node_label,
                other.infer_data_type()
            )));
        }
    };

    // The tag-limit error can only be raised inside the mutation closure (which
    // returns the structure, not a `Result`), so capture it and surface after.
    let mut tag_error: Option<String> = None;
    let output = map_atomic_in_region(
        input_val,
        region_geo.as_ref(),
        DEFAULT_REGION_MARGIN,
        |mut structure, in_region| {
            let ids: Vec<u32> = structure
                .iter_atoms()
                .filter(|(atom_id, _)| in_region(**atom_id))
                .map(|(atom_id, _)| *atom_id)
                .collect();
            for id in ids {
                if is_untag {
                    if name.is_empty() {
                        structure.clear_atom_tags(id);
                    } else {
                        structure.remove_atom_tag(id, &name);
                    }
                } else if let Err(e) = structure.add_atom_tag(id, &name) {
                    // Interning is idempotent, so the first failure means the
                    // table is full for this new name — no later atom succeeds.
                    tag_error = Some(e.to_string());
                    break;
                }
            }
            structure
        },
    );

    if let Some(message) = tag_error {
        return EvalOutput::single(NetworkResult::Error(format!("{}: {}", node_label, message)));
    }
    EvalOutput::single(output)
}

// ============================================================================
// tag
// ============================================================================

fn default_tag_name() -> String {
    "tag".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagData {
    /// Tag name added to in-region atoms. Wired `name` pin overrides this
    /// stored value.
    #[serde(default = "default_tag_name")]
    pub name: String,
    /// Input structure's tag names, captured in `eval` for the editor's
    /// suggestion dropdown (§Existing-names suggestions). Never serialized.
    #[serde(skip)]
    pub available_tags: RefCell<Vec<String>>,
}

impl NodeData for TagData {
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
        eval_tag_op(
            "tag",
            false,
            &self.name,
            &self.available_tags,
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
        )
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        if connected_input_pins.contains("name") {
            return None;
        }
        Some(self.name.clone())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        tag_parameter_metadata()
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![("name".to_string(), TextValue::String(self.name.clone()))]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("name") {
            self.name = v
                .as_string()
                .ok_or_else(|| "name must be a string".to_string())?
                .to_string();
        }
        Ok(())
    }
}

pub fn tag_get_node_type() -> NodeType {
    NodeType {
        name: "tag".to_string(),
        description: "Adds a named tag to atoms — inert, durable metadata that selects a group of \
                      atoms for downstream consumers. With a region connected, only atoms inside \
                      the region volume are tagged; otherwise all atoms are. Tags have no visual \
                      effect on their own; hover an atom to see its tags. A structure supports at \
                      most 32 distinct tag names."
            .to_string(),
        summary: Some("Tag atoms".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: tag_parameters(),
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || {
            Box::new(TagData {
                name: default_tag_name(),
                available_tags: RefCell::new(Vec::new()),
            })
        },
        node_data_saver: generic_node_data_saver::<TagData>,
        node_data_loader: generic_node_data_loader::<TagData>,
    }
}

// ============================================================================
// untag
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UntagData {
    /// Tag name removed from in-region atoms. Wired `name` pin overrides this
    /// stored value. Empty removes **all** tags from in-region atoms.
    #[serde(default)]
    pub name: String,
    /// Input structure's tag names, captured in `eval` for the editor's
    /// suggestion dropdown (§Existing-names suggestions). Never serialized.
    #[serde(skip)]
    pub available_tags: RefCell<Vec<String>>,
}

impl NodeData for UntagData {
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
        eval_tag_op(
            "untag",
            true,
            &self.name,
            &self.available_tags,
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
        )
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        if connected_input_pins.contains("name") {
            return None;
        }
        if self.name.trim().is_empty() {
            Some("all tags".to_string())
        } else {
            Some(self.name.clone())
        }
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        tag_parameter_metadata()
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![("name".to_string(), TextValue::String(self.name.clone()))]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("name") {
            self.name = v
                .as_string()
                .ok_or_else(|| "name must be a string".to_string())?
                .to_string();
        }
        Ok(())
    }
}

pub fn untag_get_node_type() -> NodeType {
    NodeType {
        name: "untag".to_string(),
        description: "Removes a named tag from atoms. With a region connected, only atoms inside \
                      the region volume are affected; otherwise all atoms are. An empty name \
                      removes every tag from the affected atoms. Removing a tag an atom does not \
                      carry is a no-op."
            .to_string(),
        summary: Some("Untag atoms".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: tag_parameters(),
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || {
            Box::new(UntagData {
                name: String::new(),
                available_tags: RefCell::new(Vec::new()),
            })
        },
        node_data_saver: generic_node_data_saver::<UntagData>,
        node_data_loader: generic_node_data_loader::<UntagData>,
    }
}
