use crate::common::atomic_structure::AtomicStructure;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use glam::DVec3;
use crate::common::atomic_structure_utils::print_atom_info;
use crate::util::timer::Timer;

#[pyclass]
struct LoggingStdout;

#[pymethods]
impl LoggingStdout {
    fn write(&self, data: &str) {
        print!("{}", data);
    }
    
    fn flush(&self) {
        // Optional: implement flush for completeness
    }
}

/// Initializes the Python simulation environment by pre-loading the force field.
/// This should be called once at application startup to avoid expensive initialization
/// during the first energy minimization call.
pub fn initialize_simulation() -> Result<String, String> {
    let _timer = Timer::new("Simulation initialization");
    
    Python::with_gil(|py| {
        // Set up stdout redirection to capture Python print statements
        let sys = py.import_bound("sys").map_err(|e| format!("Failed to import sys: {}", e))?;
        sys.setattr("stdout", LoggingStdout.into_py(py))
            .map_err(|e| format!("Failed to redirect stdout: {}", e))?;
        
        // Add the python directory to the Python path
        let path = sys.getattr("path").map_err(|e| format!("Failed to get sys.path: {}", e))?;
        
        path.call_method1("append", ("python",))
            .map_err(|e| format!("Failed to add python directory to sys.path: {}", e))?;
        
        // Import simulation module
        let simulation_module = py.import_bound("simulation")
            .map_err(|e| format!("Failed to import simulation module: {}", e))?;
        
        // Call the initialize_simulation function
        let result = simulation_module.call_method0("initialize_simulation")
            .map_err(|e| format!("Failed to call initialize_simulation: {}", e))?;
        
        // Extract the result dictionary
        let result_dict = result.downcast::<PyDict>()
            .map_err(|e| format!("Result is not a dictionary: {}", e))?;
        
        let success: bool = result_dict.get_item("success")
            .map_err(|e| format!("Failed to get 'success' field: {}", e))?
            .ok_or("Missing 'success' field")?
            .extract()
            .map_err(|e| format!("Failed to extract 'success': {}", e))?;
        
        let message: String = result_dict.get_item("message")
            .map_err(|e| format!("Failed to get 'message' field: {}", e))?
            .ok_or("Missing 'message' field")?
            .extract()
            .map_err(|e| format!("Failed to extract 'message': {}", e))?;
        
        if success {
            println!("Simulation initialization successful: {}", message);
            Ok(message)
        } else {
            Err(message)
        }
    })
}

/// Performs energy minimization on an atomic structure using Python-based force fields.
/// 
/// This function integrates with Python libraries (OpenMM, OpenFF) to perform
/// energy minimization using OpenFF 2.2.1 force fields.
/// 
/// # Arguments
/// 
/// * `structure` - A mutable reference to the atomic structure to minimize
/// 
/// # Returns
/// 
/// Returns `Ok(MinimizationResult)` with energy, iterations, and message if successful, 
/// or an error if it failed. The function updates the atom positions in the AtomicStructure 
/// with the minimized coordinates.
pub fn minimize_energy(structure: &mut AtomicStructure) -> Result<MinimizationResult, String> {
    let _total_timer = Timer::new("Total energy minimization");
    println!("Energy minimization called on structure with {} atoms", 
             structure.get_num_of_atoms());
    
    // Extract molecular data from AtomicStructure
    let (atoms_data, bonds_data) = {
        let _timer = Timer::new("Data extraction from AtomicStructure");
        extract_molecular_data(structure)?
    };
    
    // Call Python energy minimization
    match call_python_minimize_energy_with_data(atoms_data, bonds_data) {
        Ok(result) => {
            if result.success {
                // Update atom positions with minimized coordinates
                {
                    let _timer = Timer::new("Position updates back to AtomicStructure");
                    update_atom_positions(structure, &result.positions)?;
                }
                println!("Energy minimization successful! Final energy: {:.2} kJ/mol", result.energy);
                Ok(result)
            } else {
                println!("Energy minimization failed: {}", result.message);
                print_atom_info(structure);
                Err(format!("Energy minimization failed: {}", result.message))
            }
        }
        Err(e) => {
            Err(format!("Failed to call Python simulation: {}", e))
        }
    }
}

/// Result structure returned from Python energy minimization
#[derive(Debug)]
pub struct MinimizationResult {
    pub success: bool,
    pub positions: Vec<Vec<f64>>,
    pub energy: f64,
    pub iterations: i32,
    pub message: String,
}

/// Atom data structure for Python interface
#[derive(Debug)]
struct AtomData {
    atomic_number: i32,
    position: [f64; 3],
    formal_charge: i32,
}

/// Bond data structure for Python interface
#[derive(Debug)]
struct BondData {
    atom1: usize,
    atom2: usize,
    order: i32,
}

/// Extracts molecular data from AtomicStructure for Python interface
fn extract_molecular_data(structure: &AtomicStructure) -> Result<(Vec<AtomData>, Vec<BondData>), String> {
    // Create atom data - iterate through atoms HashMap
    let mut atoms_data = Vec::new();
    let mut atom_id_to_index = std::collections::HashMap::new();
    
    // Collect atom IDs to ensure consistent ordering
    let atom_ids: Vec<u64> = structure.atoms.keys().cloned().collect();
    
    for (index, &atom_id) in atom_ids.iter().enumerate() {
        if let Some(atom) = structure.get_atom(atom_id) {
            atom_id_to_index.insert(atom_id, index);
            atoms_data.push(AtomData {
                atomic_number: atom.atomic_number,
                position: [atom.position.x, atom.position.y, atom.position.z],
                formal_charge: 0, // Default to 0, could be enhanced later
            });
        }
    }
    
    // Create bond data - iterate through bonds HashMap
    let mut bonds_data = Vec::new();
    for bond in structure.bonds.values() {
        let atom1_index = atom_id_to_index.get(&bond.atom_id1)
            .ok_or_else(|| format!("Atom ID {} not found for bond", bond.atom_id1))?;
        let atom2_index = atom_id_to_index.get(&bond.atom_id2)
            .ok_or_else(|| format!("Atom ID {} not found for bond", bond.atom_id2))?;
        
        bonds_data.push(BondData {
            atom1: *atom1_index,
            atom2: *atom2_index,
            order: bond.multiplicity,
        });
    }
    
    Ok((atoms_data, bonds_data))
}

/// Updates atom positions in AtomicStructure with minimized coordinates
fn update_atom_positions(structure: &mut AtomicStructure, positions: &[Vec<f64>]) -> Result<(), String> {
    // Get atom IDs in the same order as we extracted them
    let atom_ids: Vec<u64> = structure.atoms.keys().cloned().collect();
    
    if atom_ids.len() != positions.len() {
        return Err(format!("Position count mismatch: {} atoms vs {} positions", 
                          atom_ids.len(), positions.len()));
    }
    
    for (atom_id, pos) in atom_ids.iter().zip(positions.iter()) {
        if pos.len() != 3 {
            return Err(format!("Invalid position format: expected 3 coordinates, got {}", pos.len()));
        }
        
        let new_position = DVec3::new(pos[0], pos[1], pos[2]);
        if !structure.set_atom_position(*atom_id, new_position) {
            return Err(format!("Failed to update position for atom ID {}", atom_id));
        }
    }
    
    Ok(())
}

/// Calls the minimize_energy function from Python simulation module with molecular data
fn call_python_minimize_energy_with_data(
    atoms_data: Vec<AtomData>, 
    bonds_data: Vec<BondData>
) -> Result<MinimizationResult, String> {
    let _timer = Timer::new("Python function call (total)");
    Python::with_gil(|py| {
        // Add the python directory to the Python path so we can import our module
        let simulation_module = {
            let _timer = Timer::new("Python runtime init and module import");
            let sys = py.import_bound("sys").map_err(|e| format!("Failed to import sys: {}", e))?;
            
            // Set up stdout redirection to capture Python print statements
            sys.setattr("stdout", LoggingStdout.into_py(py))
                .map_err(|e| format!("Failed to redirect stdout: {}", e))?;
            
            let path = sys.getattr("path").map_err(|e| format!("Failed to get sys.path: {}", e))?;
            
            // Add the python directory to sys.path (assuming we're running from the project root)
            path.call_method1("append", ("python",))
                .map_err(|e| format!("Failed to add python directory to sys.path: {}", e))?;
            
            // Import our simulation module
            py.import_bound("simulation")
                .map_err(|e| format!("Failed to import simulation module: {}", e))?
        };
        
        // Convert atoms data to Python format
        let (atoms_list, bonds_list) = {
            let _timer = Timer::new("Data serialization to Python format");
            let atoms_list = PyList::empty_bound(py);
            for atom in atoms_data {
                let atom_dict = PyDict::new_bound(py);
                atom_dict.set_item("atomic_number", atom.atomic_number)
                    .map_err(|e| format!("Failed to set atomic_number: {}", e))?;
                atom_dict.set_item("position", atom.position.to_vec())
                    .map_err(|e| format!("Failed to set position: {}", e))?;
                atom_dict.set_item("formal_charge", atom.formal_charge)
                    .map_err(|e| format!("Failed to set formal_charge: {}", e))?;
                atoms_list.append(atom_dict)
                    .map_err(|e| format!("Failed to append atom dict: {}", e))?;
            }
            
            // Convert bonds data to Python format
            let bonds_list = PyList::empty_bound(py);
            for bond in bonds_data {
                let bond_dict = PyDict::new_bound(py);
                bond_dict.set_item("atom1", bond.atom1)
                    .map_err(|e| format!("Failed to set atom1: {}", e))?;
                bond_dict.set_item("atom2", bond.atom2)
                    .map_err(|e| format!("Failed to set atom2: {}", e))?;
                bond_dict.set_item("order", bond.order)
                    .map_err(|e| format!("Failed to set order: {}", e))?;
                bonds_list.append(bond_dict)
                    .map_err(|e| format!("Failed to append bond dict: {}", e))?;
            }
            (atoms_list, bonds_list)
        };
        
        // Call the minimize_energy function with molecular data
        let result = {
            let _timer = Timer::new("Python minimize_energy function call");
            simulation_module.call_method1("minimize_energy", (atoms_list, bonds_list))
                .map_err(|e| format!("Failed to call minimize_energy: {}", e))?
        };
        
        // Extract the result dictionary
        let result_dict = {
            let _timer = Timer::new("Result deserialization from Python");
            result.downcast::<PyDict>()
                .map_err(|e| format!("Result is not a dictionary: {}", e))?
        };
        
        // Extract individual fields from the result
        let success: bool = result_dict.get_item("success")
            .map_err(|e| format!("Failed to get 'success' field: {}", e))?
            .ok_or("Missing 'success' field")?
            .extract()
            .map_err(|e| format!("Failed to extract 'success': {}", e))?;
        
        let positions: Vec<Vec<f64>> = result_dict.get_item("positions")
            .map_err(|e| format!("Failed to get 'positions' field: {}", e))?
            .ok_or("Missing 'positions' field")?
            .extract()
            .map_err(|e| format!("Failed to extract 'positions': {}", e))?;
        
        let energy: f64 = result_dict.get_item("energy")
            .map_err(|e| format!("Failed to get 'energy' field: {}", e))?
            .ok_or("Missing 'energy' field")?
            .extract()
            .map_err(|e| format!("Failed to extract 'energy': {}", e))?;
        
        let iterations: i32 = result_dict.get_item("iterations")
            .map_err(|e| format!("Failed to get 'iterations' field: {}", e))?
            .ok_or("Missing 'iterations' field")?
            .extract()
            .map_err(|e| format!("Failed to extract 'iterations': {}", e))?;
        
        let message: String = result_dict.get_item("message")
            .map_err(|e| format!("Failed to get 'message' field: {}", e))?
            .ok_or("Missing 'message' field")?
            .extract()
            .map_err(|e| format!("Failed to extract 'message': {}", e))?;
        
        Ok(MinimizationResult {
            success,
            positions,
            energy,
            iterations,
            message,
        })
    })
}
