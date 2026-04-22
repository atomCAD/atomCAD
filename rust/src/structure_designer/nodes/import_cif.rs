use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure_utils::auto_create_bonds_with_tolerance;
use crate::crystolecule::io::cif::structure::CifBond;
use crate::crystolecule::io::cif::symmetry::{CifAtomSite, SymmetryOperation};
use crate::crystolecule::io::cif::{CifLoadResultExtended, load_cif_extended};
use crate::crystolecule::motif::{Motif, MotifBond, ParameterElement, Site, SiteSpecifier};
use crate::crystolecule::motif_bond_inference::infer_motif_bonds;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{MoleculeData, NetworkResult};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{NodeType, OutputPinDefinition, Parameter};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::path_utils::{get_parent_directory, resolve_path, try_make_relative};
use glam::DVec3;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportCifData {
    pub file_name: Option<String>,
    pub block_name: Option<String>,
    pub use_cif_bonds: bool,
    pub infer_bonds: bool,
    pub bond_tolerance: f64,

    #[serde(skip)]
    pub cached_result: Option<CifImportResult>,
}

#[derive(Debug, Clone)]
pub struct CifImportResult {
    pub unit_cell: UnitCellStruct,
    pub atomic_structure: AtomicStructure,
    pub motif: Motif,
}

impl NodeData for ImportCifData {
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
        // If we have a cached result, use it directly
        if let Some(ref cached) = self.cached_result {
            return build_eval_output(cached);
        }

        // Get file_name from parameter 0 or stored value
        let file_name_result =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
        let file_name = if let NetworkResult::None = file_name_result {
            match &self.file_name {
                Some(f) => f.clone(),
                None => {
                    return EvalOutput::multi(vec![
                        NetworkResult::Error("No CIF file specified".to_string()),
                        NetworkResult::Error("No CIF file specified".to_string()),
                        NetworkResult::Error("No CIF file specified".to_string()),
                    ]);
                }
            }
        } else if file_name_result.is_error() {
            return EvalOutput::multi(vec![
                file_name_result.clone(),
                file_name_result.clone(),
                file_name_result,
            ]);
        } else if let NetworkResult::String(s) = file_name_result {
            s
        } else {
            return EvalOutput::multi(vec![
                NetworkResult::Error("Expected string parameter for file name".to_string()),
                NetworkResult::Error("Expected string parameter for file name".to_string()),
                NetworkResult::Error("Expected string parameter for file name".to_string()),
            ]);
        };

        // Get block_name from parameter 1
        let block_name_result =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        let block_name = match block_name_result {
            NetworkResult::String(s) if !s.is_empty() => Some(s),
            NetworkResult::None => self.block_name.clone(),
            _ => self.block_name.clone(),
        };

        // Get use_cif_bonds from parameter 2
        let use_cif_bonds_result =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 2);
        let use_cif_bonds = match use_cif_bonds_result {
            NetworkResult::Bool(b) => b,
            _ => self.use_cif_bonds,
        };

        // Get infer_bonds from parameter 3
        let infer_bonds_result =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 3);
        let infer_bonds = match infer_bonds_result {
            NetworkResult::Bool(b) => b,
            _ => self.infer_bonds,
        };

        // Get bond_tolerance from parameter 4
        let bond_tolerance_result =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 4);
        let bond_tolerance = match bond_tolerance_result {
            NetworkResult::Float(f) => f,
            _ => self.bond_tolerance,
        };

        // Resolve file path
        let design_dir = registry
            .design_file_name
            .as_ref()
            .and_then(|design_path| get_parent_directory(design_path));

        let resolved_path = match resolve_path(&file_name, design_dir.as_deref()) {
            Ok((path, _)) => path,
            Err(_) => {
                let err = NetworkResult::Error(format!("Failed to resolve path: {}", file_name));
                return EvalOutput::multi(vec![err.clone(), err.clone(), err]);
            }
        };

        // Load the CIF file
        let cif_result = match load_cif_extended(&resolved_path, block_name.as_deref()) {
            Ok(r) => r,
            Err(e) => {
                let err = NetworkResult::Error(format!("Failed to load CIF file: {}", e));
                return EvalOutput::multi(vec![err.clone(), err.clone(), err]);
            }
        };

        // Build the import result
        match build_cif_import_result(&cif_result, use_cif_bonds, infer_bonds, bond_tolerance) {
            Ok(import_result) => build_eval_output(&import_result),
            Err(e) => {
                let err = NetworkResult::Error(e);
                EvalOutput::multi(vec![err.clone(), err.clone(), err])
            }
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
        if let Some(ref block_name) = self.block_name {
            props.push((
                "block_name".to_string(),
                TextValue::String(block_name.clone()),
            ));
        }
        if !self.use_cif_bonds {
            props.push(("use_cif_bonds".to_string(), TextValue::Bool(false)));
        }
        if !self.infer_bonds {
            props.push(("infer_bonds".to_string(), TextValue::Bool(false)));
        }
        if (self.bond_tolerance - 1.15).abs() > 1e-10 {
            props.push((
                "bond_tolerance".to_string(),
                TextValue::Float(self.bond_tolerance),
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
        if let Some(v) = props.get("block_name") {
            self.block_name = Some(
                v.as_string()
                    .ok_or_else(|| "block_name must be a string".to_string())?
                    .to_string(),
            );
        }
        if let Some(v) = props.get("use_cif_bonds") {
            self.use_cif_bonds = v
                .as_bool()
                .ok_or_else(|| "use_cif_bonds must be a bool".to_string())?;
        }
        if let Some(v) = props.get("infer_bonds") {
            self.infer_bonds = v
                .as_bool()
                .ok_or_else(|| "infer_bonds must be a bool".to_string())?;
        }
        if let Some(v) = props.get("bond_tolerance") {
            self.bond_tolerance = v
                .as_float()
                .ok_or_else(|| "bond_tolerance must be a float".to_string())?;
        }
        Ok(())
    }
}

impl Default for ImportCifData {
    fn default() -> Self {
        Self::new()
    }
}

impl ImportCifData {
    pub fn new() -> Self {
        Self {
            file_name: None,
            block_name: None,
            use_cif_bonds: true,
            infer_bonds: true,
            bond_tolerance: 1.15,
            cached_result: None,
        }
    }
}

fn build_eval_output(result: &CifImportResult) -> EvalOutput {
    EvalOutput::multi(vec![
        NetworkResult::LatticeVecs(result.unit_cell.clone()),
        NetworkResult::Molecule(MoleculeData {
            atoms: result.atomic_structure.clone(),
            geo_tree_root: None,
        }),
        NetworkResult::Motif(result.motif.clone()),
    ])
}

/// Build the full import result from CIF data: unit cell, atomic structure, and motif.
pub fn build_cif_import_result(
    cif_result: &CifLoadResultExtended,
    use_cif_bonds: bool,
    infer_bonds: bool,
    bond_tolerance: f64,
) -> Result<CifImportResult, String> {
    let unit_cell = cif_result.unit_cell.clone();

    // Build sites (fractional coordinates) for the motif
    let sites: Vec<Site> = cif_result
        .atoms
        .iter()
        .map(|a| Site {
            atomic_number: a.atomic_number,
            position: a.fract,
        })
        .collect();

    // Determine bond source
    let has_cif_bonds = !cif_result.cif_bonds.is_empty();
    let use_explicit_bonds = use_cif_bonds && has_cif_bonds;

    // Build motif bonds
    let motif_bonds = if use_explicit_bonds {
        build_motif_bonds_from_cif(
            &cif_result.cif_bonds,
            &cif_result.asymmetric_atoms,
            &cif_result.symmetry_operations,
            &cif_result.atoms,
            0.01,
        )
    } else if infer_bonds {
        let parameters: Vec<ParameterElement> = Vec::new();
        infer_motif_bonds(&sites, &parameters, &unit_cell, bond_tolerance)
    } else {
        Vec::new()
    };

    // Build motif with precomputed bond indices
    let num_sites = sites.len();
    let mut bonds_by_site1_index: Vec<Vec<usize>> = vec![Vec::new(); num_sites];
    let mut bonds_by_site2_index: Vec<Vec<usize>> = vec![Vec::new(); num_sites];
    for (bond_index, bond) in motif_bonds.iter().enumerate() {
        if bond.site_1.site_index < num_sites {
            bonds_by_site1_index[bond.site_1.site_index].push(bond_index);
        }
        if bond.site_2.site_index < num_sites {
            bonds_by_site2_index[bond.site_2.site_index].push(bond_index);
        }
    }

    let motif = Motif {
        parameters: Vec::new(),
        sites,
        bonds: motif_bonds,
        bonds_by_site1_index,
        bonds_by_site2_index,
    };

    // Build atomic structure (Cartesian coordinates)
    let mut atomic_structure = AtomicStructure::new();
    for atom in &cif_result.atoms {
        let cartesian = unit_cell.dvec3_lattice_to_real(&atom.fract);
        atomic_structure.add_atom(atom.atomic_number, cartesian);
    }

    // Add bonds to atomic structure
    if use_explicit_bonds {
        add_cif_bonds_to_structure(
            &mut atomic_structure,
            &cif_result.cif_bonds,
            &cif_result.asymmetric_atoms,
            &cif_result.symmetry_operations,
            &cif_result.atoms,
            &unit_cell,
            0.01,
        );
    } else if infer_bonds {
        auto_create_bonds_with_tolerance(&mut atomic_structure, bond_tolerance);
    }

    Ok(CifImportResult {
        unit_cell,
        atomic_structure,
        motif,
    })
}

/// Build motif bonds from CIF `_geom_bond_*` data.
///
/// Maps asymmetric unit atom labels + symmetry codes to expanded atom site indices.
fn build_motif_bonds_from_cif(
    cif_bonds: &[CifBond],
    asymmetric_atoms: &[CifAtomSite],
    symmetry_operations: &[SymmetryOperation],
    expanded_atoms: &[crate::crystolecule::io::cif::ExpandedAtomSite],
    tolerance: f64,
) -> Vec<MotifBond> {
    let mut motif_bonds = Vec::new();

    for bond in cif_bonds {
        // Find the expanded atom indices for each bond endpoint
        let idx1 = resolve_cif_bond_atom(
            &bond.atom_label_1,
            &bond.symmetry_code_1,
            asymmetric_atoms,
            symmetry_operations,
            expanded_atoms,
            tolerance,
        );
        let idx2 = resolve_cif_bond_atom(
            &bond.atom_label_2,
            &bond.symmetry_code_2,
            asymmetric_atoms,
            symmetry_operations,
            expanded_atoms,
            tolerance,
        );

        if let (Some((site1, cell1)), Some((site2, cell2))) = (idx1, idx2) {
            // Normalize so site_1 is at cell (0,0,0)
            let relative_cell = cell2 - cell1;
            motif_bonds.push(MotifBond {
                site_1: SiteSpecifier {
                    site_index: site1,
                    relative_cell: glam::IVec3::ZERO,
                },
                site_2: SiteSpecifier {
                    site_index: site2,
                    relative_cell,
                },
                multiplicity: bond.bond_order,
            });
        }
    }

    motif_bonds
}

/// Resolve a CIF bond endpoint (atom label + symmetry code) to an expanded atom index
/// and cell offset.
///
/// Returns `(site_index, cell_offset)` where site_index is into the expanded_atoms list,
/// and cell_offset is an IVec3 representing additional cell translation.
fn resolve_cif_bond_atom(
    label: &str,
    symmetry_code: &Option<String>,
    asymmetric_atoms: &[CifAtomSite],
    symmetry_operations: &[SymmetryOperation],
    expanded_atoms: &[crate::crystolecule::io::cif::ExpandedAtomSite],
    tolerance: f64,
) -> Option<(usize, glam::IVec3)> {
    // Parse the symmetry code (e.g., "2_655" → symop 2, translation (+1,0,0))
    let parsed = symmetry_code.as_ref().and_then(|code| {
        if code == "." {
            None
        } else {
            crate::crystolecule::io::cif::structure::parse_symmetry_code(code)
        }
    });

    // Find the asymmetric atom with this label
    let asym_atom = asymmetric_atoms.iter().find(|a| a.label == label)?;

    // Apply the symmetry operation to get the fractional position.
    // Use apply_unwrapped so we preserve the cell offset information for atoms
    // whose asymmetric unit coordinates lie outside [0,1).
    let fract = if let Some(ref parsed_code) = parsed {
        if parsed_code.symop_index > 0 && parsed_code.symop_index <= symmetry_operations.len() {
            let op = &symmetry_operations[parsed_code.symop_index - 1]; // 1-based index
            let base_fract = op.apply_unwrapped(asym_atom.fract);
            // Add the translation from the symmetry code
            base_fract + parsed_code.translation.as_dvec3()
        } else {
            asym_atom.fract
        }
    } else {
        // No symmetry code or "." — identity operation, same cell
        asym_atom.fract
    };

    // Wrap into [0,1) and compute cell offset
    let cell_offset = glam::IVec3::new(
        fract.x.floor() as i32,
        fract.y.floor() as i32,
        fract.z.floor() as i32,
    );
    let wrapped_fract = DVec3::new(
        fract.x - fract.x.floor(),
        fract.y - fract.y.floor(),
        fract.z - fract.z.floor(),
    );

    // Find the matching expanded atom
    for (i, expanded) in expanded_atoms.iter().enumerate() {
        let dx = (expanded.fract.x - wrapped_fract.x).abs();
        let dy = (expanded.fract.y - wrapped_fract.y).abs();
        let dz = (expanded.fract.z - wrapped_fract.z).abs();
        // Handle wraparound at boundaries
        let dx = dx.min(1.0 - dx);
        let dy = dy.min(1.0 - dy);
        let dz = dz.min(1.0 - dz);
        if (dx * dx + dy * dy + dz * dz).sqrt() < tolerance {
            return Some((i, cell_offset));
        }
    }

    None
}

/// Add CIF bonds to a Cartesian AtomicStructure.
fn add_cif_bonds_to_structure(
    structure: &mut AtomicStructure,
    cif_bonds: &[CifBond],
    asymmetric_atoms: &[CifAtomSite],
    symmetry_operations: &[SymmetryOperation],
    expanded_atoms: &[crate::crystolecule::io::cif::ExpandedAtomSite],
    _unit_cell: &UnitCellStruct,
    tolerance: f64,
) {
    for bond in cif_bonds {
        let idx1 = resolve_cif_bond_atom(
            &bond.atom_label_1,
            &bond.symmetry_code_1,
            asymmetric_atoms,
            symmetry_operations,
            expanded_atoms,
            tolerance,
        );
        let idx2 = resolve_cif_bond_atom(
            &bond.atom_label_2,
            &bond.symmetry_code_2,
            asymmetric_atoms,
            symmetry_operations,
            expanded_atoms,
            tolerance,
        );

        if let (Some((site1, cell1)), Some((site2, cell2))) = (idx1, idx2) {
            // Only add bonds between atoms in the same cell (cell offset 0,0,0)
            // Cross-cell bonds in the Atomic output are handled by the motif + atom_fill pipeline
            if cell1 == glam::IVec3::ZERO && cell2 == glam::IVec3::ZERO {
                let num_atoms = expanded_atoms.len();
                if site1 < num_atoms && site2 < num_atoms {
                    // Atom IDs are 1-indexed (add_atom returns sequential 1-based IDs)
                    let id1 = (site1 + 1) as u32;
                    let id2 = (site2 + 1) as u32;
                    let order = (bond.bond_order as u8).max(1);
                    structure.add_bond(id1, id2, order);
                }
            }
        }
    }
}

/// Special loader for ImportCifData that loads and parses the CIF file after deserializing.
pub fn import_cif_data_loader(
    value: &Value,
    design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let mut data: ImportCifData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // If there's a file name, try to load and build the cached result
    if let Some(ref file_name) = data.file_name {
        match resolve_path(file_name, design_dir) {
            Ok((resolved_path, _)) => {
                match load_cif_extended(&resolved_path, data.block_name.as_deref()) {
                    Ok(cif_result) => {
                        match build_cif_import_result(
                            &cif_result,
                            data.use_cif_bonds,
                            data.infer_bonds,
                            data.bond_tolerance,
                        ) {
                            Ok(import_result) => {
                                data.cached_result = Some(import_result);
                            }
                            Err(_) => {
                                data.cached_result = None;
                            }
                        }
                    }
                    Err(_) => {
                        data.cached_result = None;
                    }
                }
            }
            Err(_) => {
                data.cached_result = None;
            }
        }
    }

    Ok(Box::new(data))
}

/// Special saver for ImportCifData that converts file path to relative before saving.
pub fn import_cif_data_saver(
    node_data: &mut dyn NodeData,
    design_dir: Option<&str>,
) -> io::Result<Value> {
    if let Some(data) = node_data.as_any_mut().downcast_mut::<ImportCifData>() {
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
            "Data type mismatch for import_cif",
        ))
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "import_cif".to_string(),
        description: "Imports a crystal structure from a CIF file. Outputs the unit cell, \
            an atomic structure of the full conventional unit cell, and a motif with \
            fractional coordinates.\n\
            It converts file paths to relative paths whenever possible so that project \
            portability is maintained."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "file_name".to_string(),
                data_type: DataType::String,
            },
            Parameter {
                id: None,
                name: "block_name".to_string(),
                data_type: DataType::String,
            },
            Parameter {
                id: None,
                name: "use_cif_bonds".to_string(),
                data_type: DataType::Bool,
            },
            Parameter {
                id: None,
                name: "infer_bonds".to_string(),
                data_type: DataType::Bool,
            },
            Parameter {
                id: None,
                name: "bond_tolerance".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: vec![
            OutputPinDefinition::fixed("unit_cell", DataType::LatticeVecs),
            // TODO: CIF's crystal lattice/motif info is discarded when emitting a
            // Molecule. Once phase-transition nodes land, this should emit Crystal
            // with an extracted Structure instead.
            OutputPinDefinition::fixed("atoms", DataType::Molecule),
            OutputPinDefinition::fixed("motif", DataType::Motif),
        ],
        public: true,
        node_data_creator: || Box::new(ImportCifData::new()),
        node_data_saver: import_cif_data_saver,
        node_data_loader: import_cif_data_loader,
    }
}
