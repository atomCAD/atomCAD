// UFF parameter table: 127 atom types with 11 parameters each.
//
// Values from Table 1 of the UFF paper (Rappé et al. 1992, JACS 114, 10024-10035).
// Ported from RDKit's Params.cpp (BSD-3-Clause).
//
// theta0 is stored in DEGREES here. Convert to radians when needed.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Per-atom-type UFF parameters.
#[derive(Debug, Clone, Copy)]
pub struct UffAtomParams {
    /// UFF atom type label (e.g., "C_3", "N_R").
    pub label: &'static str,
    /// Valence bond radius (Angstroms).
    pub r1: f64,
    /// Natural valence angle (degrees).
    pub theta0: f64,
    /// van der Waals characteristic length (Angstroms).
    pub x1: f64,
    /// van der Waals atomic energy (kcal/mol).
    pub d1: f64,
    /// van der Waals scaling term.
    pub zeta: f64,
    /// Effective charge.
    pub z1: f64,
    /// sp3 torsional barrier parameter (kcal/mol).
    pub v1: f64,
    /// Torsional contribution for sp2-sp3 bonds (kcal/mol).
    pub u1: f64,
    /// GMP electronegativity.
    pub gmp_xi: f64,
    /// GMP hardness.
    pub gmp_hardness: f64,
    /// GMP radius.
    pub gmp_radius: f64,
}

// Constants from RDKit's Params.h
/// Scaling factor for bond-order correction to bond rest length.
pub const LAMBDA: f64 = 0.1332;
/// Bond force constant prefactor (kcal/mol * Angstrom).
pub const G: f64 = 332.06;
/// Special case bond order for amide C-N bonds.
pub const AMIDE_BOND_ORDER: f64 = 1.41;
/// Threshold for angle correction penalty (cos(30 degrees) ≈ 0.8660).
/// When cos(theta) exceeds this value (i.e. angle < ~30 degrees), an exponential
/// penalty is added to prevent atom overlap. Borrowed from OpenBabel.
pub const ANGLE_CORRECTION_THRESHOLD: f64 = 0.8660;

/// Complete UFF parameter table (127 entries).
#[allow(clippy::approx_constant)] // 3.141 is a UFF parameter for Hafnium, not PI
pub static UFF_PARAMS: &[UffAtomParams] = &[
    // Row 1: H, He
    UffAtomParams {
        label: "H_",
        r1: 0.354,
        theta0: 180.0,
        x1: 2.886,
        d1: 0.044,
        zeta: 12.0,
        z1: 0.712,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 4.528,
        gmp_hardness: 6.9452,
        gmp_radius: 0.371,
    },
    UffAtomParams {
        label: "H_b",
        r1: 0.46,
        theta0: 83.5,
        x1: 2.886,
        d1: 0.044,
        zeta: 12.0,
        z1: 0.712,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 4.528,
        gmp_hardness: 6.9452,
        gmp_radius: 0.371,
    },
    UffAtomParams {
        label: "He4+4",
        r1: 0.849,
        theta0: 90.0,
        x1: 2.362,
        d1: 0.056,
        zeta: 15.24,
        z1: 0.098,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 9.66,
        gmp_hardness: 14.92,
        gmp_radius: 1.3,
    },
    // Row 2: Li - Ne
    UffAtomParams {
        label: "Li",
        r1: 1.336,
        theta0: 180.0,
        x1: 2.451,
        d1: 0.025,
        zeta: 12.0,
        z1: 1.026,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 3.006,
        gmp_hardness: 2.386,
        gmp_radius: 1.557,
    },
    UffAtomParams {
        label: "Be3+2",
        r1: 1.074,
        theta0: 109.47,
        x1: 2.745,
        d1: 0.085,
        zeta: 12.0,
        z1: 1.565,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 4.877,
        gmp_hardness: 4.443,
        gmp_radius: 1.24,
    },
    UffAtomParams {
        label: "B_3",
        r1: 0.838,
        theta0: 109.47,
        x1: 4.083,
        d1: 0.18,
        zeta: 12.052,
        z1: 1.755,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 5.11,
        gmp_hardness: 4.75,
        gmp_radius: 0.822,
    },
    UffAtomParams {
        label: "B_2",
        r1: 0.828,
        theta0: 120.0,
        x1: 4.083,
        d1: 0.18,
        zeta: 12.052,
        z1: 1.755,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 5.11,
        gmp_hardness: 4.75,
        gmp_radius: 0.822,
    },
    UffAtomParams {
        label: "C_3",
        r1: 0.757,
        theta0: 109.47,
        x1: 3.851,
        d1: 0.105,
        zeta: 12.73,
        z1: 1.912,
        v1: 2.119,
        u1: 2.0,
        gmp_xi: 5.343,
        gmp_hardness: 5.063,
        gmp_radius: 0.759,
    },
    UffAtomParams {
        label: "C_R",
        r1: 0.729,
        theta0: 120.0,
        x1: 3.851,
        d1: 0.105,
        zeta: 12.73,
        z1: 1.912,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 5.343,
        gmp_hardness: 5.063,
        gmp_radius: 0.759,
    },
    UffAtomParams {
        label: "C_2",
        r1: 0.732,
        theta0: 120.0,
        x1: 3.851,
        d1: 0.105,
        zeta: 12.73,
        z1: 1.912,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 5.343,
        gmp_hardness: 5.063,
        gmp_radius: 0.759,
    },
    UffAtomParams {
        label: "C_1",
        r1: 0.706,
        theta0: 180.0,
        x1: 3.851,
        d1: 0.105,
        zeta: 12.73,
        z1: 1.912,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 5.343,
        gmp_hardness: 5.063,
        gmp_radius: 0.759,
    },
    UffAtomParams {
        label: "N_3",
        r1: 0.7,
        theta0: 106.7,
        x1: 3.66,
        d1: 0.069,
        zeta: 13.407,
        z1: 2.544,
        v1: 0.45,
        u1: 2.0,
        gmp_xi: 6.899,
        gmp_hardness: 5.88,
        gmp_radius: 0.715,
    },
    UffAtomParams {
        label: "N_R",
        r1: 0.699,
        theta0: 120.0,
        x1: 3.66,
        d1: 0.069,
        zeta: 13.407,
        z1: 2.544,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 6.899,
        gmp_hardness: 5.88,
        gmp_radius: 0.715,
    },
    UffAtomParams {
        label: "N_2",
        r1: 0.685,
        theta0: 111.2,
        x1: 3.66,
        d1: 0.069,
        zeta: 13.407,
        z1: 2.544,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 6.899,
        gmp_hardness: 5.88,
        gmp_radius: 0.715,
    },
    UffAtomParams {
        label: "N_1",
        r1: 0.656,
        theta0: 180.0,
        x1: 3.66,
        d1: 0.069,
        zeta: 13.407,
        z1: 2.544,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 6.899,
        gmp_hardness: 5.88,
        gmp_radius: 0.715,
    },
    UffAtomParams {
        label: "O_3",
        r1: 0.658,
        theta0: 104.51,
        x1: 3.5,
        d1: 0.06,
        zeta: 14.085,
        z1: 2.3,
        v1: 0.018,
        u1: 2.0,
        gmp_xi: 8.741,
        gmp_hardness: 6.682,
        gmp_radius: 0.669,
    },
    UffAtomParams {
        label: "O_3_z",
        r1: 0.528,
        theta0: 146.0,
        x1: 3.5,
        d1: 0.06,
        zeta: 14.085,
        z1: 2.3,
        v1: 0.018,
        u1: 2.0,
        gmp_xi: 8.741,
        gmp_hardness: 6.682,
        gmp_radius: 0.669,
    },
    UffAtomParams {
        label: "O_R",
        r1: 0.68,
        theta0: 110.0,
        x1: 3.5,
        d1: 0.06,
        zeta: 14.085,
        z1: 2.3,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 8.741,
        gmp_hardness: 6.682,
        gmp_radius: 0.669,
    },
    UffAtomParams {
        label: "O_2",
        r1: 0.634,
        theta0: 120.0,
        x1: 3.5,
        d1: 0.06,
        zeta: 14.085,
        z1: 2.3,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 8.741,
        gmp_hardness: 6.682,
        gmp_radius: 0.669,
    },
    UffAtomParams {
        label: "O_1",
        r1: 0.639,
        theta0: 180.0,
        x1: 3.5,
        d1: 0.06,
        zeta: 14.085,
        z1: 2.3,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 8.741,
        gmp_hardness: 6.682,
        gmp_radius: 0.669,
    },
    UffAtomParams {
        label: "F_",
        r1: 0.668,
        theta0: 180.0,
        x1: 3.364,
        d1: 0.05,
        zeta: 14.762,
        z1: 1.735,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 10.874,
        gmp_hardness: 7.474,
        gmp_radius: 0.706,
    },
    UffAtomParams {
        label: "Ne4+4",
        r1: 0.92,
        theta0: 90.0,
        x1: 3.243,
        d1: 0.042,
        zeta: 15.44,
        z1: 0.194,
        v1: 0.0,
        u1: 2.0,
        gmp_xi: 11.04,
        gmp_hardness: 10.55,
        gmp_radius: 1.768,
    },
    // Row 3: Na - Ar
    UffAtomParams {
        label: "Na",
        r1: 1.539,
        theta0: 180.0,
        x1: 2.983,
        d1: 0.03,
        zeta: 12.0,
        z1: 1.081,
        v1: 0.0,
        u1: 1.25,
        gmp_xi: 2.843,
        gmp_hardness: 2.296,
        gmp_radius: 2.085,
    },
    UffAtomParams {
        label: "Mg3+2",
        r1: 1.421,
        theta0: 109.47,
        x1: 3.021,
        d1: 0.111,
        zeta: 12.0,
        z1: 1.787,
        v1: 0.0,
        u1: 1.25,
        gmp_xi: 3.951,
        gmp_hardness: 3.693,
        gmp_radius: 1.5,
    },
    UffAtomParams {
        label: "Al3",
        r1: 1.244,
        theta0: 109.47,
        x1: 4.499,
        d1: 0.505,
        zeta: 11.278,
        z1: 1.792,
        v1: 0.0,
        u1: 1.25,
        gmp_xi: 4.06,
        gmp_hardness: 3.59,
        gmp_radius: 1.201,
    },
    UffAtomParams {
        label: "Si3",
        r1: 1.117,
        theta0: 109.47,
        x1: 4.295,
        d1: 0.402,
        zeta: 12.175,
        z1: 2.323,
        v1: 1.225,
        u1: 1.25,
        gmp_xi: 4.168,
        gmp_hardness: 3.487,
        gmp_radius: 1.176,
    },
    UffAtomParams {
        label: "P_3+3",
        r1: 1.101,
        theta0: 93.8,
        x1: 4.147,
        d1: 0.305,
        zeta: 13.072,
        z1: 2.863,
        v1: 2.4,
        u1: 1.25,
        gmp_xi: 5.463,
        gmp_hardness: 4.0,
        gmp_radius: 1.102,
    },
    UffAtomParams {
        label: "P_3+5",
        r1: 1.056,
        theta0: 109.47,
        x1: 4.147,
        d1: 0.305,
        zeta: 13.072,
        z1: 2.863,
        v1: 2.4,
        u1: 1.25,
        gmp_xi: 5.463,
        gmp_hardness: 4.0,
        gmp_radius: 1.102,
    },
    UffAtomParams {
        label: "P_3+q",
        r1: 1.056,
        theta0: 109.47,
        x1: 4.147,
        d1: 0.305,
        zeta: 13.072,
        z1: 2.863,
        v1: 2.4,
        u1: 1.25,
        gmp_xi: 5.463,
        gmp_hardness: 4.0,
        gmp_radius: 1.102,
    },
    UffAtomParams {
        label: "S_3+2",
        r1: 1.064,
        theta0: 92.1,
        x1: 4.035,
        d1: 0.274,
        zeta: 13.969,
        z1: 2.703,
        v1: 0.484,
        u1: 1.25,
        gmp_xi: 6.928,
        gmp_hardness: 4.486,
        gmp_radius: 1.047,
    },
    UffAtomParams {
        label: "S_3+4",
        r1: 1.049,
        theta0: 103.2,
        x1: 4.035,
        d1: 0.274,
        zeta: 13.969,
        z1: 2.703,
        v1: 0.484,
        u1: 1.25,
        gmp_xi: 6.928,
        gmp_hardness: 4.486,
        gmp_radius: 1.047,
    },
    UffAtomParams {
        label: "S_3+6",
        r1: 1.027,
        theta0: 109.47,
        x1: 4.035,
        d1: 0.274,
        zeta: 13.969,
        z1: 2.703,
        v1: 0.484,
        u1: 1.25,
        gmp_xi: 6.928,
        gmp_hardness: 4.486,
        gmp_radius: 1.047,
    },
    UffAtomParams {
        label: "S_R",
        r1: 1.077,
        theta0: 92.2,
        x1: 4.035,
        d1: 0.274,
        zeta: 13.969,
        z1: 2.703,
        v1: 0.0,
        u1: 1.25,
        gmp_xi: 6.928,
        gmp_hardness: 4.486,
        gmp_radius: 1.047,
    },
    UffAtomParams {
        label: "S_2",
        r1: 0.854,
        theta0: 120.0,
        x1: 4.035,
        d1: 0.274,
        zeta: 13.969,
        z1: 2.703,
        v1: 0.0,
        u1: 1.25,
        gmp_xi: 6.928,
        gmp_hardness: 4.486,
        gmp_radius: 1.047,
    },
    UffAtomParams {
        label: "Cl",
        r1: 1.044,
        theta0: 180.0,
        x1: 3.947,
        d1: 0.227,
        zeta: 14.866,
        z1: 2.348,
        v1: 0.0,
        u1: 1.25,
        gmp_xi: 8.564,
        gmp_hardness: 4.946,
        gmp_radius: 0.994,
    },
    UffAtomParams {
        label: "Ar4+4",
        r1: 1.032,
        theta0: 90.0,
        x1: 3.868,
        d1: 0.185,
        zeta: 15.763,
        z1: 0.3,
        v1: 0.0,
        u1: 1.25,
        gmp_xi: 9.465,
        gmp_hardness: 6.355,
        gmp_radius: 2.108,
    },
    // Row 4: K - Kr
    UffAtomParams {
        label: "K_",
        r1: 1.953,
        theta0: 180.0,
        x1: 3.812,
        d1: 0.035,
        zeta: 12.0,
        z1: 1.165,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 2.421,
        gmp_hardness: 1.92,
        gmp_radius: 2.586,
    },
    UffAtomParams {
        label: "Ca6+2",
        r1: 1.761,
        theta0: 90.0,
        x1: 3.399,
        d1: 0.238,
        zeta: 12.0,
        z1: 2.141,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 3.231,
        gmp_hardness: 2.88,
        gmp_radius: 2.0,
    },
    UffAtomParams {
        label: "Sc3+3",
        r1: 1.513,
        theta0: 109.47,
        x1: 3.295,
        d1: 0.019,
        zeta: 12.0,
        z1: 2.592,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 3.395,
        gmp_hardness: 3.08,
        gmp_radius: 1.75,
    },
    UffAtomParams {
        label: "Ti3+4",
        r1: 1.412,
        theta0: 109.47,
        x1: 3.175,
        d1: 0.017,
        zeta: 12.0,
        z1: 2.659,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 3.47,
        gmp_hardness: 3.38,
        gmp_radius: 1.607,
    },
    UffAtomParams {
        label: "Ti6+4",
        r1: 1.412,
        theta0: 90.0,
        x1: 3.175,
        d1: 0.017,
        zeta: 12.0,
        z1: 2.659,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 3.47,
        gmp_hardness: 3.38,
        gmp_radius: 1.607,
    },
    UffAtomParams {
        label: "V_3+5",
        r1: 1.402,
        theta0: 109.47,
        x1: 3.144,
        d1: 0.016,
        zeta: 12.0,
        z1: 2.679,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 3.65,
        gmp_hardness: 3.41,
        gmp_radius: 1.47,
    },
    UffAtomParams {
        label: "Cr6+3",
        r1: 1.345,
        theta0: 90.0,
        x1: 3.023,
        d1: 0.015,
        zeta: 12.0,
        z1: 2.463,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 3.415,
        gmp_hardness: 3.865,
        gmp_radius: 1.402,
    },
    UffAtomParams {
        label: "Mn6+2",
        r1: 1.382,
        theta0: 90.0,
        x1: 2.961,
        d1: 0.013,
        zeta: 12.0,
        z1: 2.43,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 3.325,
        gmp_hardness: 4.105,
        gmp_radius: 1.533,
    },
    UffAtomParams {
        label: "Fe3+2",
        r1: 1.27,
        theta0: 109.47,
        x1: 2.912,
        d1: 0.013,
        zeta: 12.0,
        z1: 2.43,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 3.76,
        gmp_hardness: 4.14,
        gmp_radius: 1.393,
    },
    UffAtomParams {
        label: "Fe6+2",
        r1: 1.335,
        theta0: 90.0,
        x1: 2.912,
        d1: 0.013,
        zeta: 12.0,
        z1: 2.43,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 3.76,
        gmp_hardness: 4.14,
        gmp_radius: 1.393,
    },
    UffAtomParams {
        label: "Co6+3",
        r1: 1.241,
        theta0: 90.0,
        x1: 2.872,
        d1: 0.014,
        zeta: 12.0,
        z1: 2.43,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 4.105,
        gmp_hardness: 4.175,
        gmp_radius: 1.406,
    },
    UffAtomParams {
        label: "Ni4+2",
        r1: 1.164,
        theta0: 90.0,
        x1: 2.834,
        d1: 0.015,
        zeta: 12.0,
        z1: 2.43,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 4.465,
        gmp_hardness: 4.205,
        gmp_radius: 1.398,
    },
    UffAtomParams {
        label: "Cu3+1",
        r1: 1.302,
        theta0: 109.47,
        x1: 3.495,
        d1: 0.005,
        zeta: 12.0,
        z1: 1.756,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 4.2,
        gmp_hardness: 4.22,
        gmp_radius: 1.434,
    },
    UffAtomParams {
        label: "Zn3+2",
        r1: 1.193,
        theta0: 109.47,
        x1: 2.763,
        d1: 0.124,
        zeta: 12.0,
        z1: 1.308,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 5.106,
        gmp_hardness: 4.285,
        gmp_radius: 1.4,
    },
    UffAtomParams {
        label: "Ga3+3",
        r1: 1.26,
        theta0: 109.47,
        x1: 4.383,
        d1: 0.415,
        zeta: 11.0,
        z1: 1.821,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 3.641,
        gmp_hardness: 3.16,
        gmp_radius: 1.211,
    },
    UffAtomParams {
        label: "Ge3",
        r1: 1.197,
        theta0: 109.47,
        x1: 4.28,
        d1: 0.379,
        zeta: 12.0,
        z1: 2.789,
        v1: 0.701,
        u1: 0.7,
        gmp_xi: 4.051,
        gmp_hardness: 3.438,
        gmp_radius: 1.189,
    },
    UffAtomParams {
        label: "As3+3",
        r1: 1.211,
        theta0: 92.1,
        x1: 4.23,
        d1: 0.309,
        zeta: 13.0,
        z1: 2.864,
        v1: 1.5,
        u1: 0.7,
        gmp_xi: 5.188,
        gmp_hardness: 3.809,
        gmp_radius: 1.204,
    },
    UffAtomParams {
        label: "Se3+2",
        r1: 1.19,
        theta0: 90.6,
        x1: 4.205,
        d1: 0.291,
        zeta: 14.0,
        z1: 2.764,
        v1: 0.335,
        u1: 0.7,
        gmp_xi: 6.428,
        gmp_hardness: 4.131,
        gmp_radius: 1.224,
    },
    UffAtomParams {
        label: "Br",
        r1: 1.192,
        theta0: 180.0,
        x1: 4.189,
        d1: 0.251,
        zeta: 15.0,
        z1: 2.519,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 7.79,
        gmp_hardness: 4.425,
        gmp_radius: 1.141,
    },
    UffAtomParams {
        label: "Kr4+4",
        r1: 1.147,
        theta0: 90.0,
        x1: 4.141,
        d1: 0.22,
        zeta: 16.0,
        z1: 0.452,
        v1: 0.0,
        u1: 0.7,
        gmp_xi: 8.505,
        gmp_hardness: 5.715,
        gmp_radius: 2.27,
    },
    // Row 5: Rb - Xe
    UffAtomParams {
        label: "Rb",
        r1: 2.26,
        theta0: 180.0,
        x1: 4.114,
        d1: 0.04,
        zeta: 12.0,
        z1: 1.592,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 2.331,
        gmp_hardness: 1.846,
        gmp_radius: 2.77,
    },
    UffAtomParams {
        label: "Sr6+2",
        r1: 2.052,
        theta0: 90.0,
        x1: 3.641,
        d1: 0.235,
        zeta: 12.0,
        z1: 2.449,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 3.024,
        gmp_hardness: 2.44,
        gmp_radius: 2.415,
    },
    UffAtomParams {
        label: "Y_3+3",
        r1: 1.698,
        theta0: 109.47,
        x1: 3.345,
        d1: 0.072,
        zeta: 12.0,
        z1: 3.257,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 3.83,
        gmp_hardness: 2.81,
        gmp_radius: 1.998,
    },
    UffAtomParams {
        label: "Zr3+4",
        r1: 1.564,
        theta0: 109.47,
        x1: 3.124,
        d1: 0.069,
        zeta: 12.0,
        z1: 3.667,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 3.4,
        gmp_hardness: 3.55,
        gmp_radius: 1.758,
    },
    UffAtomParams {
        label: "Nb3+5",
        r1: 1.473,
        theta0: 109.47,
        x1: 3.165,
        d1: 0.059,
        zeta: 12.0,
        z1: 3.618,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 3.55,
        gmp_hardness: 3.38,
        gmp_radius: 1.603,
    },
    UffAtomParams {
        label: "Mo6+6",
        r1: 1.467,
        theta0: 90.0,
        x1: 3.052,
        d1: 0.056,
        zeta: 12.0,
        z1: 3.4,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 3.465,
        gmp_hardness: 3.755,
        gmp_radius: 1.53,
    },
    UffAtomParams {
        label: "Mo3+6",
        r1: 1.484,
        theta0: 109.47,
        x1: 3.052,
        d1: 0.056,
        zeta: 12.0,
        z1: 3.4,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 3.465,
        gmp_hardness: 3.755,
        gmp_radius: 1.53,
    },
    UffAtomParams {
        label: "Tc6+5",
        r1: 1.322,
        theta0: 90.0,
        x1: 2.998,
        d1: 0.048,
        zeta: 12.0,
        z1: 3.4,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 3.29,
        gmp_hardness: 3.99,
        gmp_radius: 1.5,
    },
    UffAtomParams {
        label: "Ru6+2",
        r1: 1.478,
        theta0: 90.0,
        x1: 2.963,
        d1: 0.056,
        zeta: 12.0,
        z1: 3.4,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 3.575,
        gmp_hardness: 4.015,
        gmp_radius: 1.5,
    },
    UffAtomParams {
        label: "Rh6+3",
        r1: 1.332,
        theta0: 90.0,
        x1: 2.929,
        d1: 0.053,
        zeta: 12.0,
        z1: 3.5,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 3.975,
        gmp_hardness: 4.005,
        gmp_radius: 1.509,
    },
    UffAtomParams {
        label: "Pd4+2",
        r1: 1.338,
        theta0: 90.0,
        x1: 2.899,
        d1: 0.048,
        zeta: 12.0,
        z1: 3.21,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 4.32,
        gmp_hardness: 4.0,
        gmp_radius: 1.544,
    },
    UffAtomParams {
        label: "Ag1+1",
        r1: 1.386,
        theta0: 180.0,
        x1: 3.148,
        d1: 0.036,
        zeta: 12.0,
        z1: 1.956,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 4.436,
        gmp_hardness: 3.134,
        gmp_radius: 1.622,
    },
    UffAtomParams {
        label: "Cd3+2",
        r1: 1.403,
        theta0: 109.47,
        x1: 2.848,
        d1: 0.228,
        zeta: 12.0,
        z1: 1.65,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 5.034,
        gmp_hardness: 3.957,
        gmp_radius: 1.6,
    },
    UffAtomParams {
        label: "In3+3",
        r1: 1.459,
        theta0: 109.47,
        x1: 4.463,
        d1: 0.599,
        zeta: 11.0,
        z1: 2.07,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 3.506,
        gmp_hardness: 2.896,
        gmp_radius: 1.404,
    },
    UffAtomParams {
        label: "Sn3",
        r1: 1.398,
        theta0: 109.47,
        x1: 4.392,
        d1: 0.567,
        zeta: 12.0,
        z1: 2.961,
        v1: 0.199,
        u1: 0.2,
        gmp_xi: 3.987,
        gmp_hardness: 3.124,
        gmp_radius: 1.354,
    },
    UffAtomParams {
        label: "Sb3+3",
        r1: 1.407,
        theta0: 91.6,
        x1: 4.42,
        d1: 0.449,
        zeta: 13.0,
        z1: 2.704,
        v1: 1.1,
        u1: 0.2,
        gmp_xi: 4.899,
        gmp_hardness: 3.342,
        gmp_radius: 1.404,
    },
    UffAtomParams {
        label: "Te3+2",
        r1: 1.386,
        theta0: 90.25,
        x1: 4.47,
        d1: 0.398,
        zeta: 14.0,
        z1: 2.882,
        v1: 0.3,
        u1: 0.2,
        gmp_xi: 5.816,
        gmp_hardness: 3.526,
        gmp_radius: 1.38,
    },
    UffAtomParams {
        label: "I_",
        r1: 1.382,
        theta0: 180.0,
        x1: 4.5,
        d1: 0.339,
        zeta: 15.0,
        z1: 2.65,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 6.822,
        gmp_hardness: 3.762,
        gmp_radius: 1.333,
    },
    UffAtomParams {
        label: "Xe4+4",
        r1: 1.267,
        theta0: 90.0,
        x1: 4.404,
        d1: 0.332,
        zeta: 12.0,
        z1: 0.556,
        v1: 0.0,
        u1: 0.2,
        gmp_xi: 7.595,
        gmp_hardness: 4.975,
        gmp_radius: 2.459,
    },
    // Row 6: Cs - Rn
    UffAtomParams {
        label: "Cs",
        r1: 2.57,
        theta0: 180.0,
        x1: 4.517,
        d1: 0.045,
        zeta: 12.0,
        z1: 1.573,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 2.183,
        gmp_hardness: 1.711,
        gmp_radius: 2.984,
    },
    UffAtomParams {
        label: "Ba6+2",
        r1: 2.277,
        theta0: 90.0,
        x1: 3.703,
        d1: 0.364,
        zeta: 12.0,
        z1: 2.727,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 2.814,
        gmp_hardness: 2.396,
        gmp_radius: 2.442,
    },
    UffAtomParams {
        label: "La3+3",
        r1: 1.943,
        theta0: 109.47,
        x1: 3.522,
        d1: 0.017,
        zeta: 12.0,
        z1: 3.3,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 2.8355,
        gmp_hardness: 2.7415,
        gmp_radius: 2.071,
    },
    UffAtomParams {
        label: "Ce6+3",
        r1: 1.841,
        theta0: 90.0,
        x1: 3.556,
        d1: 0.013,
        zeta: 12.0,
        z1: 3.3,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 2.774,
        gmp_hardness: 2.692,
        gmp_radius: 1.925,
    },
    UffAtomParams {
        label: "Pr6+3",
        r1: 1.823,
        theta0: 90.0,
        x1: 3.606,
        d1: 0.01,
        zeta: 12.0,
        z1: 3.3,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 2.858,
        gmp_hardness: 2.564,
        gmp_radius: 2.007,
    },
    UffAtomParams {
        label: "Nd6+3",
        r1: 1.816,
        theta0: 90.0,
        x1: 3.575,
        d1: 0.01,
        zeta: 12.0,
        z1: 3.3,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 2.8685,
        gmp_hardness: 2.6205,
        gmp_radius: 2.007,
    },
    UffAtomParams {
        label: "Pm6+3",
        r1: 1.801,
        theta0: 90.0,
        x1: 3.547,
        d1: 0.009,
        zeta: 12.0,
        z1: 3.3,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 2.881,
        gmp_hardness: 2.673,
        gmp_radius: 2.0,
    },
    UffAtomParams {
        label: "Sm6+3",
        r1: 1.78,
        theta0: 90.0,
        x1: 3.52,
        d1: 0.008,
        zeta: 12.0,
        z1: 3.3,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 2.9115,
        gmp_hardness: 2.7195,
        gmp_radius: 1.978,
    },
    UffAtomParams {
        label: "Eu6+3",
        r1: 1.771,
        theta0: 90.0,
        x1: 3.493,
        d1: 0.008,
        zeta: 12.0,
        z1: 3.3,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 2.8785,
        gmp_hardness: 2.7875,
        gmp_radius: 2.227,
    },
    UffAtomParams {
        label: "Gd6+3",
        r1: 1.735,
        theta0: 90.0,
        x1: 3.368,
        d1: 0.009,
        zeta: 12.0,
        z1: 3.3,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 3.1665,
        gmp_hardness: 2.9745,
        gmp_radius: 1.968,
    },
    UffAtomParams {
        label: "Tb6+3",
        r1: 1.732,
        theta0: 90.0,
        x1: 3.451,
        d1: 0.007,
        zeta: 12.0,
        z1: 3.3,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 3.018,
        gmp_hardness: 2.834,
        gmp_radius: 1.954,
    },
    UffAtomParams {
        label: "Dy6+3",
        r1: 1.71,
        theta0: 90.0,
        x1: 3.428,
        d1: 0.007,
        zeta: 12.0,
        z1: 3.3,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 3.0555,
        gmp_hardness: 2.8715,
        gmp_radius: 1.934,
    },
    UffAtomParams {
        label: "Ho6+3",
        r1: 1.696,
        theta0: 90.0,
        x1: 3.409,
        d1: 0.007,
        zeta: 12.0,
        z1: 3.416,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 3.127,
        gmp_hardness: 2.891,
        gmp_radius: 1.925,
    },
    UffAtomParams {
        label: "Er6+3",
        r1: 1.673,
        theta0: 90.0,
        x1: 3.391,
        d1: 0.007,
        zeta: 12.0,
        z1: 3.3,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 3.1865,
        gmp_hardness: 2.9145,
        gmp_radius: 1.915,
    },
    UffAtomParams {
        label: "Tm6+3",
        r1: 1.66,
        theta0: 90.0,
        x1: 3.374,
        d1: 0.006,
        zeta: 12.0,
        z1: 3.3,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 3.2514,
        gmp_hardness: 2.9329,
        gmp_radius: 2.0,
    },
    UffAtomParams {
        label: "Yb6+3",
        r1: 1.637,
        theta0: 90.0,
        x1: 3.355,
        d1: 0.228,
        zeta: 12.0,
        z1: 2.618,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 3.2889,
        gmp_hardness: 2.965,
        gmp_radius: 2.158,
    },
    UffAtomParams {
        label: "Lu6+3",
        r1: 1.671,
        theta0: 90.0,
        x1: 3.64,
        d1: 0.041,
        zeta: 12.0,
        z1: 3.271,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 2.9629,
        gmp_hardness: 2.4629,
        gmp_radius: 1.896,
    },
    UffAtomParams {
        label: "Hf3+4",
        r1: 1.611,
        theta0: 109.47,
        x1: 3.141,
        d1: 0.072,
        zeta: 12.0,
        z1: 3.921,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 3.7,
        gmp_hardness: 3.4,
        gmp_radius: 1.759,
    },
    UffAtomParams {
        label: "Ta3+5",
        r1: 1.511,
        theta0: 109.47,
        x1: 3.17,
        d1: 0.081,
        zeta: 12.0,
        z1: 4.075,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 5.1,
        gmp_hardness: 2.85,
        gmp_radius: 1.605,
    },
    UffAtomParams {
        label: "W_6+6",
        r1: 1.392,
        theta0: 90.0,
        x1: 3.069,
        d1: 0.067,
        zeta: 12.0,
        z1: 3.7,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 4.63,
        gmp_hardness: 3.31,
        gmp_radius: 1.538,
    },
    UffAtomParams {
        label: "W_3+4",
        r1: 1.526,
        theta0: 109.47,
        x1: 3.069,
        d1: 0.067,
        zeta: 12.0,
        z1: 3.7,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 4.63,
        gmp_hardness: 3.31,
        gmp_radius: 1.538,
    },
    UffAtomParams {
        label: "W_3+6",
        r1: 1.38,
        theta0: 109.47,
        x1: 3.069,
        d1: 0.067,
        zeta: 12.0,
        z1: 3.7,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 4.63,
        gmp_hardness: 3.31,
        gmp_radius: 1.538,
    },
    UffAtomParams {
        label: "Re6+5",
        r1: 1.372,
        theta0: 90.0,
        x1: 2.954,
        d1: 0.066,
        zeta: 12.0,
        z1: 3.7,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 3.96,
        gmp_hardness: 3.92,
        gmp_radius: 1.6,
    },
    UffAtomParams {
        label: "Re3+7",
        r1: 1.314,
        theta0: 109.47,
        x1: 2.954,
        d1: 0.066,
        zeta: 12.0,
        z1: 3.7,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 3.96,
        gmp_hardness: 3.92,
        gmp_radius: 1.6,
    },
    UffAtomParams {
        label: "Os6+6",
        r1: 1.372,
        theta0: 90.0,
        x1: 3.12,
        d1: 0.037,
        zeta: 12.0,
        z1: 3.7,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 5.14,
        gmp_hardness: 3.63,
        gmp_radius: 1.7,
    },
    UffAtomParams {
        label: "Ir6+3",
        r1: 1.371,
        theta0: 90.0,
        x1: 2.84,
        d1: 0.073,
        zeta: 12.0,
        z1: 3.731,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 5.0,
        gmp_hardness: 4.0,
        gmp_radius: 1.866,
    },
    UffAtomParams {
        label: "Pt4+2",
        r1: 1.364,
        theta0: 90.0,
        x1: 2.754,
        d1: 0.08,
        zeta: 12.0,
        z1: 3.382,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 4.79,
        gmp_hardness: 4.43,
        gmp_radius: 1.557,
    },
    UffAtomParams {
        label: "Au4+3",
        r1: 1.262,
        theta0: 90.0,
        x1: 3.293,
        d1: 0.039,
        zeta: 12.0,
        z1: 2.625,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 4.894,
        gmp_hardness: 2.586,
        gmp_radius: 1.618,
    },
    UffAtomParams {
        label: "Hg1+2",
        r1: 1.34,
        theta0: 180.0,
        x1: 2.705,
        d1: 0.385,
        zeta: 12.0,
        z1: 1.75,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 6.27,
        gmp_hardness: 4.16,
        gmp_radius: 1.6,
    },
    UffAtomParams {
        label: "Tl3+3",
        r1: 1.518,
        theta0: 120.0,
        x1: 4.347,
        d1: 0.68,
        zeta: 11.0,
        z1: 2.068,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 3.2,
        gmp_hardness: 2.9,
        gmp_radius: 1.53,
    },
    UffAtomParams {
        label: "Pb3",
        r1: 1.459,
        theta0: 109.47,
        x1: 4.297,
        d1: 0.663,
        zeta: 12.0,
        z1: 2.846,
        v1: 0.1,
        u1: 0.1,
        gmp_xi: 3.9,
        gmp_hardness: 3.53,
        gmp_radius: 1.444,
    },
    UffAtomParams {
        label: "Bi3+3",
        r1: 1.512,
        theta0: 90.0,
        x1: 4.37,
        d1: 0.518,
        zeta: 13.0,
        z1: 2.47,
        v1: 1.0,
        u1: 0.1,
        gmp_xi: 4.69,
        gmp_hardness: 3.74,
        gmp_radius: 1.514,
    },
    UffAtomParams {
        label: "Po3+2",
        r1: 1.5,
        theta0: 90.0,
        x1: 4.709,
        d1: 0.325,
        zeta: 14.0,
        z1: 2.33,
        v1: 0.3,
        u1: 0.1,
        gmp_xi: 4.21,
        gmp_hardness: 4.21,
        gmp_radius: 1.48,
    },
    UffAtomParams {
        label: "At",
        r1: 1.545,
        theta0: 180.0,
        x1: 4.75,
        d1: 0.284,
        zeta: 15.0,
        z1: 2.24,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 4.75,
        gmp_hardness: 4.75,
        gmp_radius: 1.47,
    },
    UffAtomParams {
        label: "Rn4+4",
        r1: 1.42,
        theta0: 90.0,
        x1: 4.765,
        d1: 0.248,
        zeta: 16.0,
        z1: 0.583,
        v1: 0.0,
        u1: 0.1,
        gmp_xi: 5.37,
        gmp_hardness: 5.37,
        gmp_radius: 2.2,
    },
    // Row 7: Fr - Lw (actinides)
    UffAtomParams {
        label: "Fr",
        r1: 2.88,
        theta0: 180.0,
        x1: 4.9,
        d1: 0.05,
        zeta: 12.0,
        z1: 1.847,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 2.0,
        gmp_hardness: 2.0,
        gmp_radius: 2.3,
    },
    UffAtomParams {
        label: "Ra6+2",
        r1: 2.512,
        theta0: 90.0,
        x1: 3.677,
        d1: 0.404,
        zeta: 12.0,
        z1: 2.92,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 2.843,
        gmp_hardness: 2.434,
        gmp_radius: 2.2,
    },
    UffAtomParams {
        label: "Ac6+3",
        r1: 1.983,
        theta0: 90.0,
        x1: 3.478,
        d1: 0.033,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 2.835,
        gmp_hardness: 2.835,
        gmp_radius: 2.108,
    },
    UffAtomParams {
        label: "Th6+4",
        r1: 1.721,
        theta0: 90.0,
        x1: 3.396,
        d1: 0.026,
        zeta: 12.0,
        z1: 4.202,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 3.175,
        gmp_hardness: 2.905,
        gmp_radius: 2.018,
    },
    UffAtomParams {
        label: "Pa6+4",
        r1: 1.711,
        theta0: 90.0,
        x1: 3.424,
        d1: 0.022,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 2.985,
        gmp_hardness: 2.905,
        gmp_radius: 1.8,
    },
    UffAtomParams {
        label: "U_6+4",
        r1: 1.684,
        theta0: 90.0,
        x1: 3.395,
        d1: 0.022,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 3.341,
        gmp_hardness: 2.853,
        gmp_radius: 1.713,
    },
    UffAtomParams {
        label: "Np6+4",
        r1: 1.666,
        theta0: 90.0,
        x1: 3.424,
        d1: 0.019,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 3.549,
        gmp_hardness: 2.717,
        gmp_radius: 1.8,
    },
    UffAtomParams {
        label: "Pu6+4",
        r1: 1.657,
        theta0: 90.0,
        x1: 3.424,
        d1: 0.016,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 3.243,
        gmp_hardness: 2.819,
        gmp_radius: 1.84,
    },
    UffAtomParams {
        label: "Am6+4",
        r1: 1.66,
        theta0: 90.0,
        x1: 3.381,
        d1: 0.014,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 2.9895,
        gmp_hardness: 3.0035,
        gmp_radius: 1.942,
    },
    UffAtomParams {
        label: "Cm6+3",
        r1: 1.801,
        theta0: 90.0,
        x1: 3.326,
        d1: 0.013,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 2.8315,
        gmp_hardness: 3.1895,
        gmp_radius: 1.9,
    },
    UffAtomParams {
        label: "Bk6+3",
        r1: 1.761,
        theta0: 90.0,
        x1: 3.339,
        d1: 0.013,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 3.1935,
        gmp_hardness: 3.0355,
        gmp_radius: 1.9,
    },
    UffAtomParams {
        label: "Cf6+3",
        r1: 1.75,
        theta0: 90.0,
        x1: 3.313,
        d1: 0.013,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 3.197,
        gmp_hardness: 3.101,
        gmp_radius: 1.9,
    },
    UffAtomParams {
        label: "Es6+3",
        r1: 1.724,
        theta0: 90.0,
        x1: 3.299,
        d1: 0.012,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 3.333,
        gmp_hardness: 3.089,
        gmp_radius: 1.9,
    },
    UffAtomParams {
        label: "Fm6+3",
        r1: 1.712,
        theta0: 90.0,
        x1: 3.286,
        d1: 0.012,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 3.4,
        gmp_hardness: 3.1,
        gmp_radius: 1.9,
    },
    UffAtomParams {
        label: "Md6+3",
        r1: 1.689,
        theta0: 90.0,
        x1: 3.274,
        d1: 0.011,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 3.47,
        gmp_hardness: 3.11,
        gmp_radius: 1.9,
    },
    UffAtomParams {
        label: "No6+3",
        r1: 1.679,
        theta0: 90.0,
        x1: 3.248,
        d1: 0.011,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 3.475,
        gmp_hardness: 3.175,
        gmp_radius: 1.9,
    },
    UffAtomParams {
        label: "Lw6+3",
        r1: 1.698,
        theta0: 90.0,
        x1: 3.236,
        d1: 0.011,
        zeta: 12.0,
        z1: 3.9,
        v1: 0.0,
        u1: 0.0,
        gmp_xi: 3.5,
        gmp_hardness: 3.2,
        gmp_radius: 1.9,
    },
];

/// Label-to-index lookup map, built on first access.
static LABEL_TO_INDEX: LazyLock<HashMap<&'static str, usize>> = LazyLock::new(|| {
    let mut map = HashMap::with_capacity(UFF_PARAMS.len());
    for (i, p) in UFF_PARAMS.iter().enumerate() {
        map.insert(p.label, i);
    }
    map
});

/// Look up UFF parameters by atom type label.
///
/// Returns `None` if the label is not found in the table.
pub fn get_uff_params(label: &str) -> Option<&'static UffAtomParams> {
    LABEL_TO_INDEX.get(label).map(|&i| &UFF_PARAMS[i])
}

/// Calculates the UFF equilibrium bond rest length.
///
/// Applies the Pauling bond-order correction and O'Keeffe-Brese electronegativity
/// correction to the sum of covalent radii.
///
/// Ported from RDKit's `Utils::calcBondRestLength()` in BondStretch.cpp (BSD-3-Clause).
///
/// # Arguments
/// * `bond_order` - Bond order (1.0 for single, 1.5 for aromatic, 2.0 for double, etc.)
/// * `params_i` - UFF parameters for atom i
/// * `params_j` - UFF parameters for atom j
pub fn calc_bond_rest_length(
    bond_order: f64,
    params_i: &UffAtomParams,
    params_j: &UffAtomParams,
) -> f64 {
    let ri = params_i.r1;
    let rj = params_j.r1;

    // Pauling bond-order correction
    let r_bo = -LAMBDA * (ri + rj) * bond_order.ln();

    // O'Keeffe-Brese electronegativity correction
    let xi = params_i.gmp_xi;
    let xj = params_j.gmp_xi;
    let sqrt_diff = xi.sqrt() - xj.sqrt();
    let r_en = ri * rj * sqrt_diff * sqrt_diff / (xi * ri + xj * rj);

    ri + rj + r_bo - r_en
}

/// Calculates the UFF bond stretching force constant.
///
/// k = 2 * G * Z_i * Z_j / r0^3
///
/// where G is the force constant prefactor (332.06 kcal/mol·Å) and Z_i, Z_j are
/// the effective charges.
///
/// Ported from RDKit's `Utils::calcBondForceConstant()` in BondStretch.cpp (BSD-3-Clause).
///
/// # Arguments
/// * `rest_length` - The equilibrium bond rest length (from `calc_bond_rest_length`)
/// * `params_i` - UFF parameters for atom i
/// * `params_j` - UFF parameters for atom j
pub fn calc_bond_force_constant(
    rest_length: f64,
    params_i: &UffAtomParams,
    params_j: &UffAtomParams,
) -> f64 {
    2.0 * G * params_i.z1 * params_j.z1 / (rest_length * rest_length * rest_length)
}

/// Calculates the UFF angle bending force constant.
///
/// Uses the law of cosines to compute r13 from r12, r23, and theta0, then
/// applies the UFF formula involving effective charges and bond lengths.
///
/// Ported from RDKit's `Utils::calcAngleForceConstant()` in AngleBend.cpp (BSD-3-Clause).
///
/// # Arguments
/// * `theta0` - Equilibrium angle in radians
/// * `bond_order12` - Bond order between atoms 1 and 2
/// * `bond_order23` - Bond order between atoms 2 and 3
/// * `at1` - UFF parameters for atom 1 (end atom)
/// * `at2` - UFF parameters for atom 2 (vertex atom)
/// * `at3` - UFF parameters for atom 3 (end atom)
pub fn calc_angle_force_constant(
    theta0: f64,
    bond_order12: f64,
    bond_order23: f64,
    at1: &UffAtomParams,
    at2: &UffAtomParams,
    at3: &UffAtomParams,
) -> f64 {
    let cos_theta0 = theta0.cos();
    let r12 = calc_bond_rest_length(bond_order12, at1, at2);
    let r23 = calc_bond_rest_length(bond_order23, at2, at3);
    let r13 = (r12 * r12 + r23 * r23 - 2.0 * r12 * r23 * cos_theta0).sqrt();

    let beta = 2.0 * G / (r12 * r23);
    let pre_factor = beta * at1.z1 * at3.z1 / (r13 * r13 * r13 * r13 * r13);
    let r_term = r12 * r23;
    let inner_bit = 3.0 * r_term * (1.0 - cos_theta0 * cos_theta0) - r13 * r13 * cos_theta0;
    pre_factor * r_term * inner_bit
}

/// Returns true if the atomic number belongs to group 6 (chalcogens: O, S, Se, Te, Po).
///
/// Used for special torsion parameter handling per the UFF paper.
pub fn is_in_group6(atomic_number: i32) -> bool {
    matches!(atomic_number, 8 | 16 | 34 | 52 | 84)
}

/// Hybridization of an atom, for torsion parameter calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hybridization {
    SP2,
    SP3,
}

/// Pre-computed torsion parameters: force constant V, periodicity n, and cos(n*phi0) term.
#[derive(Debug, Clone, Copy)]
pub struct TorsionParams {
    /// V/2 prefactor in kcal/mol.
    pub force_constant: f64,
    /// Periodicity (2, 3, or 6).
    pub order: u32,
    /// cos(n*phi0) term: +1 or -1.
    pub cos_term: f64,
}

/// UFF equation 17: V = 5 * sqrt(U2 * U3) * (1 + 4.18 * ln(bond_order23)).
///
/// Used for sp2-sp2 torsions and for sp3(group6)-sp2(non-group6) special case.
fn equation17(bond_order23: f64, at2: &UffAtomParams, at3: &UffAtomParams) -> f64 {
    5.0 * (at2.u1 * at3.u1).sqrt() * (1.0 + 4.18 * bond_order23.ln())
}

/// Calculates the cos(n*phi) using Chebyshev polynomial expansion.
///
/// Avoids computing acos/cos round-trips by working directly with cos(phi).
pub fn cos_n_phi(cos_phi: f64, sin_phi_sq: f64, n: u32) -> f64 {
    match n {
        2 => {
            // cos(2x) = 2*cos^2(x) - 1 = 1 - 2*sin^2(x)
            1.0 - 2.0 * sin_phi_sq
        }
        3 => {
            // cos(3x) = 4*cos^3(x) - 3*cos(x) = cos^3(x) - 3*cos(x)*sin^2(x)
            cos_phi * (cos_phi * cos_phi - 3.0 * sin_phi_sq)
        }
        6 => {
            // cos(6x) = 1 - 32*sin^6(x) + 48*sin^4(x) - 18*sin^2(x)
            1.0 + sin_phi_sq * (-32.0 * sin_phi_sq * sin_phi_sq + 48.0 * sin_phi_sq - 18.0)
        }
        _ => 1.0,
    }
}

/// Calculates sin(n*phi) from cos(phi), sin(phi), sin^2(phi).
///
/// Used for torsion gradient calculation.
pub fn sin_n_phi(cos_phi: f64, sin_phi: f64, sin_phi_sq: f64, n: u32) -> f64 {
    match n {
        2 => {
            // sin(2x) = 2*sin(x)*cos(x)
            2.0 * sin_phi * cos_phi
        }
        3 => {
            // sin(3x) = 3*sin(x) - 4*sin^3(x)
            sin_phi * (3.0 - 4.0 * sin_phi_sq)
        }
        6 => {
            // sin(6x) = cos(x)*[32*sin^5(x) - 32*sin^3(x) + 6*sin(x)]
            cos_phi * sin_phi * (32.0 * sin_phi_sq * (sin_phi_sq - 1.0) + 6.0)
        }
        _ => 0.0,
    }
}

/// Calculates inversion (out-of-plane) coefficients and force constant.
///
/// For sp2 centers with 3 neighbors, the inversion term penalizes deviation from
/// planarity. Returns `(force_constant, c0, c1, c2)` for the energy formula:
///   E = force_constant * (C0 + C1*cos(omega) + C2*cos(2*omega))
/// where omega is the Wilson angle (angle between a bond and the plane of the
/// other two bonds).
///
/// The force constant is pre-divided by 3 (for the 3 permutations of peripheral atoms).
///
/// Ported from RDKit's Utils::calcInversionCoefficientsAndForceConstant() (BSD-3-Clause).
///
/// # Arguments
/// * `at2_atomic_num` - Atomic number of the central atom (sp2 center)
/// * `is_c_bound_to_o` - True if the central atom is sp2 carbon bonded to sp2 oxygen
pub fn calc_inversion_coefficients_and_force_constant(
    at2_atomic_num: i32,
    is_c_bound_to_o: bool,
) -> (f64, f64, f64, f64) {
    let (mut k, c0, c1, c2): (f64, f64, f64, f64);

    if matches!(at2_atomic_num, 6..=8) {
        // sp2 carbon, nitrogen, or oxygen
        c0 = 1.0;
        c1 = -1.0;
        c2 = 0.0;
        k = if is_c_bound_to_o { 50.0 } else { 6.0 };
    } else {
        // Group 15 elements (P, As, Sb, Bi) — not clearly explained in UFF paper;
        // logic from MCCCS Towhee's ffuff.F, via RDKit.
        let w0_deg: f64 = match at2_atomic_num {
            15 => 84.4339, // phosphorus
            33 => 86.9735, // arsenic
            51 => 87.7047, // antimony
            83 => 90.0,    // bismuth
            _ => 0.0,
        };
        let w0 = w0_deg.to_radians();
        c2 = 1.0;
        c1 = -4.0 * w0.cos();
        c0 = -(c1 * w0.cos() + c2 * (2.0 * w0).cos());
        k = 22.0 / (c0 + c1 + c2);
    }

    k /= 3.0;
    (k, c0, c1, c2)
}

/// Compute vdW characteristic distance for a pair (geometric mean).
///
/// x_ij = sqrt(x_i * x_j) where x_i = UffAtomParams.x1
pub fn calc_vdw_distance(params_i: &UffAtomParams, params_j: &UffAtomParams) -> f64 {
    (params_i.x1 * params_j.x1).sqrt()
}

/// Compute vdW well depth for a pair (geometric mean).
///
/// D_ij = sqrt(D_i * D_j) where D_i = UffAtomParams.d1
pub fn calc_vdw_well_depth(params_i: &UffAtomParams, params_j: &UffAtomParams) -> f64 {
    (params_i.d1 * params_j.d1).sqrt()
}

/// Calculates UFF torsion parameters for the central bond between atoms 2 and 3.
///
/// Determines the force constant V, periodicity n, and cos(n*phi0) based on:
/// - Hybridization of atoms 2 and 3 (the central bond atoms)
/// - Atomic numbers of atoms 2 and 3 (for group 6 special cases)
/// - Bond order of the central bond
/// - Whether either end atom (atom 1 or atom 4) is sp2
///
/// Ported from RDKit's `TorsionAngleContrib::calcTorsionParams()` (BSD-3-Clause).
///
/// # Arguments
/// * `bond_order23` - Bond order of the central bond (between atoms 2 and 3)
/// * `at_num2` - Atomic number of atom 2
/// * `at_num3` - Atomic number of atom 3
/// * `hyb2` - Hybridization of atom 2
/// * `hyb3` - Hybridization of atom 3
/// * `at2_params` - UFF parameters for atom 2
/// * `at3_params` - UFF parameters for atom 3
/// * `end_atom_is_sp2` - True if either end atom (1 or 4) is sp2 hybridized
#[allow(clippy::too_many_arguments)]
pub fn calc_torsion_params(
    bond_order23: f64,
    at_num2: i32,
    at_num3: i32,
    hyb2: Hybridization,
    hyb3: Hybridization,
    at2_params: &UffAtomParams,
    at3_params: &UffAtomParams,
    end_atom_is_sp2: bool,
) -> TorsionParams {
    match (hyb2, hyb3) {
        (Hybridization::SP3, Hybridization::SP3) => {
            // General sp3-sp3 case
            let mut force_constant = (at2_params.v1 * at3_params.v1).sqrt();
            let mut order = 3;
            let mut cos_term = -1.0; // phi0 = 60 degrees

            // Special case for single bonds between group 6 elements
            if bond_order23 == 1.0 && is_in_group6(at_num2) && is_in_group6(at_num3) {
                let v2: f64 = if at_num2 == 8 { 2.0 } else { 6.8 };
                let v3: f64 = if at_num3 == 8 { 2.0 } else { 6.8 };
                force_constant = (v2 * v3).sqrt();
                order = 2;
                cos_term = -1.0; // phi0 = 90 degrees
            }

            TorsionParams {
                force_constant,
                order,
                cos_term,
            }
        }
        (Hybridization::SP2, Hybridization::SP2) => {
            // sp2-sp2: use equation 17
            let force_constant = equation17(bond_order23, at2_params, at3_params);
            TorsionParams {
                force_constant,
                order: 2,
                cos_term: 1.0, // phi0 = 180 degrees
            }
        }
        _ => {
            // sp2-sp3 or sp3-sp2 (mixed)
            let mut force_constant = 1.0;
            let mut order = 6;
            let mut cos_term = 1.0; // phi0 = 0 degrees

            if bond_order23 == 1.0 {
                // Special case: group 6 sp3 with non-group 6 sp2
                let group6_sp3_with_non_group6_sp2 =
                    (hyb2 == Hybridization::SP3 && is_in_group6(at_num2) && !is_in_group6(at_num3))
                        || (hyb3 == Hybridization::SP3
                            && is_in_group6(at_num3)
                            && !is_in_group6(at_num2));

                if group6_sp3_with_non_group6_sp2 {
                    force_constant = equation17(bond_order23, at2_params, at3_params);
                    order = 2;
                    cos_term = -1.0; // phi0 = 90 degrees
                } else if end_atom_is_sp2 {
                    // Special case: sp3 - sp2 - sp2 (propene-like)
                    force_constant = 2.0;
                    order = 3;
                    cos_term = -1.0; // phi0 = 180 degrees
                }
            }

            TorsionParams {
                force_constant,
                order,
                cos_term,
            }
        }
    }
}
