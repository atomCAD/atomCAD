use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::io::xyz_saver::save_xyz;
use crate::structure_designer::data_type::{DataType, RecordType};
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{NetworkResult, dmat3_to_rows};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{NodeType, OutputPinDefinition, Parameter};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::path_utils::{get_parent_directory, resolve_path, try_make_relative};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportXYZData {
    pub file_name: String, // If empty, the file name is not given yet.
}

impl NodeData for ExportXYZData {
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
    ) -> EvalOutput {
        // No `if context.execute { … }` guard — the central skip rule in
        // `evaluate_all_outputs` / `evaluate` guarantees this `eval` is only
        // invoked when `context.execute == true`. See
        // `doc/design_node_execution.md` (Phase 2 — Central skip rule for
        // Unit-returning nodes).
        let atomic_structure = match network_evaluator.evaluate_required(
            network_stack,
            node_id,
            registry,
            context,
            0,
            NetworkResult::extract_atomic,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let file_name = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.file_name.clone(),
            NetworkResult::extract_string,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // Check if file name is empty
        if file_name.is_empty() {
            return EvalOutput::single(NetworkResult::Error(
                "Missing export XYZ file name".to_string(),
            ));
        }

        // Get design directory from registry
        let design_dir = registry
            .design_file_name
            .as_ref()
            .and_then(|design_path| get_parent_directory(design_path));

        // Resolve the file path (handle relative paths)
        let resolved_path = match resolve_path(&file_name, design_dir.as_deref()) {
            Ok((path, _was_relative)) => path,
            Err(_) => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "Failed to resolve export path: {}",
                    file_name
                )));
            }
        };

        if let Err(err) = save_xyz(&atomic_structure, &resolved_path) {
            return EvalOutput::single(NetworkResult::Error(format!(
                "Failed to save XYZ file '{}': {}",
                file_name, err
            )));
        }

        // Optional `metadata` record (pin 2) → write a machine-readable sidecar
        // next to the XYZ file. The pin type is an empty anonymous record, so
        // any record value flows in (width subtyping) carrying all its fields.
        // When the pin is unconnected, `evaluate_arg` returns `None` and no
        // sidecar is written — preserving the original single-file behaviour.
        let metadata = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 2);
        match metadata {
            NetworkResult::None => {}
            NetworkResult::Error(err) => return EvalOutput::single(NetworkResult::Error(err)),
            record => {
                if let Err(err) = write_generation_parameters_sidecar(&resolved_path, &record) {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "Saved XYZ but failed to write generation-parameters sidecar for '{}': {}",
                        file_name, err
                    )));
                }
            }
        }

        EvalOutput::single(NetworkResult::Unit)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        if connected_input_pins.contains("file_name") {
            // The wired input drives the path — no static subtitle.
            None
        } else if self.file_name.is_empty() {
            // Recover the eager UX feedback that previously came from the
            // runtime check inside `eval` (which now defers to Execute under
            // the central skip rule). See `doc/design_node_execution.md`
            // ("Tradeoff: lost runtime input feedback on Unit nodes").
            Some("(no file name)".to_string())
        } else {
            Some(self.file_name.clone())
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![(
            "file_name".to_string(),
            TextValue::String(self.file_name.clone()),
        )]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("file_name") {
            self.file_name = v
                .as_string()
                .ok_or_else(|| "file_name must be a string".to_string())?
                .to_string();
        }
        Ok(())
    }
}

impl Default for ExportXYZData {
    fn default() -> Self {
        Self::new()
    }
}

impl ExportXYZData {
    pub fn new() -> Self {
        Self {
            file_name: String::new(),
        }
    }
}

/// Special saver for ExportXYZData that converts file path to relative before saving
pub fn export_xyz_data_saver(
    node_data: &mut dyn NodeData,
    design_dir: Option<&str>,
) -> io::Result<Value> {
    if let Some(data) = node_data.as_any_mut().downcast_mut::<ExportXYZData>() {
        // If there's a file name and design directory, try to convert to relative path
        if let (Some(design_dir), file_name) = (design_dir, &data.file_name)
            && !file_name.is_empty()
        {
            let (potentially_relative_path, should_update) =
                try_make_relative(file_name, Some(design_dir));
            if should_update {
                // Update the stored path to use relative path for better portability
                data.file_name = potentially_relative_path;
            }
        }

        // Now serialize the (potentially modified) data
        serde_json::to_value(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data type mismatch for export_xyz",
        ))
    }
}

/// Special loader for ExportXYZData that loads the data after deserializing
pub fn export_xyz_data_loader(
    value: &Value,
    _design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    // Simply deserialize the data - no special loading needed for export
    let data: ExportXYZData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok(Box::new(data))
}

/// Writes the generation-parameters sidecar next to an exported XYZ file.
///
/// For `foo.xyz` the sidecar is `foo.xyz.params.json`. It records the wired
/// metadata record (as JSON), a BLAKE3 hash of the just-written XYZ file so the
/// pairing can be verified later, and a versioned `format` tag leaving room for
/// future fields (e.g. a hash of the generating network).
fn write_generation_parameters_sidecar(xyz_path: &str, metadata: &NetworkResult) -> io::Result<()> {
    // Hash the bytes we just wrote so the sidecar verifiably pins this XYZ.
    let xyz_bytes = std::fs::read(xyz_path)?;
    let xyz_blake3 = blake3::hash(&xyz_bytes).to_hex().to_string();

    let xyz_file_name = std::path::Path::new(xyz_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    let doc = serde_json::json!({
        "format": "atomcad-generation-parameters",
        "version": 1,
        "xyz_file": xyz_file_name,
        "xyz_blake3": xyz_blake3,
        "parameters": network_result_to_json(metadata),
    });

    let sidecar_path = format!("{}.params.json", xyz_path);
    let json_text = serde_json::to_string_pretty(&doc)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    std::fs::write(&sidecar_path, json_text)
}

/// Best-effort, total conversion of a `NetworkResult` to JSON for the sidecar.
///
/// Values with a clean JSON shape (primitives, vectors, matrices, arrays,
/// records) map structurally; everything else (molecules, crystals, functions,
/// iterators, …) falls back to its `to_display_string()` representation. This
/// never errors — a non-serializable field is recorded as a descriptive string
/// (for atomic types this already includes the molecular formula and atom/bond
/// counts) rather than failing the export.
fn network_result_to_json(value: &NetworkResult) -> Value {
    match value {
        NetworkResult::Bool(b) => Value::Bool(*b),
        NetworkResult::Int(i) => Value::from(*i),
        NetworkResult::Float(f) => Value::from(*f),
        NetworkResult::String(s) => Value::String(s.clone()),
        NetworkResult::Vec2(v) => Value::from(vec![v.x, v.y]),
        NetworkResult::Vec3(v) => Value::from(vec![v.x, v.y, v.z]),
        NetworkResult::IVec2(v) => Value::from(vec![v.x, v.y]),
        NetworkResult::IVec3(v) => Value::from(vec![v.x, v.y, v.z]),
        NetworkResult::IMat2(m) => Value::from(m.iter().map(|r| r.to_vec()).collect::<Vec<_>>()),
        NetworkResult::IMat3(m) => Value::from(m.iter().map(|r| r.to_vec()).collect::<Vec<_>>()),
        NetworkResult::Mat3(m) => {
            let rows = dmat3_to_rows(m);
            Value::from(rows.iter().map(|r| r.to_vec()).collect::<Vec<_>>())
        }
        NetworkResult::Array(elements) => {
            Value::Array(elements.iter().map(network_result_to_json).collect())
        }
        NetworkResult::Record(fields) => {
            let map = fields
                .iter()
                .map(|(name, v)| (name.clone(), network_result_to_json(v)))
                .collect::<serde_json::Map<String, Value>>();
            Value::Object(map)
        }
        other => Value::String(other.to_display_string()),
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "export_xyz".to_string(),
        description: "Exports atomic structure on its `molecule` input into an XYZ file. \
            When a record is wired into the optional `metadata` pin, also writes a \
            `<file>.xyz.params.json` sidecar containing those parameters plus a BLAKE3 \
            hash of the XYZ file for machine-readable verification."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::HasAtoms,
            },
            Parameter {
                id: None,
                name: "file_name".to_string(),
                data_type: DataType::String,
            },
            Parameter {
                id: None,
                name: "metadata".to_string(),
                // Empty anonymous record: accepts any record value via width
                // subtyping; all fields pass through unchanged at eval. Wire
                // generation parameters here to emit a `<file>.params.json`
                // sidecar alongside the exported XYZ.
                data_type: DataType::Record(RecordType::anonymous(vec![])),
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Unit),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(ExportXYZData::new()),
        node_data_saver: export_xyz_data_saver,
        node_data_loader: export_xyz_data_loader,
    }
}
