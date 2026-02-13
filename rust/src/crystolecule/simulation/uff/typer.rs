// UFF atom type assignment from atomic connectivity.
//
// Maps (atomic_number, bond_list) → UFF atom type label (e.g., "C_3", "N_R").
// Simplified port of RDKit's AtomTyper.cpp, using atomCAD's explicit bond orders
// instead of SMARTS perception.
