// Materialize: carves atoms out of a Blueprint's structure using the blueprint's geometry.
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::motif_parser::parse_parameter_element_values;
use crate::structure_designer::common_constants::{
    REAL_IMPLICIT_VOLUME_MAX, REAL_IMPLICIT_VOLUME_MIN,
};
use crate::structure_designer::data_type::{DataType, RecordType};
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{CrystalData, NetworkResult};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network::ValidationError;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::daabox::DAABox;
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;

// Import the lattice fill algorithm
use crate::crystolecule::lattice_fill::{
    DEFAULT_REGION_MARGIN, LatticeFillConfig, LatticeFillOptions, RegionSpec, fill_lattice,
};

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterializeData {
    pub parameter_element_value_definition: String,
    pub hydrogen_passivation: bool,
    /// Whether to remove unbonded (zero-bond) atoms before passivation.
    /// Defaults to `true` to preserve the historical hardcoded behavior.
    #[serde(default = "default_true")]
    pub remove_unbonded_atoms: bool,
    #[serde(default)]
    pub remove_single_bond_atoms_before_passivation: bool,
    #[serde(default)]
    pub surface_reconstruction: bool,
    #[serde(default)]
    pub invert_phase: bool,
    #[serde(skip)]
    pub error: Option<String>,
    #[serde(skip)]
    pub parameter_element_values: HashMap<String, i16>,
    /// Cached motif parameter list from last evaluation (name, default_atomic_number)
    #[serde(skip)]
    pub available_parameters: RefCell<Vec<(String, i16)>>,
}

#[derive(Debug, Clone, Deserialize)]
struct MaterializeDataDeserialized {
    pub parameter_element_value_definition: String,
    pub hydrogen_passivation: bool,
    #[serde(default = "default_true")]
    pub remove_unbonded_atoms: bool,
    #[serde(default)]
    pub remove_single_bond_atoms_before_passivation: bool,
    #[serde(default)]
    pub surface_reconstruction: bool,
    #[serde(default)]
    pub invert_phase: bool,
}

impl<'de> Deserialize<'de> for MaterializeData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let de = MaterializeDataDeserialized::deserialize(deserializer)?;

        let mut data = MaterializeData {
            parameter_element_value_definition: de.parameter_element_value_definition,
            hydrogen_passivation: de.hydrogen_passivation,
            remove_unbonded_atoms: de.remove_unbonded_atoms,
            remove_single_bond_atoms_before_passivation: de
                .remove_single_bond_atoms_before_passivation,
            surface_reconstruction: de.surface_reconstruction,
            invert_phase: de.invert_phase,
            error: None,
            parameter_element_values: HashMap::new(),
            available_parameters: RefCell::new(Vec::new()),
        };

        if !data.parameter_element_value_definition.trim().is_empty() {
            match parse_parameter_element_values(&data.parameter_element_value_definition) {
                Ok(values) => {
                    data.parameter_element_values = values;
                }
                Err(parse_error) => {
                    data.error = Some(format!("Parameter element parse error: {}", parse_error));
                }
            }
        }

        Ok(data)
    }
}

impl MaterializeData {
    /// Parses and validates the parameter element definition and returns any validation errors
    pub fn parse_and_validate(&mut self, node_id: u64) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // Clear previous state
        self.parameter_element_values.clear();
        self.error = None;

        // Skip validation if definition is empty
        if self.parameter_element_value_definition.trim().is_empty() {
            return errors;
        }

        // Parse the parameter element value definition
        match parse_parameter_element_values(&self.parameter_element_value_definition) {
            Ok(values) => {
                self.parameter_element_values = values;
            }
            Err(parse_error) => {
                let error_msg = format!("Parameter element parse error: {}", parse_error);
                self.error = Some(error_msg.clone());
                errors.push(ValidationError::new(error_msg, Some(node_id)));
            }
        }

        errors
    }
}

impl NodeData for MaterializeData {
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
        // Evaluate geometry input
        let shape_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = shape_val {
            return EvalOutput::single(shape_val);
        }

        let mesh = match shape_val {
            NetworkResult::Blueprint(mesh) => mesh,
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "materialize: shape input must be a Blueprint".to_string(),
                ));
            }
        };
        let structure = mesh.structure.clone();
        let geo_tree_root_for_crystal = mesh.geo_tree_root.clone();
        let alignment = mesh.alignment;
        let alignment_reason = mesh.alignment_reason.clone();

        // Motif and motif offset now come from the Blueprint's structure.
        let motif = mesh.structure.motif.clone();
        let motif_offset = mesh.structure.motif_offset;

        // Cache motif parameters for UI display
        *self.available_parameters.borrow_mut() = motif
            .parameters
            .iter()
            .map(|p| (p.name.clone(), p.default_atomic_number))
            .collect();

        // Evaluate passivate input (with default)
        let hydrogen_passivation = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.hydrogen_passivation,
            NetworkResult::extract_bool,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // Evaluate rm_single input (with default)
        let remove_single_bond_atoms = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            self.remove_single_bond_atoms_before_passivation,
            NetworkResult::extract_bool,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // Evaluate surf_recon input (with default)
        let surface_reconstruction = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            3,
            self.surface_reconstruction,
            NetworkResult::extract_bool,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let invert_phase = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            4,
            self.invert_phase,
            NetworkResult::extract_bool,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // Evaluate rm_unbonded input (with default)
        let remove_unbonded_atoms = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            5,
            self.remove_unbonded_atoms,
            NetworkResult::extract_bool,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // Evaluate optional `regions` input (param index 6). Disconnected →
        // empty (today's global-settings-everywhere behavior). Connected →
        // an ordered `Array[Record(MaterializeRegion)]` layered on top of the
        // root settings via the per-field painter's algorithm in
        // `SettingsResolver` (see doc/design_blueprint_region_atom_edits.md §B6).
        let regions_input =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 6);
        let regions = match regions_input {
            NetworkResult::None => Vec::new(),
            NetworkResult::Error(_) => return EvalOutput::single(regions_input),
            NetworkResult::Array(items) => match parse_regions_from_records(items) {
                Ok(r) => r,
                Err(e) => return EvalOutput::single(NetworkResult::Error(e)),
            },
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "materialize.regions: expected Array[Record], got {:?}",
                    other.infer_data_type()
                )));
            }
        };

        // Calculate effective parameter element values (fill in defaults for missing values)
        let effective_parameter_values =
            motif.get_effective_parameter_element_values(&self.parameter_element_values);

        // Build configuration
        let config = LatticeFillConfig {
            unit_cell: mesh.structure.lattice_vecs,
            motif,
            parameter_element_values: effective_parameter_values,
            geometry: mesh.geo_tree_root,
            motif_offset,
            // Per-region materialization settings (Part B of
            // doc/design_blueprint_region_atom_edits.md).
            regions,
        };

        let options = LatticeFillOptions {
            hydrogen_passivation,
            remove_unbonded_atoms,
            remove_single_bond_atoms,
            reconstruct_surface: surface_reconstruction,
            invert_phase,
            // Phase 1: default to hydrogen. Phase 2 wires the node's
            // passivation_element property / passiv_elem pin here.
            passivation_element: 1,
        };

        // Define fill region
        let fill_region = DAABox::from_start_and_size(
            REAL_IMPLICIT_VOLUME_MIN,
            REAL_IMPLICIT_VOLUME_MAX - REAL_IMPLICIT_VOLUME_MIN,
        );

        // Call the lattice fill algorithm
        let result = fill_lattice(&config, &options, &fill_region);

        EvalOutput::single(NetworkResult::Crystal(CrystalData {
            structure,
            atoms: result.atomic_structure,
            geo_tree_root: Some(geo_tree_root_for_crystal),
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

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            // Note: parameter_element_value_definition has no matching parameter (stored-only field)
            (
                "parameter_element_value_definition".to_string(),
                TextValue::String(self.parameter_element_value_definition.clone()),
            ),
            (
                "passivate".to_string(),
                TextValue::Bool(self.hydrogen_passivation),
            ),
            (
                "rm_unbonded".to_string(),
                TextValue::Bool(self.remove_unbonded_atoms),
            ),
            (
                "rm_single".to_string(),
                TextValue::Bool(self.remove_single_bond_atoms_before_passivation),
            ),
            (
                "surf_recon".to_string(),
                TextValue::Bool(self.surface_reconstruction),
            ),
            (
                "invert_phase".to_string(),
                TextValue::Bool(self.invert_phase),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("parameter_element_value_definition") {
            self.parameter_element_value_definition = v
                .as_string()
                .ok_or_else(|| "parameter_element_value_definition must be a string".to_string())?
                .to_string();
            // Parse the definition into the HashMap used by eval()
            self.parameter_element_values.clear();
            if !self.parameter_element_value_definition.trim().is_empty() {
                match parse_parameter_element_values(&self.parameter_element_value_definition) {
                    Ok(values) => {
                        self.parameter_element_values = values;
                    }
                    Err(parse_error) => {
                        self.error =
                            Some(format!("Parameter element parse error: {}", parse_error));
                    }
                }
            }
        }
        if let Some(v) = props.get("passivate") {
            self.hydrogen_passivation = v
                .as_bool()
                .ok_or_else(|| "passivate must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("rm_unbonded") {
            self.remove_unbonded_atoms = v
                .as_bool()
                .ok_or_else(|| "rm_unbonded must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("rm_single") {
            self.remove_single_bond_atoms_before_passivation = v
                .as_bool()
                .ok_or_else(|| "rm_single must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("surf_recon") {
            self.surface_reconstruction = v
                .as_bool()
                .ok_or_else(|| "surf_recon must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("invert_phase") {
            self.invert_phase = v
                .as_bool()
                .ok_or_else(|| "invert_phase must be a boolean".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("shape".to_string(), (true, None)); // required
        m.insert("regions".to_string(), (false, None)); // optional
        m
    }
}

/// Parse a runtime `Array[Record(MaterializeRegion)]` value into the
/// `RegionSpec` list consumed by `fill_lattice` (see
/// `doc/design_blueprint_region_atom_edits.md` §B1/§B6). Per item: `volume`
/// must extract to a `Blueprint` (its `geo_tree_root` is taken; structure
/// ignored), `margin` is a `Float` or unset (→ `DEFAULT_REGION_MARGIN`), and
/// each of the five settings is a `Bool` or unset (→ inherit, `None`). An unset
/// optional field arrives as an explicit `NetworkResult::None` in the record
/// slot (record_construct's optional-field collapse exemption). Any malformed
/// item produces an `Err(String)` naming the item index.
fn parse_regions_from_records(items: Vec<NetworkResult>) -> Result<Vec<RegionSpec>, String> {
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.into_iter().enumerate() {
        // `volume` is the one required field. Take its geometry SDF; ignore
        // the Blueprint's structure (documented; §B1).
        let geometry = match item.extract_record_field("volume") {
            Some(NetworkResult::Blueprint(bp)) => bp.geo_tree_root.clone(),
            None | Some(NetworkResult::None) => {
                return Err(format!(
                    "materialize.regions[{}]: missing required 'volume' field",
                    i
                ));
            }
            Some(other) => {
                return Err(format!(
                    "materialize.regions[{}].volume: expected Blueprint, got {:?}",
                    i,
                    other.infer_data_type()
                ));
            }
        };

        // `margin` is `Optional[Float]`; unset → default.
        let margin = match item.extract_record_field("margin") {
            None | Some(NetworkResult::None) => DEFAULT_REGION_MARGIN,
            Some(NetworkResult::Float(f)) => *f,
            Some(other) => {
                return Err(format!(
                    "materialize.regions[{}].margin: expected Float, got {:?}",
                    i,
                    other.infer_data_type()
                ));
            }
        };

        out.push(RegionSpec {
            geometry,
            margin,
            passivate: parse_optional_bool_field(&item, "passivate", i)?,
            rm_single: parse_optional_bool_field(&item, "rm_single", i)?,
            surf_recon: parse_optional_bool_field(&item, "surf_recon", i)?,
            invert_phase: parse_optional_bool_field(&item, "invert_phase", i)?,
            rm_unbonded: parse_optional_bool_field(&item, "rm_unbonded", i)?,
            // Phase 2 wires the MaterializeRegion.passiv_elem record field here.
            passiv_elem: None,
        });
    }
    Ok(out)
}

/// Read one `Optional[Bool]` settings field from a region record. Missing or an
/// explicit `None` → `None` ("inherit"); a `Bool` → `Some(b)`; anything else is
/// a malformed item.
fn parse_optional_bool_field(
    item: &NetworkResult,
    name: &str,
    index: usize,
) -> Result<Option<bool>, String> {
    match item.extract_record_field(name) {
        None | Some(NetworkResult::None) => Ok(None),
        Some(NetworkResult::Bool(b)) => Ok(Some(*b)),
        Some(other) => Err(format!(
            "materialize.regions[{}].{}: expected Bool, got {:?}",
            index,
            name,
            other.infer_data_type()
        )),
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "materialize".to_string(),
      description: "Converts a Blueprint into a Crystal by carving atoms out of the Blueprint's structure using its geometry.".to_string(),
      summary: None,
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
          Parameter {
              id: None,
              name: "shape".to_string(),
              data_type: DataType::Blueprint,
          },
          Parameter {
              id: None,
              name: "passivate".to_string(),
              data_type: DataType::Bool,
          },
          Parameter {
              id: None,
              name: "rm_single".to_string(),
              data_type: DataType::Bool,
          },
          Parameter {
              id: None,
              name: "surf_recon".to_string(),
              data_type: DataType::Bool,
          },
          Parameter {
              id: None,
              name: "invert_phase".to_string(),
              data_type: DataType::Bool,
          },
          Parameter {
              id: None,
              name: "rm_unbonded".to_string(),
              data_type: DataType::Bool,
          },
          Parameter {
              id: None,
              name: "regions".to_string(),
              data_type: DataType::Array(Box::new(DataType::Record(RecordType::Named(
                  "MaterializeRegion".to_string(),
              )))),
          },
      ],
      output_pins: OutputPinDefinition::single_fixed(DataType::Crystal),
      zone_input_pins: vec![],
      zone_output_pins: vec![],
      public: true,
      node_data_creator: || Box::new(MaterializeData {
        parameter_element_value_definition: String::new(),
        hydrogen_passivation: true,
        remove_unbonded_atoms: true,
        remove_single_bond_atoms_before_passivation: false,
        surface_reconstruction: false,
        invert_phase: false,
        error: None,
        parameter_element_values: HashMap::new(),
        available_parameters: RefCell::new(Vec::new()),
      }),
      node_data_saver: generic_node_data_saver::<MaterializeData>,
      node_data_loader: generic_node_data_loader::<MaterializeData>,
    }
}
