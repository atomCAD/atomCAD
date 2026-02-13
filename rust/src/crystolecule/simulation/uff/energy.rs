// UFF energy terms and analytical gradients.
//
// Each energy term (bond stretch, angle bend, torsion, inversion) is implemented
// as a function that computes both the energy contribution and its gradient
// with respect to atomic positions.
//
// Ported from RDKit's BondStretch.cpp, AngleBend.cpp, TorsionAngle.cpp,
// and Inversion.cpp, cross-referenced with OpenBabel's forcefielduff.cpp.
