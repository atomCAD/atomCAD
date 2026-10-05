//! Mean single-bond enthalpies for twelve elements, and Pauling's estimate for
//! the pairs the table lacks.
//!
//! Nothing in the crate uses these today. They were the bond-energy term of the
//! chemisorption search's score (`ΔE_bond = Σ D(broken) − Σ D(formed)`), which
//! was dropped: the search ranks by UFF energy alone because its users work
//! under kinetic control. The tables are kept for a later consumer. The values
//! are crude by nature: a surface dimer bond and a bulk bond are both "Si–Si".

/// kJ/mol → kcal/mol, applied once when a table value is read.
pub const KJ_PER_KCAL: f64 = 4.184;

/// Pauling's factor in kJ/mol per unit electronegativity difference squared
/// (the classic 23 kcal/mol).
pub const PAULING_FACTOR_KJ: f64 = 96.5;

/// The elements the tables cover, in the order of their rows.
const ELEMENTS: [i16; 12] = [1, 6, 7, 8, 9, 14, 15, 16, 17, 32, 35, 53];

/// Pauling electronegativities, same order as [`ELEMENTS`]. Source: Wikipedia,
/// *Electronegativities of the elements (data page)*, Pauling scale.
const ELECTRONEGATIVITY: [f64; 12] = [
    2.20, 2.55, 3.04, 3.44, 3.98, 1.90, 2.19, 2.58, 3.16, 2.01, 2.96, 2.66,
];

/// Mean single-bond enthalpies (kJ/mol), lower triangle over [`ELEMENTS`];
/// `0.0` = not tabulated, estimated by Pauling's rule.
///
/// Copied verbatim from Appendix A of the chemisorption search design
/// (the Cengage general-chemistry bond enthalpy table). Two entries are not
/// from that source: C–Si 318 (the Huheey value; Wisconsin gives 301) and
/// Ge–Ge 186, derived from germanium's cohesive energy (3.85 eV/atom ÷ 2 bonds
/// per atom in the diamond lattice; the same derivation gives 223 for silicon
/// against the table's 222). Do not fill gaps from memory.
#[rustfmt::skip]
const ENTHALPY_KJ: [[f64; 12]; 12] = [
    //  H      C      N      O      F      Si     P      S      Cl     Ge     Br     I
    [ 436.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0], // H
    [ 413.0, 346.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0], // C
    [ 391.0, 305.0, 163.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0], // N
    [ 463.0, 358.0, 201.0, 146.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0], // O
    [ 565.0, 485.0, 283.0, 184.0, 155.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0], // F
    [ 318.0, 318.0,   0.0, 452.0, 565.0, 222.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0], // Si
    [ 322.0,   0.0,   0.0, 335.0, 490.0,   0.0, 201.0,   0.0,   0.0,   0.0,   0.0,   0.0], // P
    [ 347.0, 272.0,   0.0,   0.0, 284.0, 293.0,   0.0, 226.0,   0.0,   0.0,   0.0,   0.0], // S
    [ 432.0, 339.0, 192.0, 218.0, 253.0, 381.0, 326.0, 255.0, 242.0,   0.0,   0.0,   0.0], // Cl
    [   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0, 186.0,   0.0,   0.0], // Ge
    [ 366.0, 285.0, 243.0, 201.0, 249.0, 310.0,   0.0, 213.0, 216.0,   0.0, 193.0,   0.0], // Br
    [ 299.0, 213.0,   0.0, 201.0, 278.0, 234.0, 184.0,   0.0, 208.0,   0.0, 175.0, 151.0], // I
];

fn index_of(z: i16) -> Option<usize> {
    ELEMENTS.iter().position(|&e| e == z)
}

/// Whether `z` is one of the twelve elements the tables cover.
pub fn is_tabulated_element(z: i16) -> bool {
    index_of(z).is_some()
}

/// The tabulated mean single-bond enthalpy (kJ/mol), `None` for a gap in the
/// table (or an element outside it).
pub fn tabulated_enthalpy_kj(a: i16, b: i16) -> Option<f64> {
    let (i, j) = (index_of(a)?, index_of(b)?);
    let v = ENTHALPY_KJ[i.max(j)][i.min(j)];
    (v > 0.0).then_some(v)
}

/// Pauling's estimate `½[D(A–A) + D(B–B)] + 96.5·(χA − χB)²` (kJ/mol), from
/// the homonuclear enthalpies and electronegativities. Every element of the
/// table has both, so this is `None` only outside it.
pub fn pauling_estimate_kj(a: i16, b: i16) -> Option<f64> {
    let (i, j) = (index_of(a)?, index_of(b)?);
    let dchi = ELECTRONEGATIVITY[i] - ELECTRONEGATIVITY[j];
    Some(0.5 * (ENTHALPY_KJ[i][i] + ENTHALPY_KJ[j][j]) + PAULING_FACTOR_KJ * dchi * dchi)
}

/// One single bond's enthalpy in kcal/mol, and whether it is a Pauling
/// estimate; `None` for an element outside the table.
pub fn bond_enthalpy_kcal(a: i16, b: i16) -> Option<(f64, bool)> {
    match tabulated_enthalpy_kj(a, b) {
        Some(kj) => Some((kj / KJ_PER_KCAL, false)),
        None => Some((pauling_estimate_kj(a, b)? / KJ_PER_KCAL, true)),
    }
}
