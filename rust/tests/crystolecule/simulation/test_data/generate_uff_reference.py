#!/usr/bin/env python3
"""
Generate UFF reference data for atomCAD energy minimization tests.

Uses RDKit to produce ground-truth energies, gradients, and minimized
geometries for a set of test molecules. Output is a JSON file that
serves as the reference for validating the Rust UFF implementation.

Prerequisites:
    pip install rdkit numpy

Usage:
    python generate_uff_reference.py

Output:
    uff_reference.json (in the same directory as this script)
"""

import json
import math
import os
import sys
import traceback

from rdkit import Chem, rdBase
from rdkit.Chem import AllChem, rdForceFieldHelpers, rdMolTransforms

RANDOM_SEED = 42

MOLECULES = [
    {
        "name": "methane",
        "smiles": "C",
        "notes": "Tetrahedral sp3, simplest 3D molecule",
    },
    {
        "name": "ethylene",
        "smiles": "C=C",
        "notes": "sp2 planar, double bond",
    },
    {
        "name": "ethane",
        "smiles": "CC",
        "notes": "sp3-sp3 torsion",
    },
    {
        "name": "benzene",
        "smiles": "c1ccccc1",
        "notes": "Aromatic, inversion terms",
    },
    {
        "name": "butane",
        "smiles": "CCCC",
        "notes": "Gauche/anti torsion conformations",
    },
    {
        "name": "water",
        "smiles": "O",
        "notes": "Non-carbon, bent geometry",
    },
    {
        "name": "ammonia",
        "smiles": "N",
        "notes": "Nitrogen sp3, inversion",
    },
    {
        "name": "adamantane",
        "smiles": "C1C2CC3CC1CC(C2)C3",
        "notes": "Diamond fragment (adamantane C10H16), directly relevant to APM",
    },
    {
        "name": "methanethiol",
        "smiles": "CS",
        "notes": "Sulfur atom type, group 6 special torsion handling",
    },
]


def infer_uff_atom_type(atom):
    """Infer UFF atom type label from RDKit atom properties.

    This is a simplified reimplementation of RDKit's AtomTyper.cpp getAtomLabel().
    It covers the elements in our test molecules (C, H, N, O, S).
    The definitive validation of atom typing is against the per-interaction
    parameters (which RDKit computes using its internal typer).
    """
    elem = atom.GetSymbol()
    hyb = atom.GetHybridization()
    is_aromatic = atom.GetIsAromatic()
    degree = atom.GetDegree()
    HybType = Chem.rdchem.HybridizationType

    if elem == "H":
        # H bonded to electronegative atom could be H_b, but UFF uses H_ for all
        return "H_"
    elif elem == "C":
        if is_aromatic:
            return "C_R"
        elif hyb == HybType.SP3:
            return "C_3"
        elif hyb == HybType.SP2:
            return "C_2"
        elif hyb == HybType.SP:
            return "C_1"
        else:
            return "C_3"
    elif elem == "N":
        if is_aromatic:
            return "N_R"
        elif hyb == HybType.SP3:
            return "N_3"
        elif hyb == HybType.SP2:
            return "N_2"
        elif hyb == HybType.SP:
            return "N_1"
        else:
            return "N_3"
    elif elem == "O":
        if is_aromatic:
            return "O_R"
        elif hyb == HybType.SP3:
            return "O_3"
        elif hyb == HybType.SP2:
            return "O_2"
        else:
            return "O_3"
    elif elem == "S":
        # Simplified: S_3+2 for most cases (2-coordinate sulfur)
        return "S_3+2"
    elif elem == "P":
        return "P_3+3"
    elif elem == "F":
        return "F_"
    elif elem == "Cl":
        return "Cl"
    elif elem == "Br":
        return "Br"
    elif elem == "I":
        return "I_"
    elif elem == "Si":
        return "Si3"
    else:
        return f"{elem}_?"


def get_atom_info(mol):
    """Get atom information including UFF type labels."""
    atoms = []
    for i in range(mol.GetNumAtoms()):
        atom = mol.GetAtomWithIdx(i)
        uff_type = infer_uff_atom_type(atom)
        atoms.append(
            {
                "index": i,
                "atomic_number": atom.GetAtomicNum(),
                "symbol": atom.GetSymbol(),
                "uff_type": uff_type,
            }
        )
    return atoms


def get_positions(mol, conf_id=-1):
    """Get atom positions as list of [x, y, z]."""
    conf = mol.GetConformer(conf_id)
    positions = []
    for i in range(mol.GetNumAtoms()):
        pos = conf.GetAtomPosition(i)
        positions.append([round(pos.x, 10), round(pos.y, 10), round(pos.z, 10)])
    return positions


def get_bond_info(mol):
    """Get bond connectivity."""
    bonds = []
    for bond in mol.GetBonds():
        bonds.append(
            {
                "atom1": bond.GetBeginAtomIdx(),
                "atom2": bond.GetEndAtomIdx(),
                "order": bond.GetBondTypeAsDouble(),
            }
        )
    return bonds


def enumerate_angles(mol):
    """Enumerate all angle triplets (neighbor_a, center, neighbor_b)."""
    angles = []
    for atom in mol.GetAtoms():
        center = atom.GetIdx()
        neighbors = sorted([n.GetIdx() for n in atom.GetNeighbors()])
        for a in range(len(neighbors)):
            for b in range(a + 1, len(neighbors)):
                angles.append([neighbors[a], center, neighbors[b]])
    return angles


def enumerate_torsions(mol):
    """Enumerate all proper torsion quadruplets (matching RDKit's enumeration)."""
    torsions = []
    for bond in mol.GetBonds():
        a2 = bond.GetBeginAtomIdx()
        a3 = bond.GetEndAtomIdx()
        for n1 in mol.GetAtomWithIdx(a2).GetNeighbors():
            a1 = n1.GetIdx()
            if a1 == a3:
                continue
            for n4 in mol.GetAtomWithIdx(a3).GetNeighbors():
                a4 = n4.GetIdx()
                if a4 == a2 or a4 == a1:
                    continue
                torsions.append([a1, a2, a3, a4])
    return torsions


def get_interaction_params(mol):
    """Get per-interaction UFF parameters."""
    result = {
        "bond_params": [],
        "angle_params": [],
        "torsion_params": [],
        "inversion_params": [],
        "vdw_params": [],
    }

    # Bond parameters
    for bond in mol.GetBonds():
        i, j = bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()
        try:
            p = rdForceFieldHelpers.GetUFFBondStretchParams(mol, i, j)
            if p and len(p) >= 2:
                result["bond_params"].append(
                    {"atoms": [i, j], "kb": round(p[0], 6), "r0": round(p[1], 6)}
                )
        except Exception as e:
            print(f"  Warning: could not get bond params for ({i},{j}): {e}")

    # Angle parameters (RDKit Python API returns theta0 in degrees)
    angles = enumerate_angles(mol)
    for i, j, k in angles:
        try:
            p = rdForceFieldHelpers.GetUFFAngleBendParams(mol, i, j, k)
            if p and len(p) >= 2:
                result["angle_params"].append(
                    {
                        "atoms": [i, j, k],
                        "ka": round(p[0], 6),
                        "theta0_deg": round(p[1], 6),
                        "theta0_rad": round(math.radians(p[1]), 10),
                    }
                )
        except Exception as e:
            print(f"  Warning: could not get angle params for ({i},{j},{k}): {e}")

    # Torsion parameters (GetUFFTorsionParams returns a single float V, not a tuple)
    torsions = enumerate_torsions(mol)
    for a1, a2, a3, a4 in torsions:
        try:
            p = rdForceFieldHelpers.GetUFFTorsionParams(mol, a1, a2, a3, a4)
            if p is not None and p != 0:
                # p is a single float (the torsion barrier V)
                V = float(p)
                result["torsion_params"].append(
                    {"atoms": [a1, a2, a3, a4], "V": round(V, 6)}
                )
        except Exception as e:
            print(
                f"  Warning: could not get torsion params for ({a1},{a2},{a3},{a4}): {e}"
            )

    # Inversion parameters - enumerate all 3 permutations per sp2 center (as RDKit does)
    # NOTE: GetUFFInversionParams returns None in RDKit 2025.03.5 Python bindings
    # even for valid sp2 centers. We infer K from the UFF paper rules instead:
    #   C sp2: K=6.0, N sp2: K=2.0, group 15 (P,As,Sb,Bi) sp3: K=22.0/84.0
    # Inversion params will be validated against RDKit's C++ test values directly.
    for atom in mol.GetAtoms():
        if atom.GetDegree() == 3:
            center = atom.GetIdx()
            nbrs = [n.GetIdx() for n in atom.GetNeighbors()]

            # Try RDKit API first
            K = None
            try:
                p = rdForceFieldHelpers.GetUFFInversionParams(
                    mol, center, nbrs[0], nbrs[1], nbrs[2]
                )
                if p is not None:
                    K = float(p) if not hasattr(p, '__len__') else float(p[0])
            except Exception:
                pass

            # If API returned None, infer from UFF paper rules
            if K is None:
                elem = atom.GetSymbol()
                hyb = atom.GetHybridization()
                HybType = Chem.rdchem.HybridizationType
                if elem == "C" and (hyb == HybType.SP2 or atom.GetIsAromatic()):
                    K = 6.0
                elif elem == "N" and (hyb == HybType.SP2 or atom.GetIsAromatic()):
                    K = 2.0
                elif elem in ("P", "As", "Sb", "Bi"):
                    # Group 15: K depends on element
                    K = 84.0 if elem == "P" else 22.0

            if K is not None and abs(K) > 1e-10:
                # RDKit adds 3 inversion terms per center
                permutations = [
                    (nbrs[0], nbrs[1], nbrs[2]),
                    (nbrs[1], nbrs[0], nbrs[2]),
                    (nbrs[2], nbrs[0], nbrs[1]),
                ]
                for i_atom, j_atom, k_atom in permutations:
                    result["inversion_params"].append(
                        {
                            "center": center,
                            "atom_i": i_atom,
                            "atom_j": j_atom,
                            "atom_k": k_atom,
                            "K": round(K, 6),
                            "source": "inferred" if p is None else "rdkit_api",
                        }
                    )

    # VdW parameters (1,4+ pairs)
    num_atoms = mol.GetNumAtoms()
    for i in range(num_atoms):
        for j in range(i + 1, num_atoms):
            path_len = len(Chem.GetShortestPath(mol, i, j)) - 1
            if path_len >= 4:
                try:
                    p = rdForceFieldHelpers.GetUFFVdWParams(mol, i, j)
                    if p and len(p) >= 2:
                        result["vdw_params"].append(
                            {
                                "atoms": [i, j],
                                "x_ij": round(p[0], 6),
                                "D_ij": round(p[1], 6),
                            }
                        )
                except Exception:
                    pass

    return result


def compute_gradients(ff, num_atoms):
    """Compute analytical gradients (dE/dx, i.e. negative of force)."""
    grad = ff.CalcGrad()
    gradients = []
    for i in range(num_atoms):
        gradients.append(
            [
                round(grad[3 * i], 10),
                round(grad[3 * i + 1], 10),
                round(grad[3 * i + 2], 10),
            ]
        )
    return gradients


def verify_gradients_numerically(mol, step=1e-5, tolerance=0.01):
    """Verify analytical gradients against numerical (central difference).

    Returns (passed, max_relative_error, details).
    This is a self-check on the reference data.

    Note: We rebuild the force field for each displaced position because
    RDKit's ForceField.Initialize() copies positions into internal storage.
    CalcEnergy/CalcGrad use the internal copy, so modifying the conformer
    alone doesn't affect the force field's calculations.
    """
    num_atoms = mol.GetNumAtoms()

    # Get analytical gradients at current positions
    ff = AllChem.UFFGetMoleculeForceField(mol)
    ff.Initialize()
    analytical_grad = ff.CalcGrad()

    conf = mol.GetConformer()
    max_rel_err = 0.0
    details = []

    for atom_idx in range(num_atoms):
        orig_pos = conf.GetAtomPosition(atom_idx)
        orig = [orig_pos.x, orig_pos.y, orig_pos.z]

        for coord_idx, coord_name in enumerate(["x", "y", "z"]):
            flat_idx = atom_idx * 3 + coord_idx

            # Forward step
            fwd = list(orig)
            fwd[coord_idx] += step
            conf.SetAtomPosition(atom_idx, tuple(fwd))
            ff_plus = AllChem.UFFGetMoleculeForceField(mol)
            ff_plus.Initialize()
            e_plus = ff_plus.CalcEnergy()

            # Backward step
            bwd = list(orig)
            bwd[coord_idx] -= step
            conf.SetAtomPosition(atom_idx, tuple(bwd))
            ff_minus = AllChem.UFFGetMoleculeForceField(mol)
            ff_minus.Initialize()
            e_minus = ff_minus.CalcEnergy()

            # Restore original
            conf.SetAtomPosition(atom_idx, tuple(orig))

            numerical = (e_plus - e_minus) / (2 * step)
            analytical = analytical_grad[flat_idx]

            if abs(analytical) > 1e-8:
                rel_err = abs(numerical - analytical) / abs(analytical)
            elif abs(numerical) > 1e-8:
                rel_err = abs(numerical - analytical) / abs(numerical)
            else:
                rel_err = 0.0

            max_rel_err = max(max_rel_err, rel_err)

            if rel_err > tolerance:
                details.append(
                    {
                        "atom": atom_idx,
                        "coord": coord_name,
                        "analytical": analytical,
                        "numerical": numerical,
                        "rel_error": rel_err,
                    }
                )

    passed = max_rel_err < tolerance
    return passed, max_rel_err, details


def compute_geometric_measurements(mol, conf_id=-1):
    """Compute geometric properties (bond lengths, angles, dihedrals)."""
    conf = mol.GetConformer(conf_id)
    measurements = {"bond_lengths": [], "angles": [], "dihedrals": []}

    # Bond lengths
    for bond in mol.GetBonds():
        i, j = bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()
        length = rdMolTransforms.GetBondLength(conf, i, j)
        measurements["bond_lengths"].append(
            {"atoms": [i, j], "length": round(length, 8)}
        )

    # Angles
    for atom in mol.GetAtoms():
        center = atom.GetIdx()
        neighbors = sorted([n.GetIdx() for n in atom.GetNeighbors()])
        for a in range(len(neighbors)):
            for b in range(a + 1, len(neighbors)):
                try:
                    angle = rdMolTransforms.GetAngleDeg(
                        conf, neighbors[a], center, neighbors[b]
                    )
                    measurements["angles"].append(
                        {
                            "atoms": [neighbors[a], center, neighbors[b]],
                            "angle_deg": round(angle, 6),
                        }
                    )
                except Exception:
                    pass

    # Dihedrals (one representative per bond)
    for bond in mol.GetBonds():
        a2, a3 = bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()
        n1_list = sorted(
            [
                n.GetIdx()
                for n in mol.GetAtomWithIdx(a2).GetNeighbors()
                if n.GetIdx() != a3
            ]
        )
        n4_list = sorted(
            [
                n.GetIdx()
                for n in mol.GetAtomWithIdx(a3).GetNeighbors()
                if n.GetIdx() != a2
            ]
        )
        if n1_list and n4_list:
            a1, a4 = n1_list[0], n4_list[0]
            try:
                dihedral = rdMolTransforms.GetDihedralDeg(conf, a1, a2, a3, a4)
                measurements["dihedrals"].append(
                    {
                        "atoms": [a1, a2, a3, a4],
                        "dihedral_deg": round(dihedral, 6),
                    }
                )
            except Exception:
                pass

    return measurements


def process_molecule(mol_info):
    """Generate complete reference data for a molecule."""
    name = mol_info["name"]
    smiles = mol_info["smiles"]
    notes = mol_info["notes"]

    print(f"Processing {name} ({smiles})...")

    # Create molecule with explicit hydrogens
    mol = Chem.MolFromSmiles(smiles)
    if mol is None:
        raise RuntimeError(f"Failed to parse SMILES: {smiles}")
    mol = Chem.AddHs(mol)

    # Check UFF has parameters for all atoms
    if not rdForceFieldHelpers.UFFHasAllMoleculeParams(mol):
        raise RuntimeError(f"UFF missing parameters for some atoms in {name}")

    # Generate 3D coordinates with fixed seed
    embed_params = AllChem.ETKDGv3()
    embed_params.randomSeed = RANDOM_SEED
    if AllChem.EmbedMolecule(mol, embed_params) != 0:
        # Fallback to simpler embedding
        if AllChem.EmbedMolecule(mol, randomSeed=RANDOM_SEED) != 0:
            raise RuntimeError(f"Failed to embed {name}")

    # Atom and bond info
    atoms = get_atom_info(mol)
    bonds = get_bond_info(mol)
    input_positions = get_positions(mol)

    # Interaction parameters
    params_data = get_interaction_params(mol)

    interaction_counts = {
        "bonds": len(params_data["bond_params"]),
        "angles": len(params_data["angle_params"]),
        "torsions": len(params_data["torsion_params"]),
        "inversions": len(params_data["inversion_params"]),
        "vdw_pairs": len(params_data["vdw_params"]),
    }

    # --- Energy and gradients at input positions ---

    # Full UFF (with vdW)
    ff_full = AllChem.UFFGetMoleculeForceField(mol)
    if ff_full is None:
        raise RuntimeError(f"Failed to build UFF force field for {name}")
    ff_full.Initialize()
    input_energy_full = ff_full.CalcEnergy()
    input_gradients_full = compute_gradients(ff_full, mol.GetNumAtoms())

    # Bonded-only UFF (vdwThresh=0 excludes all vdW pairs)
    ff_bonded = AllChem.UFFGetMoleculeForceField(mol, vdwThresh=0.0)
    if ff_bonded is not None:
        ff_bonded.Initialize()
        input_energy_bonded = ff_bonded.CalcEnergy()
        input_gradients_bonded = compute_gradients(ff_bonded, mol.GetNumAtoms())
    else:
        print(f"  Warning: could not build bonded-only FF, using full FF")
        input_energy_bonded = input_energy_full
        input_gradients_bonded = input_gradients_full

    input_energy_vdw = round(input_energy_full - input_energy_bonded, 10)

    # Verify gradients numerically (self-check on reference data)
    grad_ok, grad_max_err, grad_failures = verify_gradients_numerically(mol)
    if not grad_ok:
        print(
            f"  WARNING: Gradient verification failed! Max relative error: {grad_max_err:.4e}"
        )
        for f in grad_failures[:3]:
            print(
                f"    Atom {f['atom']}.{f['coord']}: analytical={f['analytical']:.6f}, "
                f"numerical={f['numerical']:.6f}, rel_err={f['rel_error']:.4e}"
            )
    else:
        print(f"  Gradient self-check passed (max rel error: {grad_max_err:.2e})")

    # Input geometry measurements
    input_geometry = compute_geometric_measurements(mol)

    # --- Minimization (full UFF) ---
    # Re-build force field for minimization (fresh state)
    ff_min = AllChem.UFFGetMoleculeForceField(mol)
    ff_min.Initialize()
    converged = ff_min.Minimize(maxIts=2000)

    minimized_positions = get_positions(mol)
    minimized_energy_full = ff_min.CalcEnergy()

    # Bonded-only energy at minimized positions
    ff_min_bonded = AllChem.UFFGetMoleculeForceField(mol, vdwThresh=0.0)
    if ff_min_bonded is not None:
        ff_min_bonded.Initialize()
        minimized_energy_bonded = ff_min_bonded.CalcEnergy()
    else:
        minimized_energy_bonded = minimized_energy_full

    minimized_energy_vdw = round(minimized_energy_full - minimized_energy_bonded, 10)

    minimized_geometry = compute_geometric_measurements(mol)

    result = {
        "name": name,
        "smiles": smiles,
        "notes": notes,
        "num_atoms": mol.GetNumAtoms(),
        "num_bonds": mol.GetNumBonds(),
        "atoms": atoms,
        "bonds": bonds,
        "input_positions": input_positions,
        "interaction_counts": interaction_counts,
        "bond_params": params_data["bond_params"],
        "angle_params": params_data["angle_params"],
        "torsion_params": params_data["torsion_params"],
        "inversion_params": params_data["inversion_params"],
        "vdw_params": params_data["vdw_params"],
        "input_energy": {
            "total": round(input_energy_full, 10),
            "bonded": round(input_energy_bonded, 10),
            "vdw": input_energy_vdw,
        },
        "input_gradients": {
            "full": input_gradients_full,
            "bonded": input_gradients_bonded,
        },
        "gradient_verification": {
            "passed": grad_ok,
            "max_relative_error": round(grad_max_err, 8),
            "step_size": 1e-5,
            "tolerance": 0.01,
        },
        "input_geometry": input_geometry,
        "minimization_converged": converged == 0,
        "minimized_positions": minimized_positions,
        "minimized_energy": {
            "total": round(minimized_energy_full, 10),
            "bonded": round(minimized_energy_bonded, 10),
            "vdw": minimized_energy_vdw,
        },
        "minimized_geometry": minimized_geometry,
    }

    print(f"  Atoms: {mol.GetNumAtoms()}, Bonds: {mol.GetNumBonds()}")
    print(f"  Interactions: {interaction_counts}")
    print(
        f"  E_input:  total={input_energy_full:.6f}  "
        f"bonded={input_energy_bonded:.6f}  vdW={input_energy_vdw:.6f}"
    )
    print(
        f"  E_min:    total={minimized_energy_full:.6f}  "
        f"bonded={minimized_energy_bonded:.6f}  vdW={minimized_energy_vdw:.6f}"
    )
    print(f"  Converged: {converged == 0}")

    return result


def generate_butane_scan():
    """Generate 72-point butane dihedral energy scan (relaxed).

    For each target dihedral angle (0-355 in 5 degree steps):
    1. Set the C-C-C-C dihedral to the target angle
    2. Add a strong torsion constraint
    3. Minimize (everything except the constrained dihedral relaxes)
    4. Record the energy
    """
    print("\nGenerating butane dihedral scan...")

    mol = Chem.MolFromSmiles("CCCC")
    mol = Chem.AddHs(mol)

    embed_params = AllChem.ETKDGv3()
    embed_params.randomSeed = RANDOM_SEED
    if AllChem.EmbedMolecule(mol, embed_params) != 0:
        AllChem.EmbedMolecule(mol, randomSeed=RANDOM_SEED)

    # First optimize to get a good starting geometry
    ff = AllChem.UFFGetMoleculeForceField(mol)
    ff.Initialize()
    ff.Minimize(maxIts=2000)

    # Find the four carbon atoms (heavy atoms come first after AddHs)
    carbons = [a.GetIdx() for a in mol.GetAtoms() if a.GetAtomicNum() == 6]
    if len(carbons) != 4:
        raise RuntimeError(f"Expected 4 carbons in butane, got {len(carbons)}")
    carbons.sort()
    c0, c1, c2, c3 = carbons

    # Save optimized conformer as template
    template_mol = Chem.RWMol(mol)

    scan_points = []
    for angle_deg in range(0, 360, 5):
        # Copy template molecule
        mol_copy = Chem.RWMol(template_mol)
        conf = mol_copy.GetConformer()

        # Set the C-C-C-C dihedral
        rdMolTransforms.SetDihedralDeg(conf, c0, c1, c2, c3, float(angle_deg))

        # Build force field with dihedral constraint
        ff = AllChem.UFFGetMoleculeForceField(mol_copy)
        if ff is None:
            print(f"  Warning: could not build FF at angle {angle_deg}")
            continue
        ff.Initialize()

        # Constrain dihedral with strong force constant (absolute angle, not relative)
        ff.UFFAddTorsionConstraint(
            c0,
            c1,
            c2,
            c3,
            False,  # relative=False (use absolute angle)
            float(angle_deg) - 0.1,
            float(angle_deg) + 0.1,
            1e6,
        )
        ff.Minimize(maxIts=500)

        energy = ff.CalcEnergy()
        actual_dihedral = rdMolTransforms.GetDihedralDeg(
            mol_copy.GetConformer(), c0, c1, c2, c3
        )

        scan_points.append(
            {
                "target_angle_deg": angle_deg,
                "actual_angle_deg": round(actual_dihedral, 4),
                "energy": round(energy, 8),
            }
        )

    # Compute relative energies (relative to global minimum)
    min_energy = min(p["energy"] for p in scan_points)
    for p in scan_points:
        p["relative_energy"] = round(p["energy"] - min_energy, 8)

    # Identify key conformations
    anti_energy = None
    gauche_plus_energy = None
    eclipsed_energy = None
    syn_energy = None

    for p in scan_points:
        angle = p["target_angle_deg"]
        if angle == 180:
            anti_energy = p["relative_energy"]
        elif angle == 60:
            gauche_plus_energy = p["relative_energy"]
        elif angle == 120:
            eclipsed_energy = p["relative_energy"]
        elif angle == 0:
            syn_energy = p["relative_energy"]

    print(f"  Generated {len(scan_points)} scan points")
    print(f"  Energy range: {min_energy:.4f} to {max(p['energy'] for p in scan_points):.4f}")
    if anti_energy is not None:
        print(f"  Anti (180): {anti_energy:.4f} kcal/mol (relative)")
    if gauche_plus_energy is not None:
        print(f"  Gauche+ (60): {gauche_plus_energy:.4f} kcal/mol (relative)")
    if eclipsed_energy is not None:
        print(f"  Eclipsed (120): {eclipsed_energy:.4f} kcal/mol (relative)")
    if syn_energy is not None:
        print(f"  Syn (0): {syn_energy:.4f} kcal/mol (relative)")

    return {
        "carbon_indices": [c0, c1, c2, c3],
        "num_points": len(scan_points),
        "scan_points": scan_points,
        "min_energy": round(min_energy, 8),
        "key_conformations": {
            "anti_180": anti_energy,
            "gauche_60": gauche_plus_energy,
            "eclipsed_120": eclipsed_energy,
            "syn_0": syn_energy,
        },
        "notes": (
            "72-point relaxed dihedral scan of butane C-C-C-C torsion. "
            "At each angle, the dihedral is constrained and all other "
            "coordinates are optimized. Energies in kcal/mol."
        ),
    }


def main():
    print(f"Generating UFF reference data using RDKit {rdBase.rdkitVersion}")
    print(f"Random seed: {RANDOM_SEED}")
    print("=" * 70)

    reference_data = {
        "generator": "RDKit",
        "rdkit_version": rdBase.rdkitVersion,
        "random_seed": RANDOM_SEED,
        "energy_units": "kcal/mol",
        "distance_units": "angstrom",
        "angle_units": "theta0_rad is radians, theta0_deg and all geometry angles are degrees",
        "gradient_convention": "dE/dx (negative of force). Gradient units: kcal/(mol*angstrom)",
        "description": (
            "Ground-truth UFF reference data for validating atomCAD's "
            "Rust UFF implementation. Generated by generate_uff_reference.py "
            "using RDKit's UFF force field. The 'bonded' energy/gradients exclude "
            "van der Waals terms (built with vdwThresh=0); 'total' includes vdW."
        ),
        "molecules": [],
        "butane_dihedral_scan": None,
    }

    # Process each molecule
    for mol_info in MOLECULES:
        try:
            mol_data = process_molecule(mol_info)
            reference_data["molecules"].append(mol_data)
        except Exception as e:
            print(f"  ERROR processing {mol_info['name']}: {e}")
            traceback.print_exc()
            sys.exit(1)
        print()

    # Butane dihedral scan
    try:
        reference_data["butane_dihedral_scan"] = generate_butane_scan()
    except Exception as e:
        print(f"  ERROR generating butane scan: {e}")
        traceback.print_exc()

    # Write JSON
    script_dir = os.path.dirname(os.path.abspath(__file__))
    output_path = os.path.join(script_dir, "uff_reference.json")
    with open(output_path, "w", newline="\n") as f:
        json.dump(reference_data, f, indent=2)
        f.write("\n")

    print("\n" + "=" * 70)
    print(f"Reference data written to: {output_path}")
    print(f"Total molecules: {len(reference_data['molecules'])}")
    total_atoms = sum(m["num_atoms"] for m in reference_data["molecules"])
    print(f"Total atoms across all molecules: {total_atoms}")

    # Summary table
    print("\nSummary:")
    print(f"  {'Molecule':<15} {'Atoms':>5} {'Bonds':>5} {'E_input':>12} {'E_min':>12} {'Converged':>10}")
    print(f"  {'-'*15} {'-'*5} {'-'*5} {'-'*12} {'-'*12} {'-'*10}")
    for m in reference_data["molecules"]:
        print(
            f"  {m['name']:<15} {m['num_atoms']:>5} {m['num_bonds']:>5} "
            f"{m['input_energy']['total']:>12.4f} {m['minimized_energy']['total']:>12.4f} "
            f"{'yes' if m['minimization_converged'] else 'NO':>10}"
        )


if __name__ == "__main__":
    main()
