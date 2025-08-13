"""
Energy minimization simulation module for atomCAD.

This module contains functions for performing energy minimization
using OpenMM with OpenFF force fields and UFF as a fallback.
"""

import os
import sys
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
        _force_field = ForceField(str(force_field_path))
        print(f"Successfully loaded OpenFF force field: {force_field_path}")
        return _force_field
    except Exception as e:
        raise RuntimeError(f"Failed to load OpenFF force field: {e}")

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
            - tolerance: float (default 1e-6)
    
    Returns:
        Dictionary with:
            - success: bool
            - positions: [[x, y, z], ...] (optimized coordinates in Angstroms)
            - energy: float (final energy in kJ/mol)
            - iterations: int (number of iterations used)
            - message: str (status/error message)
    """
    try:
        # Check if required libraries are available
        if not OPENMM_AVAILABLE:
            return _create_error_result("Error: OpenMM is not installed or available")
        
        if not OPENFF_AVAILABLE:
            return _create_error_result("Error: OpenFF toolkit is not installed or available")
        
        # If no molecular data provided, just test the force field loading
        if atoms is None or bonds is None:
            force_field = _load_force_field()
            return {
                "success": True,
                "positions": [],
                "energy": 0.0,
                "iterations": 0,
                "message": f"Success: OpenFF force field loaded with {len(force_field._parameter_handlers)} parameter handlers"
            }
        
        # Perform actual energy minimization
        return _perform_minimization(atoms, bonds, options or {})
        
    except Exception as e:
        return _create_error_result(f"Error: {str(e)}")

def _perform_minimization(atoms, bonds, options):
    """
    Perform the actual energy minimization using OpenMM and OpenFF.
    """
    from openff.toolkit import Molecule, Topology
    from openff.interchange import Interchange
    import numpy as np
    
    # Set default options
    max_iterations = options.get('max_iterations', 1000)
    tolerance = options.get('tolerance', 1e-6)
    
    # Create OpenFF molecule from input data
    molecule = _create_openff_molecule(atoms, bonds)
    
    # Assign partial charges using available method (not AM1-BCC on Windows)
    try:
        # Try MMFF94 charges first (available via RDKit)
        molecule.assign_partial_charges(partial_charge_method="mmff94")
    except Exception:
        try:
            # Fallback to Gasteiger charges
            molecule.assign_partial_charges(partial_charge_method="gasteiger")
        except Exception:
            # Last resort: use formal charges only
            molecule.assign_partial_charges(partial_charge_method="formal_charge")
    
    # Load force field and create system
    force_field = _load_force_field()
    topology = Topology.from_molecules([molecule])
    interchange = Interchange.from_smirnoff(force_field, topology)
    
    # Convert to OpenMM
    openmm_system = interchange.to_openmm()
    openmm_topology = interchange.to_openmm_topology()
    
    # Set up minimization
    integrator = LangevinMiddleIntegrator(300*kelvin, 1/picosecond, 0.004*picoseconds)
    simulation = Simulation(openmm_topology, openmm_system, integrator)
    
    # Set initial positions
    positions = []
    for atom in atoms:
        pos = atom['position']
        positions.append([pos[0], pos[1], pos[2]])
    
    simulation.context.setPositions(np.array(positions) * angstrom)
    
    # Minimize energy
    simulation.minimizeEnergy(tolerance=tolerance*kilojoules_per_mole, maxIterations=max_iterations)
    
    # Get results
    state = simulation.context.getState(getPositions=True, getEnergy=True)
    final_positions = state.getPositions(asNumpy=True).value_in_unit(angstrom)
    final_energy = state.getPotentialEnergy().value_in_unit(kilojoules_per_mole)
    
    return {
        "success": True,
        "positions": final_positions.tolist(),
        "energy": float(final_energy),
        "iterations": max_iterations,  # OpenMM doesn't report actual iterations used
        "message": f"Energy minimization completed successfully. Final energy: {final_energy:.2f} kJ/mol"
    }

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
