// L-BFGS optimizer wrapper for molecular geometry optimization.
//
// Wraps the `lbfgs` crate to minimize a force field's energy with respect
// to atomic positions. Supports frozen atoms (gradient zeroed for fixed atoms).
