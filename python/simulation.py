"""
Energy minimization simulation module for atomCAD.

This module contains functions for performing energy minimization
using OpenMM with OpenFF force fields and UFF as a fallback.
"""

import os
import sys
import time
from pathlib import Path

# OpenMM imports
try:
    from openmm.app import *
    from openmm import *
    from openmm.unit import *
    OPENMM_AVAILABLE = True
except ImportError as e:
    print(f"Warning: OpenMM not available: {e}")
    OPENMM_AVAILABLE = False

# OpenFF imports - test each one individually
OPENFF_AVAILABLE = False
OPENFF_FORCEFIELD_AVAILABLE = False
OPENFF_MOLECULE_AVAILABLE = False
OPENFF_TOPOLOGY_AVAILABLE = False

try:
    import openff.toolkit
    print(f"OpenFF toolkit base import successful: {openff.toolkit.__version__}")
    
    # Test individual imports
    try:
        from openff.toolkit import ForceField
        OPENFF_FORCEFIELD_AVAILABLE = True
        print("ForceField import successful")
    except ImportError as e:
        print(f"ForceField import failed: {e}")
    
    try:
        from openff.toolkit import Molecule
        OPENFF_MOLECULE_AVAILABLE = True
        print("Molecule import successful")
    except ImportError as e:
        print(f"Molecule import failed: {e}")
    
    try:
        from openff.toolkit import Topology
        OPENFF_TOPOLOGY_AVAILABLE = True
        print("Topology import successful")
    except ImportError as e:
        print(f"Topology import failed: {e}")
    
    # Only mark as available if ForceField works (minimum requirement)
    OPENFF_AVAILABLE = OPENFF_FORCEFIELD_AVAILABLE
    
except ImportError as e:
    print(f"Warning: OpenFF toolkit base import failed: {e}")
    OPENFF_AVAILABLE = False

# Global force field instance
_force_field = None

def _create_error_result(message):
    """
    Create a standardized error result dictionary.
    
    Args:
        message: Error message string
        
    Returns:
        Dictionary with error structure
    """
    return {
        "success": False,
        "positions": [],
        "energy": 0.0,
        "iterations": 0,
        "message": message
    }

def _get_project_root():
    """Get the project root directory (parent of python folder)."""
    current_dir = Path(__file__).parent
    return current_dir.parent

def _load_force_field():
    """Load the OpenFF force field from the resources/forcefields directory."""
    global _force_field
    
    if not OPENFF_FORCEFIELD_AVAILABLE:
        raise RuntimeError("OpenFF ForceField class is not available")
    
    if _force_field is not None:
        print(f"[TIMING] Force field loading: using cached force field")
        return _force_field
    
    # Path to the OpenFF force field file
    project_root = _get_project_root()
    force_field_path = project_root / "resources" / "forcefields" / "openff-2.2.1.offxml"
    
    if not force_field_path.exists():
        raise FileNotFoundError(f"Force field file not found: {force_field_path}")
    
    try:
        # Import ForceField here to avoid issues with module-level imports
        from openff.toolkit import ForceField
        
        # Load the OpenFF force field
        start_time = time.time()
        _force_field = ForceField(str(force_field_path))
        load_time = time.time() - start_time
        print(f"[TIMING] Force field loading from file took {load_time:.3f} seconds")
        print(f"Successfully loaded OpenFF force field: {force_field_path}")
        return _force_field
    except Exception as e:
        raise RuntimeError(f"Failed to load OpenFF force field: {e}")

def initialize_simulation():
    """
    Initialize the simulation environment by pre-loading the force field and warming up the Python runtime.
    This should be called once at application startup to avoid the expensive initialization cost
    during the first energy minimization.
    
    Returns:
        Dictionary with:
            - success: bool
            - message: str (status message)
            - initialization_time: float (seconds)
    """
    start_time = time.time()
    print(f"[TIMING] Simulation initialization started")
    
    try:
        # Check if required libraries are available
        if not OPENMM_AVAILABLE:
            return {
                "success": False,
                "message": "Error: OpenMM is not installed or available",
                "initialization_time": time.time() - start_time
            }
        
        if not OPENFF_AVAILABLE:
            return {
                "success": False,
                "message": "Error: OpenFF toolkit is not installed or available", 
                "initialization_time": time.time() - start_time
            }
        
        # Pre-load the force field to cache it
        force_field = _load_force_field()
        
        # Import commonly used modules to warm up the Python runtime
        from openff.toolkit import Molecule, Topology
        from openff.interchange import Interchange
        from openff.toolkit.utils.toolkits import RDKitToolkitWrapper
        import numpy as np
        
        initialization_time = time.time() - start_time
        print(f"[TIMING] Simulation initialization completed in {initialization_time:.3f} seconds")
        
        return {
            "success": True,
            "message": f"Simulation initialized successfully. Force field loaded with {len(force_field._parameter_handlers)} parameter handlers",
            "initialization_time": initialization_time
        }
        
    except Exception as e:
        initialization_time = time.time() - start_time
        print(f"[TIMING] Simulation initialization failed in {initialization_time:.3f} seconds")
        return {
            "success": False,
            "message": f"Initialization failed: {str(e)}",
            "initialization_time": initialization_time
        }

def minimize_energy(atoms=None, bonds=None, options=None):
    """
    Energy minimization function using OpenMM and OpenFF.
    
    Args:
        atoms: List of dictionaries with:
            - atomic_number: int (1=H, 6=C, 7=N, 8=O, etc.)
            - position: [x, y, z] (in Angstroms)
            - formal_charge: int (optional, defaults to 0)
        
        bonds: List of dictionaries with:
            - atom1: int (index into atoms array)
            - atom2: int (index into atoms array) 
            - order: int (1=single, 2=double, 3=triple)
        
        options: Dictionary with:
            - max_iterations: int (default 1000)
    
    Returns:
        Dictionary with:
            - success: bool
            - positions: [[x, y, z], ...] (optimized coordinates in Angstroms)
            - energy: float (final energy in kJ/mol)
            - iterations: int (number of iterations used)
            - message: str (status/error message)
    """
    start_time = time.time()
    print(f"[TIMING] Python minimize_energy function started")
    try:
        # Check if required libraries are available
        if not OPENMM_AVAILABLE:
            return _create_error_result("Error: OpenMM is not installed or available")
        
        if not OPENFF_AVAILABLE:
            return _create_error_result("Error: OpenFF toolkit is not installed or available")
        
        # If no molecular data provided, just test the force field loading
        if atoms is None or bonds is None:
            force_field = _load_force_field()
            total_time = time.time() - start_time
            print(f"[TIMING] Python minimize_energy (force field test only) took {total_time:.3f} seconds")
            return {
                "success": True,
                "positions": [],
                "energy": 0.0,
                "iterations": 0,
                "message": f"Success: OpenFF force field loaded with {len(force_field._parameter_handlers)} parameter handlers"
            }
        
        # Perform actual energy minimization
        result = _perform_minimization(atoms, bonds, options or {})
        total_time = time.time() - start_time
        print(f"[TIMING] Python minimize_energy (complete) took {total_time:.3f} seconds")
        return result
        
    except Exception as e:
        total_time = time.time() - start_time
        print(f"[TIMING] Python minimize_energy (error) took {total_time:.3f} seconds")
        return _create_error_result(f"Error: {str(e)}")

def _perform_minimization(atoms, bonds, options):
    """
    Perform the actual energy minimization using OpenMM and OpenFF.
    """
    from openff.toolkit import Molecule, Topology
    from openff.interchange import Interchange
    import numpy as np
    
    print(f"[TIMING] Starting energy minimization for {len(atoms)} atoms and {len(bonds)} bonds")
    
    # Set default options
    max_iterations = options.get('max_iterations', 1000)
    tolerance = options.get('tolerance', 1e-6)
    
    # Create OpenFF molecule from input data
    start_time = time.time()
    molecule = _create_openff_molecule(atoms, bonds)
    molecule_time = time.time() - start_time
    print(f"[TIMING] OpenFF molecule creation took {molecule_time:.3f} seconds")
    
    # Add conformer (3D coordinates) - needed for proper force field parameter assignment
    # Use OpenFF's Quantity for proper unit handling
    from openff.units import unit as openff_unit
    positions_array = []
    for atom in atoms:
        pos = atom['position']
        positions_array.append([pos[0], pos[1], pos[2]])
    
    # Create conformer with OpenFF units (not OpenMM units)
    conformer = np.array(positions_array) * openff_unit.angstrom
    molecule._conformers = [conformer]
    
    # Assign partial charges using available methods (following MSEP pattern)
    start_time = time.time()
    from openff.toolkit.utils.toolkits import RDKitToolkitWrapper
    
    try:
        # Try MMFF94 charges first (available via RDKit)
        molecule.assign_partial_charges(partial_charge_method="mmff94", toolkit_registry=RDKitToolkitWrapper())
        charge_method = "mmff94"
    except Exception as e:
        print(f"Failed to assign partial charges with method 'mmff94'. Fallback to 'gasteiger': {e}")
        try:
            # Fallback to Gasteiger charges
            molecule.assign_partial_charges(partial_charge_method="gasteiger", toolkit_registry=RDKitToolkitWrapper())
            charge_method = "gasteiger"
        except Exception as e:
            # If both fail, manually set charges to formal charges (or 0.0)
            print(f"Failed to assign partial charges with method 'gasteiger'. Using formal charges: {e}")
            partial_charges = []
            for atom in atoms:
                formal_charge = atom.get('formal_charge', 0)
                partial_charges.append(float(formal_charge))
            
            from openmm.unit import elementary_charge
            molecule.partial_charges = np.array(partial_charges) * elementary_charge
            charge_method = "formal_charges"
    
    charge_time = time.time() - start_time
    print(f"[TIMING] Partial charge assignment ({charge_method}) took {charge_time:.3f} seconds")
    
    # Load force field and create system
    force_field = _load_force_field()
    topology = Topology.from_molecules([molecule])
    
    # Create interchange with charge_from_molecules to use our pre-assigned charges
    start_time = time.time()
    interchange = Interchange.from_smirnoff(
        force_field, 
        topology, 
        charge_from_molecules=[molecule]
    )
    
    # Convert to OpenMM
    openmm_system = interchange.to_openmm()
    openmm_topology = interchange.to_openmm_topology()
    system_time = time.time() - start_time
    print(f"[TIMING] OpenMM system setup took {system_time:.3f} seconds")
    
    # Set up minimization
    integrator = LangevinMiddleIntegrator(300*kelvin, 1/picosecond, 0.004*picoseconds)
    simulation = Simulation(openmm_topology, openmm_system, integrator)
    
    # Set initial positions
    positions = []
    for atom in atoms:
        pos = atom['position']
        positions.append([pos[0], pos[1], pos[2]])
    
    simulation.context.setPositions(np.array(positions) * angstrom)
    
    # Minimize energy (following MSEP pattern - no tolerance parameter)

# Get initial energy before minimization
    initial_state = simulation.context.getState(getEnergy=True)
    start_energy = initial_state.getPotentialEnergy().value_in_unit(kilojoules_per_mole)

    # OpenMM tolerance expects force units (kJ/mol/nm), not energy units (kJ/mol)
    # MSEP doesn't pass tolerance, so we'll follow their approach
    start_time = time.time()
    simulation.minimizeEnergy(maxIterations=max_iterations)
    minimization_time = time.time() - start_time
    print(f"[TIMING] Actual energy minimization took {minimization_time:.3f} seconds")
    
    # Get results
    state = simulation.context.getState(getPositions=True, getEnergy=True)
    final_positions = state.getPositions(asNumpy=True).value_in_unit(angstrom)
    final_energy = state.getPotentialEnergy().value_in_unit(kilojoules_per_mole)

    msg = f"Energy minimization completed successfully. Initial energy: {start_energy:.2f} kJ/mol, Final energy: {final_energy:.2f} kJ/mol"
    print(msg)

    result = {
        "success": True,
        "positions": final_positions.tolist(),
        "energy": float(final_energy),
        "iterations": max_iterations,  # OpenMM doesn't report actual iterations used
        "message": msg
    }
    
    print(f"[TIMING] Energy minimization result processing completed")
    return result

def _create_openff_molecule(atoms, bonds):
    """
    Create an OpenFF Molecule object from atoms and bonds data.
    """
    from openff.toolkit import Molecule
    import numpy as np
    
    # Create empty molecule
    molecule = Molecule()
    
    # Add atoms
    atom_indices = []
    for atom in atoms:
        atomic_number = atom['atomic_number']
        formal_charge = atom.get('formal_charge', 0)
        is_aromatic = False  # Default to non-aromatic; could be enhanced later
        
        atom_idx = molecule.add_atom(
            atomic_number=atomic_number,
            formal_charge=formal_charge,
            is_aromatic=is_aromatic
        )
        atom_indices.append(atom_idx)
    
    # Add bonds
    for bond in bonds:
        atom1_idx = bond['atom1']
        atom2_idx = bond['atom2']
        bond_order = bond['order']
        is_aromatic = False  # Default to non-aromatic; could be enhanced later
        
        molecule.add_bond(
            atom1=atom1_idx,
            atom2=atom2_idx,
            bond_order=bond_order,
            is_aromatic=is_aromatic
        )
    
    return molecule
