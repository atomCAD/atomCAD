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

def minimize_energy():
    """
    Energy minimization function using OpenMM and OpenFF.
    
    This function loads the OpenFF 2.2.1 force field and prepares it for
    energy minimization. Currently returns a status message.
    
    Returns:
        str: Status message indicating success or failure
    """
    try:
        # Debug: Print Python path and availability flags
        import sys
        debug_info = []
        debug_info.append(f"Python executable: {sys.executable}")
        debug_info.append(f"Python version: {sys.version}")
        debug_info.append(f"OpenMM available: {OPENMM_AVAILABLE}")
        debug_info.append(f"OpenFF available: {OPENFF_AVAILABLE}")
        debug_info.append(f"OpenFF ForceField available: {OPENFF_FORCEFIELD_AVAILABLE}")
        debug_info.append(f"OpenFF Molecule available: {OPENFF_MOLECULE_AVAILABLE}")
        debug_info.append(f"OpenFF Topology available: {OPENFF_TOPOLOGY_AVAILABLE}")
        debug_info.append("Python sys.path:")
        for i, path in enumerate(sys.path):
            debug_info.append(f"  [{i}] {path}")
        
        # Try to import packages manually for debugging
        try:
            import openmm
            debug_info.append(f"OpenMM import successful: {openmm.__version__}")
        except ImportError as e:
            debug_info.append(f"OpenMM import failed: {e}")
        
        try:
            import openff.toolkit
            debug_info.append(f"OpenFF base import successful: {openff.toolkit.__version__}")
            
            # Now test individual OpenFF components
            try:
                from openff.toolkit import ForceField
                debug_info.append("OpenFF ForceField import successful")
            except ImportError as e:
                debug_info.append(f"OpenFF ForceField import failed: {e}")
            except Exception as e:
                debug_info.append(f"OpenFF ForceField import error: {e}")
            
            try:
                from openff.toolkit import Molecule
                debug_info.append("OpenFF Molecule import successful")
            except ImportError as e:
                debug_info.append(f"OpenFF Molecule import failed: {e}")
            except Exception as e:
                debug_info.append(f"OpenFF Molecule import error: {e}")
            
            try:
                from openff.toolkit import Topology
                debug_info.append("OpenFF Topology import successful")
            except ImportError as e:
                debug_info.append(f"OpenFF Topology import failed: {e}")
            except Exception as e:
                debug_info.append(f"OpenFF Topology import error: {e}")
                
        except ImportError as e:
            debug_info.append(f"OpenFF base import failed: {e}")
        except Exception as e:
            debug_info.append(f"OpenFF base import error: {e}")
        
        # Check if required libraries are available
        if not OPENMM_AVAILABLE:
            return "Error: OpenMM is not installed or available\nDebug info:\n" + "\n".join(debug_info)
        
        if not OPENFF_AVAILABLE:
            return "Error: OpenFF toolkit is not installed or available\nDebug info:\n" + "\n".join(debug_info)
        
        # Load the force field
        force_field = _load_force_field()
        
        # TODO: In the next step, we'll add molecule processing and actual minimization
        success_msg = f"Success: OpenFF force field loaded with {len(force_field._parameter_handlers)} parameter handlers"
        return success_msg + "\nDebug info:\n" + "\n".join(debug_info)
        
    except Exception as e:
        return f"Error: {str(e)}"
