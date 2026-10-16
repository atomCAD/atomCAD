//! Baked ambient occlusion, the computation (`doc/design_depth_perception.md`
//! D6, D7, D10, Phase 4).
//!
//! Fixtures write diamond positions straight into an `AtomicStructure` (no
//! bonds: exposure reads positions and radii only) and pass receiver lists
//! explicitly instead of relying on `in_crystal_depth`. The lattice constant
//! is 3.5 Å, so every position is a multiple of 7/8 and lattice translations
//! are exact in floating point. Random neighbourhoods use a fixed seed.

use atomcad_crystolecule::atomic_constants::ATOM_INFO;
use atomcad_crystolecule::atomic_structure::{AtomicStructure, DELETED_SITE_ATOMIC_NUMBER};
use atomcad_display::ambient_occlusion::{
    AtomExposure, DIRECTION_COUNT, FULL_PATH_FRACTION, FULLY_EXPOSED, R_MAX, atom_exposure,
    cap_mask, cap_mask_exact, directions, exposure_of_mask, merge_changes, occluder_snapshot,
    occlusion_changes, occlusion_changes_by_place, pack_exposure, unpack_exposure,
    update_atom_exposure,
};
use atomcad_display::atomic_tessellator::{ao_receivers, tessellate_atomic_structure_impostors};
use atomcad_display::preferences::{
    AtomicRenderingMethod, AtomicStructureVisualization, AtomicStructureVisualizationPreferences,
};
use atomcad_renderer::atom_impostor_mesh::AtomImpostorMesh;
use atomcad_renderer::bond_impostor_mesh::BondImpostorMesh;
use atomcad_renderer::transparent_impostor_mesh::TransparentImpostorMesh;
use glam::f64::DVec3;

// ============================================================================
// Fixtures
// ============================================================================

const CARBON: i16 = 6;
const A: f64 = 3.5;

/// The diamond sites of the cells `0..nx × 0..ny × 0..nz`, plus the faces'
/// closing sites, clipped to the box `[0, n·A]` on each axis.
fn diamond_sites(nx: i32, ny: i32, nz: i32) -> Vec<DVec3> {
    const BASIS: [[f64; 3]; 8] = [
        [0.0, 0.0, 0.0],
        [0.0, 0.5, 0.5],
        [0.5, 0.0, 0.5],
        [0.5, 0.5, 0.0],
        [0.25, 0.25, 0.25],
        [0.25, 0.75, 0.75],
        [0.75, 0.25, 0.75],
        [0.75, 0.75, 0.25],
    ];
    let max = DVec3::new(nx as f64, ny as f64, nz as f64) * A;
    let mut sites = Vec::new();
    for i in 0..=nx {
        for j in 0..=ny {
            for k in 0..=nz {
                for b in BASIS {
                    let p = (DVec3::new(i as f64, j as f64, k as f64) + DVec3::from(b)) * A;
                    if p.x <= max.x && p.y <= max.y && p.z <= max.z {
                        sites.push(p);
                    }
                }
            }
        }
    }
    sites
}

fn structure_of(sites: &[DVec3]) -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    for p in sites {
        structure.add_atom(CARBON, *p);
    }
    structure
}

/// A diamond block of `n³` cells.
fn block(n: i32) -> AtomicStructure {
    structure_of(&diamond_sites(n, n, n))
}

fn all_ids(structure: &AtomicStructure) -> Vec<u32> {
    let mut ids: Vec<u32> = structure.atom_ids().copied().collect();
    ids.sort_unstable();
    ids
}

/// The atom nearest to `p`.
fn atom_near(structure: &AtomicStructure, p: DVec3) -> u32 {
    structure
        .atoms_values()
        .min_by(|a, b| {
            a.position
                .distance_squared(p)
                .total_cmp(&b.position.distance_squared(p))
        })
        .unwrap()
        .id
}

fn exposure_of(values: &AtomExposure, id: u32) -> f32 {
    unpack_exposure(values.get(id).expect("a value")).0
}

/// A fixed-seed xorshift generator, uniform in [0, 1).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    fn unit_vector(&mut self) -> DVec3 {
        let z = 2.0 * self.next() - 1.0;
        let phi = std::f64::consts::TAU * self.next();
        let r = (1.0 - z * z).sqrt();
        DVec3::new(r * phi.cos(), r * phi.sin(), z)
    }
}

fn prefs() -> AtomicStructureVisualizationPreferences {
    AtomicStructureVisualizationPreferences {
        visualization: AtomicStructureVisualization::BallAndStick,
        rendering_method: AtomicRenderingMethod::Impostors,
        ball_and_stick_cull_depth: None,
        space_filling_cull_depth: None,
        scene_transparency_enabled: false,
        scene_alpha: 1.0,
        label_scale: 0.7,
        show_tool_envelopes: false,
        tool_envelope_color: [0, 0, 0],
        ambient_occlusion: false,
    }
}

// ============================================================================
// Exposure values (D6)
// ============================================================================

#[test]
fn an_isolated_atom_is_fully_exposed() {
    let structure = structure_of(&[DVec3::ZERO]);
    let (values, stats) = atom_exposure(&structure, &all_ids(&structure));
    assert_eq!(values.get(1), Some(FULLY_EXPOSED));
    assert_eq!(stats.receivers_computed, 1);
}

/// Each atom of a C–C pair at 1.54 Å is blocked by about the cap of one
/// covalent sphere, `(1 − cos θ)/2` with `sin θ = r/d`, within the 128
/// directions' quantisation.
#[test]
fn a_pair_blocks_one_cap_each() {
    let d = 1.54;
    let structure = structure_of(&[DVec3::ZERO, DVec3::new(d, 0.0, 0.0)]);
    let (values, _) = atom_exposure(&structure, &all_ids(&structure));
    let r = ATOM_INFO[&(CARBON as i32)].covalent_radius;
    let blocked = (1.0 - (r / d).asin().cos()) / 2.0;
    for id in [1, 2] {
        let e = exposure_of(&values, id) as f64;
        assert!(
            (1.0 - e - blocked).abs() < 0.03,
            "atom {id}: blocked {}, closed form {blocked}",
            1.0 - e
        );
    }
}

/// An interior atom is buried, and on a (100) face a terrace atom lies
/// between an adatom placed on it and an atom at the bottom of a trench cut
/// into it.
#[test]
fn burial_orders_interior_trench_terrace_adatom() {
    let n = 6;
    let top = n as f64 * A;
    let centre_xy = DVec3::new(top / 2.0, top / 2.0, 0.0);

    let mut sites = diamond_sites(n, n, n);
    // The trench: one cell wide along y, one cell deep, a quarter of the
    // block away from the terrace atom.
    let trench_x = top / 4.0;
    sites.retain(|p| !((p.x - trench_x).abs() < A / 2.0 && p.z > top - A + 0.1));
    let mut structure = structure_of(&sites);
    let interior = atom_near(&structure, DVec3::splat(top / 2.0));
    let terrace = atom_near(&structure, centre_xy + DVec3::new(A, 0.0, top));
    let trench = atom_near(&structure, DVec3::new(trench_x, top / 2.0, top - A));
    let terrace_position = structure.get_atom(terrace).unwrap().position;
    let adatom = structure.add_atom(CARBON, terrace_position + DVec3::new(0.0, 0.0, 1.5));

    let (values, _) = atom_exposure(&structure, &[interior, trench, terrace, adatom]);
    let [e_interior, e_trench, e_terrace, e_adatom] =
        [interior, trench, terrace, adatom].map(|id| exposure_of(&values, id));
    println!(
        "exposure: interior {e_interior}, trench {e_trench}, terrace {e_terrace}, adatom {e_adatom}"
    );
    assert!(e_interior < 0.1, "interior {e_interior}");
    assert!(
        e_trench < e_terrace,
        "trench {e_trench}, terrace {e_terrace}"
    );
    assert!(
        e_terrace < e_adatom,
        "terrace {e_terrace}, adatom {e_adatom}"
    );
}

/// Culled atoms (left out of the receivers) still occlude: a receiver's value
/// does not depend on which other atoms are receivers.
#[test]
fn atoms_left_out_of_the_receivers_still_occlude() {
    let structure = block(3);
    let ids = all_ids(&structure);
    let (all, _) = atom_exposure(&structure, &ids);
    let one = ids[ids.len() / 2];
    let (alone, _) = atom_exposure(&structure, &[one]);
    assert_eq!(alone.len(), 1);
    assert_eq!(alone.get(one), all.get(one));
    assert!(exposure_of(&alone, one) < 1.0);
}

/// Ghosts (structure alpha below 1, set by `xray`) do not occlude.
#[test]
fn ghosts_do_not_occlude() {
    let mut structure = structure_of(&[DVec3::ZERO, DVec3::new(1.54, 0.0, 0.0)]);
    structure.set_atom_alpha(2, 0.5);
    let (values, _) = atom_exposure(&structure, &[1]);
    assert_eq!(values.get(1), Some(FULLY_EXPOSED));
}

/// `ao_receivers` returns no receivers for a diff structure, and leaves out
/// culled atoms, ghosts and every atom under *Scene transparency*.
#[test]
fn ao_receivers_follows_the_tessellators_opaque_set() {
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(CARBON, DVec3::ZERO);
    assert!(ao_receivers(&diff, &prefs()).is_empty());

    let mut structure = structure_of(&[DVec3::ZERO, DVec3::X * 1.5, DVec3::X * 3.0]);
    assert_eq!(ao_receivers(&structure, &prefs()), all_ids(&structure));

    structure.set_atom_alpha(2, 0.5);
    structure.set_atom_in_crystal_depth(3, 20.0);
    let culling = AtomicStructureVisualizationPreferences {
        ball_and_stick_cull_depth: Some(8.0),
        ..prefs()
    };
    assert_eq!(ao_receivers(&structure, &culling), vec![1]);

    let transparent = AtomicStructureVisualizationPreferences {
        scene_transparency_enabled: true,
        scene_alpha: 0.5,
        ..prefs()
    };
    assert!(ao_receivers(&structure, &transparent).is_empty());
}

// ============================================================================
// The cap table (D7)
// ============================================================================

/// The table against the exact cone test on 1 000 random neighbourhoods:
/// mean absolute error of `E` below 0.03, largest below 0.08.
#[test]
fn the_table_agrees_with_the_exact_cone_test() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut sum, mut worst) = (0.0, 0.0f64);
    for _ in 0..1000 {
        let occluders = 1 + (rng.next() * 30.0) as usize;
        let (mut table, mut exact) = (0u128, 0u128);
        for _ in 0..occluders {
            let axis = rng.unit_vector();
            let distance = 1.0 + rng.next() * (R_MAX - 1.0);
            let radius = 0.3 + rng.next() * 1.5;
            let half_angle = (radius / distance).min(1.0).asin();
            table |= cap_mask(axis, half_angle);
            exact |= cap_mask_exact(axis, half_angle);
        }
        let error = (exposure_of_mask(table).0 - exposure_of_mask(exact).0).abs();
        sum += error;
        worst = worst.max(error);
    }
    let mean = sum / 1000.0;
    println!("cap table: mean error {mean}, largest {worst}");
    assert!(mean < 0.03, "mean {mean}");
    assert!(worst < 0.08, "largest {worst}");
}

/// The kernel finds an occluder's cap without a square root or an `asin`
/// (cube cell of the raw offset, half-angle bucket from `r²/d²`); on 1 000
/// random single occluders it blocks exactly what `cap_mask` of the unit
/// direction and `asin(r/d)` blocks.
#[test]
fn the_kernel_picks_the_same_cap_as_cap_mask() {
    let mut rng = Rng(0x5851_F42D_4C95_7F2D);
    for _ in 0..1000 {
        let axis = rng.unit_vector();
        let distance = 0.5 + rng.next() * (R_MAX - 0.5);
        let (&z, info) = ATOM_INFO.iter().nth((rng.next() * 50.0) as usize).unwrap();
        let mut structure = structure_of(&[DVec3::ZERO]);
        structure.add_atom(z as i16, axis * distance);
        let (values, _) = atom_exposure(&structure, &[1]);
        let half_angle = (info.covalent_radius / distance).min(1.0).asin();
        let (e, b) = exposure_of_mask(cap_mask(axis, half_angle));
        assert_eq!(
            values.get(1),
            Some(pack_exposure(e, b)),
            "axis {axis}, distance {distance}, element {z}"
        );
    }
}

/// The same structure gives bit-identical values across runs and thread
/// counts: `rayon`'s order must not leak into results.
#[test]
fn exposure_is_deterministic_across_thread_counts() {
    let structure = block(3);
    let ids = all_ids(&structure);
    let run = |threads: usize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| atom_exposure(&structure, &ids).0)
    };
    let one = run(1);
    assert_eq!(one, run(4));
    assert_eq!(one, run(1));
}

/// Two atoms related by a lattice translation, both farther than `R_MAX`
/// from the block's side faces and at the same depth, get equal values.
#[test]
fn symmetry_equivalent_atoms_get_equal_values() {
    let structure = block(8);
    // 2.6 Å below the top face, so the value is neither 0 nor 1.
    let z = 7.0 * A + 0.25 * A;
    let p = DVec3::new(3.0 * A + 0.25 * A, 3.0 * A + 0.25 * A, z);
    let q = p + DVec3::new(A, A, 0.0);
    let (a, b) = (atom_near(&structure, p), atom_near(&structure, q));
    assert_eq!(structure.get_atom(a).unwrap().position, p);
    assert_eq!(structure.get_atom(b).unwrap().position, q);
    let (values, _) = atom_exposure(&structure, &[a, b]);
    let e = exposure_of(&values, a);
    assert!(e > 0.0 && e < 1.0, "{e}");
    assert_eq!(values.get(a), values.get(b));
}

// ============================================================================
// The bent vector (D10)
// ============================================================================

#[test]
fn isolated_and_fully_buried_atoms_have_no_bent_vector() {
    assert_eq!(exposure_of_mask(0), (1.0, DVec3::ZERO));
    assert_eq!(exposure_of_mask(u128::MAX), (0.0, DVec3::ZERO));
    // An isolated atom's packed value is (0, 0, 0, 1).
    assert_eq!(pack_exposure(1.0, DVec3::ZERO), FULLY_EXPOSED);
}

/// A (100) terrace atom's bent vector points within 10° of the surface
/// normal, with a length below ¼.
#[test]
fn a_terrace_atoms_bent_vector_is_the_surface_normal() {
    let n = 6;
    let top = n as f64 * A;
    let structure = block(n);
    let terrace = atom_near(&structure, DVec3::new(top / 2.0, top / 2.0, top));
    let (values, _) = atom_exposure(&structure, &[terrace]);
    let (_, b) = unpack_exposure(values.get(terrace).unwrap());
    let b = b.as_dvec3();
    let angle = b.angle_between(DVec3::Z).to_degrees();
    println!("terrace bent vector {b}, {angle}° from the normal");
    assert!(angle < 10.0, "{angle}°");
    assert!(b.length() < 0.25, "|b| = {}", b.length());
}

/// `|b| ≤ ¼` on 1 000 random neighbourhoods, up to the 128 directions'
/// quantisation (half a direction).
#[test]
fn the_bent_vector_is_at_most_a_quarter_long() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut longest = 0.0f64;
    for _ in 0..1000 {
        let mut mask = 0u128;
        for _ in 0..1 + (rng.next() * 30.0) as usize {
            let distance = 1.0 + rng.next() * (R_MAX - 1.0);
            let half_angle = ((0.3 + rng.next() * 1.5) / distance).min(1.0).asin();
            mask |= cap_mask(rng.unit_vector(), half_angle);
        }
        longest = longest.max(exposure_of_mask(mask).1.length());
    }
    // The extreme case, a hemisphere, at every axis direction.
    for axis in directions() {
        let mut mask = 0u128;
        for (k, d) in directions().iter().enumerate() {
            if d.dot(*axis) < 0.0 {
                mask |= 1 << k;
            }
        }
        longest = longest.max(exposure_of_mask(mask).1.length());
    }
    println!("longest bent vector {longest}");
    assert!(longest <= 0.25 + 0.5 / DIRECTION_COUNT as f64, "{longest}");
}

/// D10's closed form: a half-space sky (every direction below a tilted plane
/// blocked) gives `V(N) = E + 2·b·N = (1 + cos θ)/2`, through packing, at
/// five tilts of `N` from the open side's normal.
#[test]
fn a_half_space_sky_has_the_closed_form_visibility() {
    for normal in [
        DVec3::Z,
        DVec3::new(1.0, 2.0, 3.0).normalize(),
        DVec3::new(-0.3, 0.9, -0.2).normalize(),
    ] {
        let mut mask = 0u128;
        for (k, d) in directions().iter().enumerate() {
            if d.dot(normal) < 0.0 {
                mask |= 1 << k;
            }
        }
        let (e, b) = exposure_of_mask(mask);
        let (e, b) = unpack_exposure(pack_exposure(e, b));
        // A unit vector perpendicular to `normal`, to tilt `N` towards.
        let side = normal.any_orthonormal_vector();
        for degrees in [0.0f64, 45.0, 90.0, 135.0, 180.0] {
            let (s, c) = degrees.to_radians().sin_cos();
            let n = (normal * c + side * s).as_vec3();
            let v = (e + 2.0 * b.dot(n)).clamp(0.0, 1.0) as f64;
            let expected = (1.0 + c) / 2.0;
            assert!(
                (v - expected).abs() < 0.04,
                "normal {normal}, {degrees}°: V {v}, closed form {expected}"
            );
        }
    }
}

#[test]
fn packing_round_trips_within_one_step() {
    let mut rng = Rng(0x0123_4567_89AB_CDEF);
    for _ in 0..1000 {
        let e = rng.next();
        let b = rng.unit_vector() * rng.next() * 0.25;
        let (e2, b2) = unpack_exposure(pack_exposure(e, b));
        assert!((e2 as f64 - e).abs() <= 0.5 / 127.0 + 1e-6, "E {e} → {e2}");
        // `b` travels as 4b, so its step is a quarter of 1/127.
        let error = (b2.as_dvec3() - b).abs().max_element();
        assert!(error <= 0.25 * 0.5 / 127.0 + 1e-6, "b {b} → {b2}");
    }
}

// ============================================================================
// Into the vertices
// ============================================================================

fn tessellate(
    structure: &AtomicStructure,
    exposure: Option<&AtomExposure>,
) -> (AtomImpostorMesh, BondImpostorMesh) {
    let (mut atoms, mut bonds, mut transparent) = (
        AtomImpostorMesh::new(),
        BondImpostorMesh::new(),
        TransparentImpostorMesh::new(),
    );
    tessellate_atomic_structure_impostors(
        &mut atoms,
        &mut bonds,
        &mut transparent,
        structure,
        &prefs(),
        exposure,
    );
    (atoms, bonds)
}

/// Opaque atoms carry their packed value on every corner and bonds carry
/// their two ends'; `exposure: None` leaves every vertex fully exposed.
#[test]
fn opaque_atoms_and_bonds_carry_their_packed_values() {
    let mut structure = structure_of(&[DVec3::ZERO, DVec3::X * 1.54, DVec3::new(0.0, 5.0, 0.0)]);
    structure.add_bond(1, 2, 1);
    let (values, _) = atom_exposure(&structure, &all_ids(&structure));
    assert_ne!(values.get(1), Some(FULLY_EXPOSED));

    let (atoms, bonds) = tessellate(&structure, Some(&values));
    for atom in structure.atoms_values() {
        let corners: Vec<_> = atoms
            .vertices
            .iter()
            .filter(|v| v.center_position == atom.position.as_vec3().to_array())
            .collect();
        assert_eq!(corners.len(), 4);
        assert!(
            corners
                .iter()
                .all(|v| Some(v.exposure) == values.get(atom.id))
        );
    }
    assert_eq!(bonds.vertices.len(), 4);
    for v in &bonds.vertices {
        let (start, end) = if v.start_position == [0.0; 3] {
            (1, 2)
        } else {
            (2, 1)
        };
        assert_eq!(Some(v.exposure_start), values.get(start));
        assert_eq!(Some(v.exposure_end), values.get(end));
    }

    let (atoms, bonds) = tessellate(&structure, None);
    assert!(atoms.vertices.iter().all(|v| v.exposure == FULLY_EXPOSED));
    assert!(
        bonds
            .vertices
            .iter()
            .all(|v| v.exposure_start == FULLY_EXPOSED && v.exposure_end == FULLY_EXPOSED)
    );
}

/// Delete markers and anchor spheres are fully exposed even when the values
/// passed in hold an entry for them.
#[test]
fn markers_and_anchors_are_fully_exposed() {
    let mut diff = AtomicStructure::new_diff();
    let marker = diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::ZERO);
    let moved = diff.add_atom(CARBON, DVec3::X * 1.54);
    diff.set_anchor_position(moved, DVec3::new(0.0, 3.0, 0.0));
    diff.decorator_mut().show_anchor_arrows = true;
    // Bypass `ao_receivers` (which has none for a diff) to give every atom a
    // value.
    let (values, _) = atom_exposure(&diff, &[marker, moved]);
    assert!(values.contains(marker));

    let (atoms, bonds) = tessellate(&diff, Some(&values));
    let marker_position = DVec3::ZERO.as_vec3().to_array();
    let anchor_position = DVec3::new(0.0, 3.0, 0.0).as_vec3().to_array();
    for v in &atoms.vertices {
        if v.center_position == marker_position || v.center_position == anchor_position {
            assert_eq!(v.exposure, FULLY_EXPOSED, "at {:?}", v.center_position);
        }
    }
    assert!(
        atoms
            .vertices
            .iter()
            .any(|v| v.center_position == anchor_position)
    );
    // The anchor arrow is a plain bond quad.
    assert!(
        bonds
            .vertices
            .iter()
            .all(|v| v.exposure_start == FULLY_EXPOSED && v.exposure_end == FULLY_EXPOSED)
    );
}

// ============================================================================
// Incremental updates (D7): each compared with `==` against `atom_exposure`
// on the new structure.
// ============================================================================

/// `update_atom_exposure` from `old`'s values to `new`, with the changes
/// taken from `occlusion_changes(occluder_snapshot(old), new)`, asserted equal
/// to a full computation on `new`.
fn assert_update_is_full(
    old: &AtomicStructure,
    old_receivers: &[u32],
    new: &AtomicStructure,
    new_receivers: &[u32],
) -> atomcad_display::ambient_occlusion::ExposureStats {
    let (previous, _) = atom_exposure(old, old_receivers);
    let changes = occlusion_changes(&occluder_snapshot(old), new);
    let (updated, stats) = update_atom_exposure(&previous, &changes, new, new_receivers);
    let (full, _) = atom_exposure(new, new_receivers);
    assert_eq!(updated, full);
    stats
}

#[test]
fn moving_adding_and_deleting_an_atom_updates_exactly() {
    let old = block(6);
    let ids = all_ids(&old);
    let centre = atom_near(&old, DVec3::splat(3.0 * A));

    let mut moved = old.clone();
    let p = moved.get_atom(centre).unwrap().position;
    moved.set_atom_position(centre, p + DVec3::new(0.5, 0.0, 0.0));
    let stats = assert_update_is_full(&old, &ids, &moved, &ids);
    assert!(!stats.full_path);
    assert!(stats.receivers_computed > 0);
    assert!(stats.receivers_computed < ids.len() / 4);

    let mut added = old.clone();
    let new_atom = added.add_atom(CARBON, DVec3::new(3.0 * A, 3.0 * A, 6.0 * A + 1.5));
    let mut added_ids = ids.clone();
    added_ids.push(new_atom);
    assert_update_is_full(&old, &ids, &added, &added_ids);

    let mut deleted = old.clone();
    deleted.delete_atom(centre);
    let remaining: Vec<u32> = ids.iter().copied().filter(|id| *id != centre).collect();
    assert_update_is_full(&old, &ids, &deleted, &remaining);
}

/// An atom whose alpha drops below 1 stops occluding.
#[test]
fn a_ghosted_atom_updates_exactly() {
    let old = block(4);
    let ids = all_ids(&old);
    let mut new = old.clone();
    let ghost = atom_near(&new, DVec3::splat(2.0 * A));
    new.set_atom_alpha(ghost, 0.3);
    let receivers: Vec<u32> = ids.iter().copied().filter(|id| *id != ghost).collect();
    let stats = assert_update_is_full(&old, &ids, &new, &receivers);
    assert!(stats.receivers_computed > 0);
}

/// A changed receiver set (a different cull depth) with no geometry change
/// computes only the new receivers.
#[test]
fn a_changed_receiver_set_computes_only_the_new_receivers() {
    let structure = block(4);
    let ids = all_ids(&structure);
    let (half, rest) = ids.split_at(ids.len() * 9 / 10);
    let (previous, _) = atom_exposure(&structure, half);
    let changes = occlusion_changes(&occluder_snapshot(&structure), &structure);
    assert!(changes.is_empty());
    let (updated, stats) = update_atom_exposure(&previous, &changes, &structure, &ids);
    assert_eq!(stats.receivers_computed, rest.len());
    assert_eq!(updated, atom_exposure(&structure, &ids).0);

    // Fewer receivers: the dropped ones' values go, nothing is computed.
    let (fewer, stats) = update_atom_exposure(&updated, &changes, &structure, half);
    assert_eq!(stats.receivers_computed, 0);
    assert_eq!(fewer, previous);
}

#[test]
fn an_empty_diff_computes_nothing() {
    let structure = block(3);
    let ids = all_ids(&structure);
    let (previous, _) = atom_exposure(&structure, &ids);
    let changes = occlusion_changes(&occluder_snapshot(&structure), &structure.clone());
    let (updated, stats) = update_atom_exposure(&previous, &changes, &structure, &ids);
    assert_eq!(stats.receivers_computed, 0);
    assert!(!stats.full_path);
    assert_eq!(updated, previous);
}

/// A diff over most atoms takes the full path; a receiver set needing just
/// under `FULL_PATH_FRACTION` of the receivers computed does not, and one
/// just over it does.
#[test]
fn the_full_path_starts_above_a_quarter_of_the_receivers() {
    let old = block(3);
    let ids = all_ids(&old);
    let mut shifted = old.clone();
    for id in &ids {
        let p = shifted.get_atom(*id).unwrap().position;
        shifted.set_atom_position(*id, p * 1.01);
    }
    let stats = assert_update_is_full(&old, &ids, &shifted, &ids);
    assert!(stats.full_path);

    let limit = (ids.len() as f64 * FULL_PATH_FRACTION).floor() as usize;
    let no_changes = occlusion_changes(&occluder_snapshot(&old), &old);
    for (missing, full_path) in [(limit, false), (limit + 1, true)] {
        let (previous, _) = atom_exposure(&old, &ids[missing..]);
        let (updated, stats) = update_atom_exposure(&previous, &no_changes, &old, &ids);
        assert_eq!(stats.receivers_computed, missing);
        assert_eq!(stats.full_path, full_path, "{missing} of {}", ids.len());
        assert_eq!(updated, atom_exposure(&old, &ids).0);
    }
}

/// Three successive moves of the same atom, each tick's diff taken against
/// the tick before and folded by `merge_changes`, then a single update at
/// the end: equal to a full computation on the final structure. Replacing
/// instead of merging forgets the first ticks' places and fails this.
#[test]
fn three_merged_moves_update_exactly() {
    let first = block(7);
    let ids = all_ids(&first);
    let mover = atom_near(&first, DVec3::splat(2.0 * A));
    let (previous, _) = atom_exposure(&first, &ids);

    let mut pending = Default::default();
    let mut current = first.clone();
    for step in 1..=3 {
        let mut next = current.clone();
        let p = next.get_atom(mover).unwrap().position;
        next.set_atom_position(mover, p + DVec3::new(1.5, 0.0, 0.0) * step as f64);
        pending = merge_changes(
            pending,
            occlusion_changes(&occluder_snapshot(&current), &next),
        );
        current = next;
    }
    let (updated, stats) = update_atom_exposure(&previous, &pending, &current, &ids);
    assert!(!stats.full_path);
    assert_eq!(updated, atom_exposure(&current, &ids).0);
}

/// An occluder at exactly `R_MAX` from a receiver, and one ulp inside and
/// outside it, updated through the structure-grid path, equals the full path
/// in all three cases (the shared `within_r_max` predicate).
#[test]
fn an_occluder_at_r_max_is_judged_alike_by_both_paths() {
    // The element with the largest covalent radius, so that its cap at 6 Å
    // covers some of the 128 directions.
    let (&big, _) = ATOM_INFO
        .iter()
        .filter(|(z, _)| **z < 119)
        .max_by(|a, b| a.1.covalent_radius.total_cmp(&b.1.covalent_radius))
        .unwrap();
    // Far-off bystanders, so that the update stays below the full-path limit.
    let mut base = AtomicStructure::new();
    let receiver = base.add_atom(CARBON, DVec3::ZERO);
    for i in 0..20 {
        base.add_atom(CARBON, DVec3::new(-100.0 - 10.0 * i as f64, 0.0, 0.0));
    }
    let occluder = base.add_atom(big as i16, DVec3::new(200.0, 0.0, 0.0));

    for (x, inside) in [
        (R_MAX, true),
        (R_MAX.next_down(), true),
        (R_MAX.next_up(), false),
    ] {
        let mut new = base.clone();
        new.set_atom_position(occluder, DVec3::new(x, 0.0, 0.0));
        let ids = all_ids(&new);
        let receivers: Vec<u32> = ids.iter().copied().filter(|id| *id != occluder).collect();
        let stats = assert_update_is_full(&base, &receivers, &new, &receivers);
        assert!(!stats.full_path);
        let (values, _) = atom_exposure(&new, &[receiver]);
        assert_eq!(
            values.get(receiver) != Some(FULLY_EXPOSED),
            inside,
            "occluder at {x}"
        );
    }
}

/// A producer may renumber atoms that did not move (`atom_edit`'s
/// `apply_diff` does): matched by place, a renumbered block with one atom
/// moved diffs as that atom's two places, every other value is re-keyed, and
/// the update equals a full computation on the renumbered structure.
#[test]
fn a_renumbered_structure_diffs_by_place() {
    let sites = diamond_sites(6, 6, 6);
    let old = structure_of(&sites);
    let old_ids = all_ids(&old);
    let (previous, _) = atom_exposure(&old, &old_ids);

    // The same sites in reverse order, so almost every id names another
    // atom, and the one at the centre moved by 0.5 Å.
    let centre = atom_near(&old, DVec3::splat(3.0 * A));
    let centre_site = old.get_atom(centre).unwrap().position;
    let mut new_sites: Vec<DVec3> = sites.iter().rev().copied().collect();
    for p in &mut new_sites {
        if *p == centre_site {
            *p += DVec3::new(0.5, 0.0, 0.0);
        }
    }
    let new = structure_of(&new_sites);
    let new_ids = all_ids(&new);

    let by_id = occlusion_changes(&occluder_snapshot(&old), &new);
    assert!(
        by_id.len() > old_ids.len(),
        "by id, nearly everything moved"
    );

    let (changes, renumbering) = occlusion_changes_by_place(&occluder_snapshot(&old), &new);
    assert_eq!(changes.len(), 2, "the moved atom's old and new place");
    assert!(renumbering.renumbered_count() > old_ids.len() / 2);
    assert_eq!(renumbering.new_id(centre), None, "the moved atom is lost");

    let (updated, stats) =
        update_atom_exposure(&previous.remapped(&renumbering), &changes, &new, &new_ids);
    assert!(!stats.full_path);
    assert!(stats.receivers_computed < new_ids.len() / 4);
    assert_eq!(updated, atom_exposure(&new, &new_ids).0);
}

/// Atoms at one place are matched one to one; a surplus is a change there.
#[test]
fn coincident_atoms_are_matched_one_to_one() {
    let p = DVec3::new(1.0, 2.0, 3.0);
    let old = structure_of(&[p, p]);
    let new = structure_of(&[p]);
    let (changes, renumbering) = occlusion_changes_by_place(&occluder_snapshot(&old), &new);
    assert_eq!(changes.len(), 1);
    assert!(renumbering.keeps_ids());
}

/// With ids kept, the place diff equals the id diff and renumbers nothing.
#[test]
fn a_structure_that_keeps_its_ids_diffs_as_by_id() {
    let old = block(4);
    let mut new = old.clone();
    let moved = atom_near(&new, DVec3::splat(2.0 * A));
    let p = new.get_atom(moved).unwrap().position;
    new.set_atom_position(moved, p + DVec3::new(0.3, 0.0, 0.0));
    let snapshot = occluder_snapshot(&old);
    let (changes, renumbering) = occlusion_changes_by_place(&snapshot, &new);
    assert!(renumbering.keeps_ids());
    assert_eq!(changes.len(), occlusion_changes(&snapshot, &new).len());
    assert_eq!(changes.len(), 2);
}
