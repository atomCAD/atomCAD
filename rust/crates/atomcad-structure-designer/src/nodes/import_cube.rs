//! `import_cube` — imports volumetric scalar data from a Gaussian `.cube` file.
//!
//! Two outputs, because a `.cube` file carries two things: the sampled field
//! itself (pin 0) and the atom block that field was computed around (pin 1).
//! The `molecule` pin is what makes ingestion verifiable before any field
//! rendering exists — it renders through the existing impostor path, so a
//! missed Bohr→Ångström conversion shows up as a molecule 1.89x too large
//! rather than as a number nobody can check. See `doc/design_scalar_fields.md`.
//!
//! Two conventions this node inherits and must not break:
//!
//! - **Every error path returns *two* results.** `EvalOutput::multi` is
//!   positional, so a one-element error output would leave pin 1 reading
//!   `NetworkResult::None` instead of the error — the same trap the diff-output
//!   nodes document in `evaluator/atom_op.rs`.
//! - **The units check warns, it never re-interprets.** `load_cube` always
//!   reads coordinates as Bohr; an implausible atom block surfaces here as a
//!   **non-blocking** `NodeDataError::warning` (the node still produces a
//!   usable field, so it fails the blocking litmus in
//!   `doc/design_error_management.md`).
//!
//! The parsed payload is `#[serde(skip)]` and reloaded by
//! [`import_cube_data_loader`] after deserialization, exactly as
//! `ImportXYZData` does: a cube file is megabytes of samples and has no
//! business inside a `.cnnd`.

use crate::data_type::DataType;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::{MoleculeData, NetworkResult};
use crate::node_data::{EvalOutput, NodeData, NodeDataError};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{NodeType, OutputPinDefinition, Parameter};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::field::{SampledField, ScalarField};
use atomcad_crystolecule::io::cube_loader::{CubeFile, load_cube};
use atomcad_util::path_utils::{get_parent_directory, resolve_path, try_make_relative};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;

/// The parsed content of one `.cube` file, in the shape the node evaluates
/// from.
///
/// Not `CubeFile` itself, for two reasons: `NodeData` requires `Clone` (via
/// `clone_box`), and the field is held behind an `Arc` so cloning the node data
/// — which happens on every copy/paste and undo snapshot — never copies
/// megabytes of samples. The `Arc` is also exactly what
/// `NetworkResult::ScalarField` wants, so `eval` hands it straight through.
#[derive(Debug, Clone)]
pub struct LoadedCube {
    pub atoms: AtomicStructure,
    pub field: Arc<SampledField>,
    /// Advisory only — see the module doc.
    pub units_warning: Option<String>,
}

impl LoadedCube {
    /// Take the single field out of a freshly parsed `CubeFile`.
    ///
    /// Returns `None` for a file that yielded no field at all. `load_cube`
    /// never does that today — it rejects every multi-field file, so `fields`
    /// always holds exactly one element — but indexing `fields[0]` would turn
    /// a future loader change into a panic, and this node has an error channel
    /// already.
    pub fn from_cube_file(cube: CubeFile) -> Option<Self> {
        let CubeFile {
            atoms,
            fields,
            units_warning,
        } = cube;
        let field = fields.into_iter().next()?;
        Some(LoadedCube {
            atoms,
            field: Arc::new(field),
            units_warning,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportCubeData {
    pub file_name: Option<String>, // If none, nothing has been imported yet.

    #[serde(skip)]
    pub loaded: Option<LoadedCube>,
}

impl ImportCubeData {
    pub fn new() -> Self {
        Self {
            file_name: None,
            loaded: None,
        }
    }

    /// Both output values for a successfully loaded file.
    fn outputs(loaded: &LoadedCube) -> EvalOutput {
        let field: Arc<dyn ScalarField> = loaded.field.clone();
        EvalOutput::multi(vec![
            NetworkResult::ScalarField(field),
            NetworkResult::Molecule(MoleculeData {
                atoms: loaded.atoms.clone(),
                geo_tree_root: None,
            }),
        ])
    }

    /// The same error on both pins. See the module doc on why a one-element
    /// error output is wrong here.
    fn error(message: String) -> EvalOutput {
        EvalOutput::multi(vec![
            NetworkResult::Error(message.clone()),
            NetworkResult::Error(message),
        ])
    }
}

impl Default for ImportCubeData {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeData for ImportCubeData {
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
        context: &mut crate::evaluator::network_evaluator::NetworkEvaluationContext,
    ) -> EvalOutput {
        let result = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);

        // No wired file name: evaluate from the preloaded payload.
        if let NetworkResult::None = result {
            return match &self.loaded {
                Some(loaded) => Self::outputs(loaded),
                None => Self::error("No cube file imported".to_string()),
            };
        }

        if result.is_error() {
            return EvalOutput::multi(vec![result.clone(), result]);
        }

        let NetworkResult::String(file_name) = result else {
            return Self::error("Expected string parameter for file name".to_string());
        };

        // A wired file name overrides the stored property, matching `import_xyz`.
        let design_dir = registry
            .design_file_name
            .as_ref()
            .and_then(|design_path| get_parent_directory(design_path));

        let resolved_path = match resolve_path(&file_name, design_dir.as_deref()) {
            Ok((resolved_path, _was_relative)) => resolved_path,
            Err(_) => {
                return Self::error(format!("Failed to resolve path: {}", file_name));
            }
        };

        match load_cube(&resolved_path, true) {
            Ok(cube) => match LoadedCube::from_cube_file(cube) {
                Some(loaded) => Self::outputs(&loaded),
                None => Self::error(format!("Cube file contains no field: {}", file_name)),
            },
            Err(error) => Self::error(format!(
                "Failed to load cube file: {}: {}",
                file_name, error
            )),
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        if connected_input_pins.contains("file_name") {
            None
        } else {
            self.file_name.clone()
        }
    }

    /// The units plausibility warning from the loader, surfaced as a
    /// **non-blocking** error so it reaches the unified error list without
    /// stopping evaluation — the node still produces a usable field.
    fn get_data_error(&self) -> Option<NodeDataError> {
        self.loaded
            .as_ref()
            .and_then(|loaded| loaded.units_warning.clone())
            .map(NodeDataError::warning)
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        let mut props = Vec::new();
        if let Some(ref file_name) = self.file_name {
            props.push((
                "file_name".to_string(),
                TextValue::String(file_name.clone()),
            ));
        }
        props
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("file_name") {
            self.file_name = Some(
                v.as_string()
                    .ok_or_else(|| "file_name must be a string".to_string())?
                    .to_string(),
            );
        }
        Ok(())
    }
}

/// Loads the cube payload after deserializing, mirroring
/// `import_xyz_data_loader`.
///
/// A failure leaves `loaded` as `None`, so the node exists and reports the
/// problem when evaluated rather than failing the whole project load.
pub fn import_cube_data_loader(
    value: &Value,
    design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let mut data: ImportCubeData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    if let Some(ref file_name) = data.file_name {
        data.loaded = match resolve_path(file_name, design_dir) {
            Ok((resolved_path, _was_relative)) => match load_cube(&resolved_path, true) {
                Ok(cube) => LoadedCube::from_cube_file(cube),
                Err(_cube_error) => None,
            },
            Err(_path_error) => None,
        };
    }

    Ok(Box::new(data))
}

/// Relativizes the stored path before saving, so projects stay portable.
pub fn import_cube_data_saver(
    node_data: &mut dyn NodeData,
    design_dir: Option<&str>,
) -> io::Result<Value> {
    if let Some(data) = node_data.as_any_mut().downcast_mut::<ImportCubeData>() {
        if let (Some(file_name), Some(design_dir)) = (&data.file_name, design_dir) {
            let (potentially_relative_path, should_update) =
                try_make_relative(file_name, Some(design_dir));
            if should_update {
                data.file_name = Some(potentially_relative_path);
            }
        }

        serde_json::to_value(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data type mismatch for import_cube",
        ))
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "import_cube".to_string(),
      description: "Imports volumetric scalar data (a molecular orbital, an electron density, an electrostatic potential) from a Gaussian .cube file.
The field output carries the sampled data; the molecule output carries the atoms the file was computed around, which is the quickest way to check that a file loaded correctly.
Coordinates in a .cube file are always read as Bohr and converted to Angstrom. If the atom block's interatomic distances look chemically implausible under that assumption the node shows a warning, but it never re-interprets the file.
It converts file paths to relative paths whenever possible (if the file is in the same directory as the node or in a subdirectory) so that when you copy your whole project to another location or machine the cube file references will remain valid.".to_string(),
      summary: None,
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
        Parameter {
          id: None,
          name: "file_name".to_string(),
          data_type: DataType::String,
        },
      ],
      output_pins: vec![
        OutputPinDefinition::fixed("field", DataType::ScalarField),
        OutputPinDefinition::fixed("molecule", DataType::Molecule),
      ],
      zone_input_pins: vec![],
      zone_output_pins: vec![],
      public: true,
      node_data_creator: || Box::new(ImportCubeData::new()),
      node_data_saver: import_cube_data_saver,
      node_data_loader: import_cube_data_loader,
    }
}
