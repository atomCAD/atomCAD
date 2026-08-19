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
    print("wrote", path.relative_to(REPO_ROOT))


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


def write_xyz(path: Path, atoms_angstrom, comment: str) -> None:
    symbols = {1: "H", 6: "C", 8: "O"}
    lines = [str(len(atoms_angstrom)), comment]
    for z, x, y, zz in atoms_angstrom:
        lines.append("%-3s %14.8f %14.8f %14.8f" % (symbols[z], x, y, zz))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="ascii")
    print("wrote", path.relative_to(REPO_ROOT))


# --- gitignored eyeball files ----------------------------------------------


def make_manual_files(out_dir: Path) -> None:
    """The files the P3 and P4 manual walkthroughs load.

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
        print("copied", (out_dir / "ramp_3x4x5.cube").relative_to(REPO_ROOT))
    else:
        print("skipped ramp_3x4x5.cube — run the `tests` subcommand first")


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
    print("wrote", target.relative_to(REPO_ROOT))


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
