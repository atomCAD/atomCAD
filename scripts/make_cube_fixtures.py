#!/usr/bin/env python3
"""Generate the `.cube` fixtures and sample files used by the scalar-field work.

Three subcommands, deliberately separated by what they are *for*:

    tests    rust/tests/fixtures/cube/   tiny, COMMITTED, asserted against by
                                         literal value in the Rust test suite
    manual   sample_data/cube/           gitignored eyeball files for the P3/P4
                                         manual walkthroughs; regenerate on demand
    pyscf    rust/tests/fixtures/cube/   one committed realism fixture from a real
                                         producer; needs PySCF (see the design doc)

Only `numpy` is required for `tests` and `manual`. `pyscf` is optional and
nothing else depends on it.

The committed fixtures are the hand-checkable artifact: the tests assert against
values a reviewer can verify by eye, never against whatever this script happened
to emit. Design doc: `doc/design_scalar_fields.md`.
"""

from __future__ import annotations

import argparse
import math
import shutil
from pathlib import Path

import numpy as np

# CODATA 2018. Must match `BOHR_TO_ANGSTROM` in
# rust/crates/atomcad-crystolecule/src/io/cube_loader.rs.
BOHR_TO_ANGSTROM = 0.529177210903
ANGSTROM_TO_BOHR = 1.0 / BOHR_TO_ANGSTROM

REPO_ROOT = Path(__file__).resolve().parent.parent
TEST_FIXTURE_DIR = REPO_ROOT / "rust" / "tests" / "fixtures" / "cube"
SAMPLE_DATA_DIR = REPO_ROOT / "sample_data" / "cube"


def display_path(path: Path) -> str:
    """Repo-relative when it can be, absolute otherwise.

    `--out` accepts any directory, including a relative one and one outside the
    repo, so `Path.relative_to` is not safe here -- it raises on both.
    """
    try:
        return str(path.resolve().relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def write_cube(
    path: Path,
    comment1: str,
    comment2: str,
    origin_bohr,
    axes_bohr,
    dims,
    atoms,
    values,
) -> None:
    """Write one single-field cube file.

    `atoms` is a list of `(Z, x, y, z)` in **Bohr**. `values` is indexed
    `[i, j, k]` and written x-slowest / z-fastest, six per line with a break at
    the end of each innermost run — the layout real cube writers produce.
    """
    nx, ny, nz = dims
    assert values.shape == (nx, ny, nz), (values.shape, dims)

    lines = [comment1, comment2]
    lines.append(
        "%5d%12.6f%12.6f%12.6f" % (len(atoms), origin_bohr[0], origin_bohr[1], origin_bohr[2])
    )
    for n, axis in zip(dims, axes_bohr):
        lines.append("%5d%12.6f%12.6f%12.6f" % (n, axis[0], axis[1], axis[2]))
    for z, x, y, zz in atoms:
        lines.append("%5d%12.6f%12.6f%12.6f%12.6f" % (z, float(z), x, y, zz))

    for i in range(nx):
        for j in range(ny):
            row = []
            for k in range(nz):
                row.append("%13.5E" % values[i, j, k])
                if len(row) == 6:
                    lines.append("".join(row))
                    row = []
            if row:
                lines.append("".join(row))

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="ascii")
    print("wrote", display_path(path))


def grid_positions_angstrom(origin_a, spacing_a, dims):
    """Real-space Ångström position of every sample, shaped (nx, ny, nz, 3)."""
    nx, ny, nz = dims
    xs = origin_a[0] + spacing_a[0] * np.arange(nx)
    ys = origin_a[1] + spacing_a[1] * np.arange(ny)
    zs = origin_a[2] + spacing_a[2] * np.arange(nz)
    gx, gy, gz = np.meshgrid(xs, ys, zs, indexing="ij")
    return np.stack([gx, gy, gz], axis=-1)


# --- geometry shared by several fixtures -----------------------------------

# Real water: O-H 0.958 A, H-O-H 104.5 degrees, in the xz plane.
_HALF_ANGLE = math.radians(104.5 / 2.0)
WATER_ANGSTROM = [
    (8, 0.0, 0.0, 0.0),
    (1, 0.958 * math.sin(_HALF_ANGLE), 0.0, 0.958 * math.cos(_HALF_ANGLE)),
    (1, -0.958 * math.sin(_HALF_ANGLE), 0.0, 0.958 * math.cos(_HALF_ANGLE)),
]


def to_bohr(atoms_angstrom):
    return [
        (z, x * ANGSTROM_TO_BOHR, y * ANGSTROM_TO_BOHR, zz * ANGSTROM_TO_BOHR)
        for z, x, y, zz in atoms_angstrom
    ]


def s_blob(points, atoms_angstrom, decay=1.5):
    """A crude non-negative envelope: one decaying exponential per nucleus."""
    total = np.zeros(points.shape[:-1])
    for _z, x, y, zz in atoms_angstrom:
        d = np.linalg.norm(points - np.array([x, y, zz]), axis=-1)
        total += np.exp(-decay * d)
    return total


def p2z(points, alpha=0.25, center=(0.0, 0.0, 0.0)):
    """A crude analytic 2p_z: `z * exp(-alpha * r^2)`, centred on `center`.

    Signed, with a nodal plane at the centre's z. The Rust gradient test
    differentiates this by hand, so the formula and `alpha` must stay in step
    with `p2z_analytic_gradient` in `cube_loader_test.rs`. The 0.4 A spacing is
    what sets that test's tolerance: central differences at this step land
    within ~4% of the peak gradient magnitude.
    """
    rel = points - np.array(center)
    r2 = np.sum(rel * rel, axis=-1)
    return rel[..., 2] * np.exp(-alpha * r2)


def elf_like(points, atoms_angstrom, sigma=0.8):
    """A caricature of a *bounded analysis field* — ELF, RDG and their kin.

    `0.5 + 0.5 * max_i exp(-(r_i / sigma)^2)`: non-negative, bounded to
    [0.5, 1.0], and — the point — nowhere near zero in the vacuum, because a
    real ELF sits around 0.5 wherever the density looks like a uniform electron
    gas, empty space included.

    This is what the `isosurface` Auto rule's plausibility window exists to
    reject. It is *not* a density, so the conventional 0.002 isolevel is
    meaningless on it and would enclose the entire box. Its
    `iso_for_fraction(0.72)` is 0.5000 — a ratio of 250 against `DENSITY_LEVEL`,
    against 9.2 for the promolecular density below and 21-65 for the real cubes
    in the zoo. See `doc/design_isosurface_level.md` §The plausibility window.

    Not a real ELF, and nothing pretends otherwise: what it reproduces is the
    one property the window keys on — a field whose own localized scale is
    orders of magnitude above 0.002.
    """
    peak = np.zeros(points.shape[:-1])
    for _z, x, y, zz in atoms_angstrom:
        d = np.linalg.norm(points - np.array([x, y, zz]), axis=-1)
        peak = np.maximum(peak, np.exp(-(d * d) / (sigma * sigma)))
    return 0.5 + 0.5 * peak


# --- the colour-map pair: density and electrostatic potential ---------------

# van der Waals radii in Angstrom (Bondi 1964), used only to calibrate the
# density amplitudes below.
VDW_ANGSTROM = {1: 1.20, 6: 1.70, 7: 1.55, 8: 1.52}

# The isolevel the amplitudes are calibrated against, and the level the P5
# walkthrough enters into the node. 0.002 e/bohr^3 is the conventional choice
# for a molecular "density envelope".
DENSITY_REFERENCE_LEVEL = 0.002

# A single decay constant for every element. The true asymptotic decay of an
# electron density is exp(-2*sqrt(2*I)*r) with I the ionisation potential in
# hartree; I is close to 0.5 Ha across the light main-group elements, so
# zeta = 1.0 (in inverse Bohr) is a defensible common value.
DENSITY_ZETA = 1.0


def promolecular_density(points, atoms_angstrom):
    """A crude promolecular density: one decaying exponential per nucleus.

    `rho(r) = sum_i A_i * exp(-2 * zeta * r_i)`, with `r_i` in Bohr, so the
    result carries the same e/bohr^3 units a real cube writer emits. Each `A_i`
    is calibrated so the `DENSITY_REFERENCE_LEVEL` contour of an *isolated* atom
    lands exactly on that element's van der Waals radius -- which is what makes
    the conventional 0.002 isolevel produce the expected envelope here.

    Valence-only and monotonic: there is no core cusp, so values near a nucleus
    are far too small to be a real density. Nothing downstream cares. The
    envelope is the point, and everything this fixture feeds -- the isosurface,
    the ESP painted on it -- lives well outside the core.
    """
    total = np.zeros(points.shape[:-1])
    for z, x, y, zz in atoms_angstrom:
        if z not in VDW_ANGSTROM:
            raise KeyError("no van der Waals radius for Z=%d; add one to VDW_ANGSTROM" % z)
        amplitude = DENSITY_REFERENCE_LEVEL * math.exp(
            2.0 * DENSITY_ZETA * VDW_ANGSTROM[z] * ANGSTROM_TO_BOHR
        )
        d_bohr = np.linalg.norm(points - np.array([x, y, zz]), axis=-1) * ANGSTROM_TO_BOHR
        total += amplitude * np.exp(-2.0 * DENSITY_ZETA * d_bohr)
    return total


# TIP3P partial charges for water, in units of e. They sum to zero and
# reproduce the molecule's dipole closely enough for a picture.
TIP3P_WATER_CHARGES = [-0.834, 0.417, 0.417]

_erf = np.vectorize(math.erf)


def point_charge_esp(points, atoms_angstrom, charges, sigma_bohr=0.5):
    """Electrostatic potential of a set of Gaussian-smeared point charges.

    `V(r) = sum_i q_i * erf(r_i / sigma) / r_i`, with `r_i` in Bohr, giving
    hartree/e -- the units a real `cubegen.mep` file carries.

    The smearing is what keeps the file finite. A bare point charge diverges at
    its own nucleus, and the nuclei here sit *on* grid points, so an undamped
    formula would write `inf` and poison `value_range()`. `erf(r/sigma)/r`
    agrees with `1/r` to better than one part in 10^5 beyond `3*sigma`
    (1.5 Bohr at the default), which is far inside any density envelope, so the
    damping is invisible everywhere the surface actually samples it.

    A point-charge model is not a real ESP. What it reproduces faithfully is the
    *shape* on the envelope -- negative over the lone-pair side, positive over
    the hydrogens -- and a magnitude in the right decade (roughly +/-0.08 Ha/e
    for water), which is all the colour map needs.
    """
    assert len(charges) == len(atoms_angstrom), (len(charges), len(atoms_angstrom))
    total = np.zeros(points.shape[:-1])
    for (_z, x, y, zz), q in zip(atoms_angstrom, charges):
        d_bohr = np.linalg.norm(points - np.array([x, y, zz]), axis=-1) * ANGSTROM_TO_BOHR
        d_bohr = np.maximum(d_bohr, 1e-12)
        total += q * _erf(d_bohr / sigma_bohr) / d_bohr
    return total


# --- committed test fixtures ------------------------------------------------


def make_test_fixtures(out_dir: Path) -> None:
    # 1. The ramp. THE most important fixture in the plan: three *different*
    #    dimensions and a value that encodes its own index, so any axis
    #    transposition or mirroring shows up immediately. Spacing is chosen so
    #    the sample positions are whole Ångström, which keeps the P4 manual
    #    walkthrough checkable in your head.
    dims = (3, 4, 5)
    values = np.zeros(dims)
    for i in range(dims[0]):
        for j in range(dims[1]):
            for k in range(dims[2]):
                values[i, j, k] = 100 * i + 10 * j + k
    step = ANGSTROM_TO_BOHR  # 1.0 A per grid step
    write_cube(
        out_dir / "ramp_3x4x5.cube",
        " Asymmetric ramp fixture: value(i,j,k) = 100i + 10j + k",
        " 1.0 Angstrom spacing, origin at the first sample, one carbon atom",
        (0.0, 0.0, 0.0),
        [(step, 0.0, 0.0), (0.0, step, 0.0), (0.0, 0.0, step)],
        dims,
        [(6, 0.0, 0.0, 0.0)],
        values,
    )

    # 2. A synthetic 2p_z, for the sign-across-the-nodal-plane test and the
    #    gradient test. Signed values, one oxygen at the centre.
    dims = (11, 11, 11)
    origin_a = (-2.0, -2.0, -2.0)
    spacing_a = (0.4, 0.4, 0.4)
    points = grid_positions_angstrom(origin_a, spacing_a, dims)
    write_cube(
        out_dir / "p2z_11x11x11.cube",
        " Synthetic 2p_z on oxygen: z * exp(-0.25 * r^2), r in Angstrom",
        " 0.4 Angstrom spacing, nodal plane on the k = 5 grid plane (z = 0)",
        tuple(c * ANGSTROM_TO_BOHR for c in origin_a),
        [
            (spacing_a[0] * ANGSTROM_TO_BOHR, 0.0, 0.0),
            (0.0, spacing_a[1] * ANGSTROM_TO_BOHR, 0.0),
            (0.0, 0.0, spacing_a[2] * ANGSTROM_TO_BOHR),
        ],
        dims,
        [(8, 0.0, 0.0, 0.0)],
        p2z(points),
    )

    # 3. Water in Bohr — the "reads as chemically sane" case. A matching .xyz
    #    is the independent reference the atom-block test compares against.
    dims = (5, 5, 5)
    origin_a = (-1.6, -1.6, -1.6)
    spacing_a = (0.8, 0.8, 0.8)
    points = grid_positions_angstrom(origin_a, spacing_a, dims)
    density = s_blob(points, WATER_ANGSTROM)
    write_cube(
        out_dir / "water_bohr.cube",
        " Water, coordinates in Bohr (the normal case)",
        " Crude non-negative envelope, 0.8 Angstrom spacing",
        tuple(c * ANGSTROM_TO_BOHR for c in origin_a),
        [
            (spacing_a[0] * ANGSTROM_TO_BOHR, 0.0, 0.0),
            (0.0, spacing_a[1] * ANGSTROM_TO_BOHR, 0.0),
            (0.0, 0.0, spacing_a[2] * ANGSTROM_TO_BOHR),
        ],
        dims,
        to_bohr(WATER_ANGSTROM),
        density,
    )
    write_xyz(out_dir / "water_reference.xyz", WATER_ANGSTROM, "Water reference geometry, Angstrom")

    # 4. The same water, but with Angstrom numbers in the coordinate columns.
    #    Read as Bohr (which the loader always does) it comes out 1.89x too
    #    small, so the plausibility check must warn — and the coordinates must
    #    still NOT be rescaled.
    write_cube(
        out_dir / "water_angstrom.cube",
        " Water, coordinates mistakenly written in Angstrom",
        " Read as Bohr this is 1.89x too small; the loader must warn, not rescale",
        origin_a,
        [(spacing_a[0], 0.0, 0.0), (0.0, spacing_a[1], 0.0), (0.0, 0.0, spacing_a[2])],
        dims,
        WATER_ANGSTROM,
        density,
    )

    # 5. Two carbon atoms 20 Bohr apart: the high-side trip of the same check.
    dims = (2, 2, 2)
    write_cube(
        out_dir / "two_fragments.cube",
        " Two carbon atoms 20 Bohr apart: separated fragments",
        " Trips the HIGH side of the units plausibility check; positions stay as read",
        (0.0, 0.0, 0.0),
        [(10.0, 0.0, 0.0), (0.0, 10.0, 0.0), (0.0, 0.0, 10.0)],
        dims,
        [(6, 0.0, 0.0, 0.0), (6, 20.0, 0.0, 0.0)],
        np.zeros(dims),
    )

    # --- isolevel selection: doc/design_isosurface_level.md -----------------
    #
    # One grid for all three, so the density and its ESP are a matched pair:
    # 0.3 A spacing, 17 x 15 x 19 = 4845 samples, ~66 KB each.
    #
    # Three DIFFERENT dimensions, per doc/testing.md: a cubic grid hides axis
    # transposition. It cannot actually bite here — a value distribution is
    # invariant under any permutation of its samples, and the density/ESP pair
    # would transpose together — but the rule costs nothing to keep and the
    # next fixture added beside these may not be so forgiving.
    #
    # Coarse enough to stay a *tiny* committed fixture; fine enough that the
    # 0.002 envelope is resolved and the ESP percentile on it has converged.
    # Measured against the same analytic functions at 0.1 A spacing, the
    # surface p98 moves from 0.0909 here to 0.0899 there — 1%, while the file
    # would grow 25x.
    #
    # `ValueDistribution` itself is tested on fields built in code with
    # `SampledField::new`, the way `field_test.rs` already does. These three
    # exist because the *Auto* rule and the colour fit have to run end to end
    # through the loader at least once, on something shaped like real data.
    lvl_dims = (17, 15, 19)
    lvl_origin_a = (-2.4, -2.1, -2.7)
    lvl_spacing_a = (0.3, 0.3, 0.3)
    lvl_points = grid_positions_angstrom(lvl_origin_a, lvl_spacing_a, lvl_dims)
    lvl_origin_bohr = tuple(c * ANGSTROM_TO_BOHR for c in lvl_origin_a)
    lvl_axes_bohr = [
        (lvl_spacing_a[0] * ANGSTROM_TO_BOHR, 0.0, 0.0),
        (0.0, lvl_spacing_a[1] * ANGSTROM_TO_BOHR, 0.0),
        (0.0, 0.0, lvl_spacing_a[2] * ANGSTROM_TO_BOHR),
    ]
    lvl_atoms_bohr = to_bohr(WATER_ANGSTROM)

    # 6. A non-negative density the plausibility window ACCEPTS. Auto must
    #    resolve it to DENSITY_LEVEL (0.002) with basis "density-like":
    #    value_range = [1.0876e-07, 6.3508e-01], iso_for_fraction(0.72) =
    #    1.8393e-02, so the ratio against 0.002 is 9.2 — well under
    #    MAX_LEVEL_RATIO = 125. Also the `field` pin of the P4 colour-fit test.
    #
    #    The ratio is lower than the 21-65 the real zoo densities give, because
    #    `promolecular_density` is valence-only and has no core cusp. That is
    #    fine for what this fixture is for — it pins the *branch*, not the
    #    calibration; the constants are calibrated against the zoo, in the doc.
    write_cube(
        out_dir / "water_density_17x15x19.cube",
        " Crude promolecular electron density of water, e/bohr^3, coords in Bohr",
        " Auto isolevel: non-negative, ratio 9.2 -> the density branch, level 0.002",
        lvl_origin_bohr,
        lvl_axes_bohr,
        lvl_dims,
        lvl_atoms_bohr,
        promolecular_density(lvl_points, WATER_ANGSTROM),
    )

    # 7. The ESP partner on the SAME grid: the `color_field` pin. Signed, and
    #    the whole point of the P4 test — its value_range is [-1.4215, 0.5819]
    #    while on the 0.002 envelope it runs p2 = -0.0909 to p98 = +0.0656.
    #    Fitting the colour domain to the volume would be 15.6x too wide and
    #    paint the envelope one flat colour.
    write_cube(
        out_dir / "water_esp_17x15x19.cube",
        " Electrostatic potential of water from TIP3P point charges, hartree/e",
        " Colour fit: volume range +/-1.42, but only +/-0.09 on the 0.002 envelope",
        lvl_origin_bohr,
        lvl_axes_bohr,
        lvl_dims,
        lvl_atoms_bohr,
        point_charge_esp(lvl_points, WATER_ANGSTROM, TIP3P_WATER_CHARGES),
    )

    # 8. The impostor the window REJECTS. Non-negative like a density, but
    #    iso_for_fraction(0.72) = 0.5000 — ratio 250, over MAX_LEVEL_RATIO — so
    #    Auto must fall back to the fraction and report basis "atypical", not
    #    take 0.002 and swallow the box.
    write_cube(
        out_dir / "elf_like_17x15x19.cube",
        " Caricature of a bounded analysis field (ELF-like), dimensionless 0.5-1.0",
        " Auto isolevel: non-negative but ratio 250 -> window rejects -> 0.5000",
        lvl_origin_bohr,
        lvl_axes_bohr,
        lvl_dims,
        lvl_atoms_bohr,
        elf_like(lvl_points, WATER_ANGSTROM),
    )


def write_xyz(path: Path, atoms_angstrom, comment: str) -> None:
    symbols = {1: "H", 6: "C", 8: "O"}
    lines = [str(len(atoms_angstrom)), comment]
    for z, x, y, zz in atoms_angstrom:
        lines.append("%-3s %14.8f %14.8f %14.8f" % (symbols[z], x, y, zz))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="ascii")
    print("wrote", display_path(path))


# --- gitignored eyeball files ----------------------------------------------


def make_manual_files(out_dir: Path) -> None:
    """The files the P3, P4 and P5 manual walkthroughs load.

    Coarser and larger than the committed fixtures — these are for looking at,
    not for asserting against.
    """
    dims = (25, 25, 25)
    origin_a = (-3.0, -3.0, -3.0)
    spacing_a = (0.25, 0.25, 0.25)
    points = grid_positions_angstrom(origin_a, spacing_a, dims)
    values = p2z(points, alpha=0.6)
    axes_bohr = [
        (spacing_a[0] * ANGSTROM_TO_BOHR, 0.0, 0.0),
        (0.0, spacing_a[1] * ANGSTROM_TO_BOHR, 0.0),
        (0.0, 0.0, spacing_a[2] * ANGSTROM_TO_BOHR),
    ]
    write_cube(
        out_dir / "water.cube",
        " Water with a crude 2p_z on the oxygen; coordinates in Bohr",
        " Manual walkthrough: expect O-H 0.96 A and H-O-H 104.5 degrees, bonded",
        tuple(c * ANGSTROM_TO_BOHR for c in origin_a),
        axes_bohr,
        dims,
        to_bohr(WATER_ANGSTROM),
        values,
    )
    write_cube(
        out_dir / "water_angstrom.cube",
        " The same water written in Angstrom: the units_warning path",
        " Manual walkthrough: expect an amber warning and an UNCHANGED geometry",
        origin_a,
        [(spacing_a[0], 0.0, 0.0), (0.0, spacing_a[1], 0.0), (0.0, 0.0, spacing_a[2])],
        dims,
        WATER_ANGSTROM,
        values,
    )
    ramp = TEST_FIXTURE_DIR / "ramp_3x4x5.cube"
    if ramp.exists():
        out_dir.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ramp, out_dir / "ramp_3x4x5.cube")
        print("copied", display_path(out_dir / "ramp_3x4x5.cube"))
    else:
        print("skipped ramp_3x4x5.cube — run the `tests` subcommand first")

    make_colour_map_files(out_dir)


def make_colour_map_files(out_dir: Path) -> None:
    """The matched density / electrostatic-potential pair the P5 walkthrough loads.

    Three files on two grids:

    * `water_density.cube` -- the `field` pin. Non-negative, so the isosurface
      has one component; the conventional 0.002 isolevel gives the van der Waals
      envelope.
    * `water_esp.cube` -- the `color_field` pin, on the **same** grid. Signed.
      On the 0.002 envelope it runs from -0.092 Ha/e over the lone pairs to
      +0.061 Ha/e over the hydrogens, so a colour range of +/-0.08 shows the
      whole ramp and the conventional +/-0.05 saturates both ends the way a
      published ESP map does. Its `value_range()` is far wider (about -1.4 Ha/e)
      because the potential still climbs inside the core, which no surface ever
      samples -- exactly why the design does not auto-fit the colour domain.
    * `water_esp_small.cube` -- the same potential on a box half the size, for
      the mismatched-bounds step. The 0.002 envelope reaches past +/-1.5 A along
      each O-H, so the overhang has no colour field to sample and must render as
      the neutral midpoint of the map: the white band the reference guide
      documents and the node deliberately does not report.

    A finer grid than the P3/P4 files above (0.2 A rather than 0.25 A) because
    these are the first sample files that get *tessellated* -- marching cubes
    exposes grid coarseness that point sampling never did.
    """
    spacing_a = (0.2, 0.2, 0.2)
    axes_bohr = [
        (spacing_a[0] * ANGSTROM_TO_BOHR, 0.0, 0.0),
        (0.0, spacing_a[1] * ANGSTROM_TO_BOHR, 0.0),
        (0.0, 0.0, spacing_a[2] * ANGSTROM_TO_BOHR),
    ]
    atoms_bohr = to_bohr(WATER_ANGSTROM)

    # The full box: -3.0 A to +3.0 A on every axis.
    origin_a = (-3.0, -3.0, -3.0)
    dims = (31, 31, 31)
    points = grid_positions_angstrom(origin_a, spacing_a, dims)
    origin_bohr = tuple(c * ANGSTROM_TO_BOHR for c in origin_a)

    write_cube(
        out_dir / "water_density.cube",
        " Crude promolecular electron density of water, e/bohr^3, coords in Bohr",
        " P5 walkthrough: isosurface level 0.002 gives the vdW envelope",
        origin_bohr,
        axes_bohr,
        dims,
        atoms_bohr,
        promolecular_density(points, WATER_ANGSTROM),
    )
    write_cube(
        out_dir / "water_esp.cube",
        " Electrostatic potential of water from TIP3P point charges, hartree/e",
        " P5 walkthrough: color_field for water_density.cube; range +/-0.08",
        origin_bohr,
        axes_bohr,
        dims,
        atoms_bohr,
        point_charge_esp(points, WATER_ANGSTROM, TIP3P_WATER_CHARGES),
    )

    # The deliberately undersized box: -1.5 A to +1.5 A, same spacing and origin
    # parity, so the mismatch is in extent only.
    small_origin_a = (-1.5, -1.5, -1.5)
    small_dims = (16, 16, 16)
    small_points = grid_positions_angstrom(small_origin_a, spacing_a, small_dims)
    write_cube(
        out_dir / "water_esp_small.cube",
        " The same potential on a box half the size: the mismatched-bounds case",
        " P5 walkthrough: expect a white band where the surface leaves this box",
        tuple(c * ANGSTROM_TO_BOHR for c in small_origin_a),
        axes_bohr,
        small_dims,
        atoms_bohr,
        point_charge_esp(small_points, WATER_ANGSTROM, TIP3P_WATER_CHARGES),
    )


# --- optional realism fixture ----------------------------------------------


def make_pyscf_fixture(out_dir: Path) -> None:
    """One low-resolution water HOMO from a real producer.

    Nothing in P1-P4 depends on this; it exists so the token-stream parser meets
    a real writer's whitespace, wrapping and `%13.5E` formatting at least once.
    PySCF publishes no Windows wheels — run this inside WSL (see the design doc).
    """
    try:
        from pyscf import gto, scf
        from pyscf.tools import cubegen
    except ImportError as exc:  # pragma: no cover - environment dependent
        raise SystemExit(
            "PySCF is not importable: %s\n"
            "It is optional; every test and walkthrough in the plan works without it.\n"
            "To install (inside WSL):\n"
            "  sudo apt update && sudo apt install -y python3-venv\n"
            "  python3 -m venv ~/pyscf && ~/pyscf/bin/pip install pyscf" % exc
        )

    atom_spec = "; ".join(
        "%s %.8f %.8f %.8f" % ({1: "H", 8: "O"}[z], x, y, zz) for z, x, y, zz in WATER_ANGSTROM
    )
    mol = gto.M(atom=atom_spec, basis="sto-3g", unit="Angstrom")
    mf = scf.RHF(mol).run()
    homo_index = mol.nelectron // 2 - 1
    out_dir.mkdir(parents=True, exist_ok=True)
    target = out_dir / "water_homo.cube"
    cubegen.orbital(mol, str(target), mf.mo_coeff[:, homo_index], nx=20, ny=20, nz=20)
    print("wrote", display_path(target))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    p_tests = sub.add_parser("tests", help="write the committed test fixtures")
    p_tests.add_argument("--out", type=Path, default=TEST_FIXTURE_DIR)

    p_manual = sub.add_parser("manual", help="write the gitignored walkthrough files")
    p_manual.add_argument("--out", type=Path, default=SAMPLE_DATA_DIR)

    p_pyscf = sub.add_parser("pyscf", help="write the optional PySCF realism fixture")
    p_pyscf.add_argument("--out", type=Path, default=TEST_FIXTURE_DIR)

    args = parser.parse_args()
    if args.command == "tests":
        make_test_fixtures(args.out)
    elif args.command == "manual":
        make_manual_files(args.out)
    elif args.command == "pyscf":
        make_pyscf_fixture(args.out)


if __name__ == "__main__":
    main()
