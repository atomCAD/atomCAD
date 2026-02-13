pub mod energy;
pub mod params;
pub mod typer;

// UFF (Universal Force Field) implementation.
//
// Implements the force field described in:
// Rappé et al., "UFF, a Full Periodic Table Force Field for Molecular Mechanics
// and Molecular Dynamics Simulations", JACS 1992, 114, 10024-10035.
//
// Ported from RDKit's modular UFF implementation, cross-referenced with OpenBabel.
