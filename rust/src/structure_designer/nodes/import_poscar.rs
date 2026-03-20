use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::io::poscar_loader::{load_poscar, parse_poscar};
use crate::crystolecule::motif::Motif;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{NodeType, Parameter};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::path_utils::{get_parent_directory, resolve_path, try_make_relative};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportPoscarData {
    pub file_name: Option<String>,

    #[serde(skip)]
    pub cached_unit_cell: Option<UnitCellStruct>,

    #[serde(skip)]
    pub cached_motif: Option<Motif>,
}

impl NodeData for ImportPoscarData {
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
        context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext,
    ) -> NetworkResult {
        let result = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);

        let parsed = if let NetworkResult::None = result {
            // No parameter provided, use the cached data from file load
            match (&self.cached_unit_cell, &self.cached_motif) {
                (Some(uc), Some(m)) => Some((uc.clone(), m.clone())),
                _ => None,
            }
        } else {
            if result.is_error() {
                return result;
            }

            if let NetworkResult::String(file_name) = result {
                let design_dir = registry
                    .design_file_name
                    .as_ref()
                    .and_then(|design_path| get_parent_directory(design_path));

                match resolve_path(&file_name, design_dir.as_deref()) {
                    Ok((resolved_path, _was_relative)) => match load_poscar(&resolved_path) {
                        Ok((unit_cell, motif)) => Some((unit_cell, motif)),
                        Err(e) => {
                            return NetworkResult::Error(format!(
                                "Failed to load POSCAR file '{}': {}",
                                file_name, e
                            ));
                        }
                    },
                    Err(_) => {
                        return NetworkResult::Error(format!(
                            "Failed to resolve path: {}",
                            file_name
                        ));
                    }
                }
            } else {
                return NetworkResult::Error(
                    "Expected string parameter for file name".to_string(),
                );
            }
        };

        match parsed {
            Some((unit_cell, motif)) => NetworkResult::Array(vec![
                NetworkResult::UnitCell(unit_cell),
                NetworkResult::Motif(motif),
            ]),
            None => NetworkResult::Error("No POSCAR file imported".to_string()),
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

impl Default for ImportPoscarData {
    fn default() -> Self {
        Self::new()
    }
}

impl ImportPoscarData {
    pub fn new() -> Self {
        Self {
            file_name: None,
            cached_unit_cell: None,
            cached_motif: None,
        }
    }

    /// Creates an ImportPoscarData from POSCAR content string (for testing).
    pub fn from_content(content: &str) -> Result<Self, String> {
        let (unit_cell, motif) =
            parse_poscar(content).map_err(|e| format!("Failed to parse POSCAR: {}", e))?;
        Ok(Self {
            file_name: None,
            cached_unit_cell: Some(unit_cell),
            cached_motif: Some(motif),
        })
    }
}

/// Special loader for ImportPoscarData that loads the POSCAR file after deserializing
pub fn import_poscar_data_loader(
    value: &Value,
    design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let mut data: ImportPoscarData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    if let Some(ref file_name) = data.file_name {
        match resolve_path(file_name, design_dir) {
            Ok((resolved_path, _was_relative)) => match load_poscar(&resolved_path) {
                Ok((unit_cell, motif)) => {
                    data.cached_unit_cell = Some(unit_cell);
                    data.cached_motif = Some(motif);
                }
                Err(_) => {
                    data.cached_unit_cell = None;
                    data.cached_motif = None;
                }
            },
            Err(_) => {
                data.cached_unit_cell = None;
                data.cached_motif = None;
            }
        }
    }

    Ok(Box::new(data))
}

/// Special saver for ImportPoscarData that converts file path to relative before saving
pub fn import_poscar_data_saver(
    node_data: &mut dyn NodeData,
    design_dir: Option<&str>,
) -> io::Result<Value> {
    if let Some(data) = node_data.as_any_mut().downcast_mut::<ImportPoscarData>() {
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
            "Data type mismatch for import_poscar",
        ))
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "import_poscar".to_string(),
        description: "Imports crystal structure data from a VASP POSCAR file.\n\
            Outputs the unit cell (basis vectors) and motif (atomic sites with fractional coordinates).\n\
            Supports VASP 5+ format with element names.\n\
            File paths are converted to relative paths when possible for portability."
            .to_string(),
        summary: Some("Import crystal data from POSCAR file".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![Parameter {
            id: None,
            name: "file_name".to_string(),
            data_type: DataType::String,
        }],
        output_type: DataType::UnitCell,
        additional_output_types: vec![DataType::Motif],
        public: true,
        node_data_creator: || Box::new(ImportPoscarData::new()),
        node_data_saver: import_poscar_data_saver,
        node_data_loader: import_poscar_data_loader,
    }
}
