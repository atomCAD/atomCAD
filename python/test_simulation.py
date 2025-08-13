#!/usr/bin/env python3
"""
Unit tests for simulation.py energy minimization functionality.

Run these tests in the conda environment with:
    python test_simulation.py
or:
    python -m unittest test_simulation.py
"""

import unittest
import sys
from pathlib import Path

# Add the python directory to the path so we can import simulation
sys.path.insert(0, str(Path(__file__).parent))

from simulation import minimize_energy


class TestEnergyMinimization(unittest.TestCase):
    """Test cases for the energy minimization functionality."""
    
    def test_backward_compatibility_no_parameters(self):
        """Test that the function works without parameters (backward compatibility)."""
        result = minimize_energy()
        
        self.assertIsInstance(result, dict)
        self.assertTrue(result["success"])
        self.assertIn("parameter handlers", result["message"])
        self.assertEqual(result["positions"], [])
        self.assertEqual(result["energy"], 0.0)
        self.assertEqual(result["iterations"], 0)
    
    def test_methane_molecule(self):
        """Test energy minimization with a methane molecule (CH4)."""
        # Methane: 1 carbon + 4 hydrogens
        atoms = [
            {"atomic_number": 6, "position": [0.0, 0.0, 0.0]},      # Carbon
            {"atomic_number": 1, "position": [1.1, 0.0, 0.0]},      # H1
            {"atomic_number": 1, "position": [-0.37, 1.04, 0.0]},   # H2  
            {"atomic_number": 1, "position": [-0.37, -0.52, 0.9]},  # H3
            {"atomic_number": 1, "position": [-0.37, -0.52, -0.9]}, # H4
        ]
        
        bonds = [
            {"atom1": 0, "atom2": 1, "order": 1},  # C-H1
            {"atom1": 0, "atom2": 2, "order": 1},  # C-H2
            {"atom1": 0, "atom2": 3, "order": 1},  # C-H3
            {"atom1": 0, "atom2": 4, "order": 1},  # C-H4
        ]
        
        result = minimize_energy(atoms, bonds)
        
        # Validate result structure
        self.assertIsInstance(result, dict)
        self.assertIn("success", result)
        self.assertIn("positions", result)
        self.assertIn("energy", result)
        self.assertIn("iterations", result)
        self.assertIn("message", result)
        
        # Validate successful minimization
        self.assertTrue(result["success"], f"Minimization failed: {result['message']}")
        self.assertEqual(len(result["positions"]), 5)  # 5 atoms
        self.assertIsInstance(result["energy"], float)
        self.assertIsInstance(result["iterations"], int)
        
        # Check that positions are reasonable (each position has 3 coordinates)
        for pos in result["positions"]:
            self.assertEqual(len(pos), 3)
            self.assertIsInstance(pos[0], float)
            self.assertIsInstance(pos[1], float)
            self.assertIsInstance(pos[2], float)
        
        print(f"Methane minimization: Energy = {result['energy']:.2f} kJ/mol")
    
    def test_ethane_molecule(self):
        """Test energy minimization with an ethane molecule (C2H6)."""
        # Ethane: 2 carbons + 6 hydrogens
        atoms = [
            {"atomic_number": 6, "position": [0.0, 0.0, 0.0]},      # C1
            {"atomic_number": 6, "position": [1.5, 0.0, 0.0]},      # C2
            {"atomic_number": 1, "position": [-0.5, 1.0, 0.0]},     # H1 on C1
            {"atomic_number": 1, "position": [-0.5, -0.5, 0.9]},    # H2 on C1
            {"atomic_number": 1, "position": [-0.5, -0.5, -0.9]},   # H3 on C1
            {"atomic_number": 1, "position": [2.0, 1.0, 0.0]},      # H4 on C2
            {"atomic_number": 1, "position": [2.0, -0.5, 0.9]},     # H5 on C2
            {"atomic_number": 1, "position": [2.0, -0.5, -0.9]},    # H6 on C2
        ]
        
        bonds = [
            {"atom1": 0, "atom2": 1, "order": 1},  # C1-C2
            {"atom1": 0, "atom2": 2, "order": 1},  # C1-H1
            {"atom1": 0, "atom2": 3, "order": 1},  # C1-H2
            {"atom1": 0, "atom2": 4, "order": 1},  # C1-H3
            {"atom1": 1, "atom2": 5, "order": 1},  # C2-H4
            {"atom1": 1, "atom2": 6, "order": 1},  # C2-H5
            {"atom1": 1, "atom2": 7, "order": 1},  # C2-H6
        ]
        
        result = minimize_energy(atoms, bonds)
        
        # Validate successful minimization
        self.assertTrue(result["success"], f"Minimization failed: {result['message']}")
        self.assertEqual(len(result["positions"]), 8)  # 8 atoms
        self.assertIsInstance(result["energy"], float)
        
        # Check that positions are reasonable
        for pos in result["positions"]:
            self.assertEqual(len(pos), 3)
            for coord in pos:
                self.assertIsInstance(coord, float)
        
        print(f"Ethane minimization: Energy = {result['energy']:.2f} kJ/mol")
    
    def test_with_formal_charges(self):
        """Test that formal charges are handled correctly."""
        # Simple molecule with formal charge
        atoms = [
            {"atomic_number": 6, "position": [0.0, 0.0, 0.0], "formal_charge": 0},
            {"atomic_number": 1, "position": [1.0, 0.0, 0.0], "formal_charge": 0},
        ]
        
        bonds = [
            {"atom1": 0, "atom2": 1, "order": 1},
        ]
        
        result = minimize_energy(atoms, bonds)
        self.assertTrue(result["success"], f"Minimization failed: {result['message']}")
    
    def test_custom_options(self):
        """Test that custom options are respected."""
        atoms = [
            {"atomic_number": 6, "position": [0.0, 0.0, 0.0]},
            {"atomic_number": 1, "position": [1.0, 0.0, 0.0]},
        ]
        
        bonds = [
            {"atom1": 0, "atom2": 1, "order": 1},
        ]
        
        options = {
            "max_iterations": 500,
            "tolerance": 1e-5
        }
        
        result = minimize_energy(atoms, bonds, options)
        self.assertTrue(result["success"], f"Minimization failed: {result['message']}")
        # Note: OpenMM doesn't report actual iterations, so we can't test that
    
    def test_error_handling_empty_molecule(self):
        """Test error handling with empty molecule."""
        result = minimize_energy([], [])
        
        self.assertIsInstance(result, dict)
        self.assertFalse(result["success"])
        self.assertEqual(result["positions"], [])
        self.assertEqual(result["energy"], 0.0)
        self.assertEqual(result["iterations"], 0)
        self.assertIn("Error:", result["message"])
    
    def test_error_handling_invalid_bond(self):
        """Test error handling with invalid bond indices."""
        atoms = [
            {"atomic_number": 6, "position": [0.0, 0.0, 0.0]},
        ]
        
        bonds = [
            {"atom1": 0, "atom2": 5, "order": 1},  # Invalid atom index 5
        ]
        
        result = minimize_energy(atoms, bonds)
        
        self.assertIsInstance(result, dict)
        self.assertFalse(result["success"])
        self.assertEqual(result["positions"], [])
        self.assertEqual(result["energy"], 0.0)
        self.assertEqual(result["iterations"], 0)
        self.assertIn("Error:", result["message"])


def run_tests():
    """Run all tests and provide a summary."""
    print("=" * 60)
    print("Running Energy Minimization Tests")
    print("=" * 60)
    
    # Create test suite
    suite = unittest.TestLoader().loadTestsFromTestCase(TestEnergyMinimization)
    
    # Run tests with verbose output
    runner = unittest.TextTestRunner(verbosity=2)
    result = runner.run(suite)
    
    print("\n" + "=" * 60)
    if result.wasSuccessful():
        print("✅ ALL TESTS PASSED!")
    else:
        print("❌ SOME TESTS FAILED!")
        print(f"Failures: {len(result.failures)}")
        print(f"Errors: {len(result.errors)}")
    print("=" * 60)
    
    return result.wasSuccessful()


if __name__ == "__main__":
    success = run_tests()
    sys.exit(0 if success else 1)
