// Molecular topology: enumerates bonded interactions from an AtomicStructure.
//
// Given the bond graph, this module builds lists of:
// - Bonds (1-2 interactions)
// - Angles (1-3 interactions)
// - Torsions (1-4 interactions)
// - Inversions (out-of-plane at sp2 centers)
//
// These interaction lists are consumed by force field implementations
// to compute energies and gradients.
