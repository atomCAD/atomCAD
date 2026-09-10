//! P1 tests for the mechanosynthesis replay engine: parsing and validation, the
//! id-rule matrix, wildcard elements, matching, transforms, replay semantics,
//! the highlight tag, `compare_structures`, and a three-step end-to-end
//! fixture. See `design_mechanosynth_node.md` §Phases, Track A P1.
//!
//! No real operation library is involved. Workpieces and expected results are
//! built in code with `add_atom` / `add_bond` and compared with
//! `compare_structures` (an empty mismatch list), never by asserting on atom
//! ids — a replay re-assigns them.

use atomcad_crystolecule::atomic_structure::{AtomicStructure, BondReference};
use atomcad_crystolecule::mechanosynth::{
    BuildScript, DEFAULT_TOLERANCE, HighlightTags, Mismatch, NO_LAYER, NO_SITE, OpLibrary,
    PatternElement, Step, apply_step, compare_structures, describe_mismatches, load_build_script,
    load_library, parse_build_script, parse_library, replay, resolve_tolerance, steps_applied,
    validate_script_ops,
};
use atomcad_test_support::fixture_path;
use glam::{DMat3, DVec3};

const H: i16 = 1;
const C: i16 = 6;
const N: i16 = 7;
const O: i16 = 8;
const SI: i16 = 14;

/// Positions are ideal, so structural comparisons can be far tighter than any
/// match tolerance.
const EXACT: f64 = 1e-9;

// ============================================================================
// Helpers
// ============================================================================

fn library(name: &str) -> OpLibrary {
    load_library(&fixture_path(&format!("mechanosynth/{name}")))
        .unwrap_or_else(|e| panic!("fixture {name} should parse: {e}"))
}

fn script(name: &str) -> BuildScript {
    load_build_script(&fixture_path(&format!("mechanosynth/{name}")))
        .unwrap_or_else(|e| panic!("fixture {name} should parse: {e}"))
}

/// Applies one operation of `lib` at `t` with the identity rotation.
fn apply_named(
    workpiece: &mut AtomicStructure,
    lib: &OpLibrary,
    op_name: &str,
    t: DVec3,
    tolerance: f64,
) -> Vec<u32> {
    let op = lib.get(op_name).expect("op in fixture");
    let step = Step::new(op_name, t);
    apply_step(workpiece, op, &step, 1, tolerance)
        .expect("step should apply")
        .touched
}

/// The pre-metadata highlight request: the current-step tag alone.
fn current_only(tag: &str) -> HighlightTags<'_> {
    HighlightTags {
        current: Some(tag),
        ..HighlightTags::default()
    }
}

fn assert_same(a: &AtomicStructure, b: &AtomicStructure) {
    let mismatches = compare_structures(a, b, 1e-6);
    assert!(
        mismatches.is_empty(),
        "structures differ:\n{}",
        describe_mismatches(&mismatches)
    );
}

/// The only atom within `radius` of `pos`, or a panic naming what was there.
fn atom_at(s: &AtomicStructure, pos: DVec3) -> u32 {
    let found = s.get_atoms_in_radius(&pos, 1e-6);
    assert_eq!(found.len(), 1, "expected exactly one atom at {pos:?}");
    found[0]
}

fn has_atom_at(s: &AtomicStructure, pos: DVec3) -> bool {
    !s.get_atoms_in_radius(&pos, 1e-6).is_empty()
}

fn element_at(s: &AtomicStructure, pos: DVec3) -> i16 {
    s.get_atom(atom_at(s, pos)).unwrap().atomic_number
}

fn bond_order(s: &AtomicStructure, a: u32, b: u32) -> Option<u8> {
    s.get_atom(a)?
        .bonds
        .iter()
        .find(|bond| bond.other_atom_id() == b)
        .map(|bond| bond.bond_order())
}

/// Carbon at the origin with a hydrogen along +z and three more completing the
/// tetrahedron — the "bridgehead" fragment the multi-step fixture works on.
fn methane() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let c = s.add_atom(C, DVec3::ZERO);
    let bond_length = 1.09;
    // cos(109.47 deg) = -1/3, so the three lower hydrogens sit at z = -L/3 on a
    // circle of radius L*sqrt(8)/3.
    let z = -bond_length / 3.0;
    let radius = bond_length * (8.0f64 / 9.0).sqrt();
    let mut directions = vec![DVec3::new(0.0, 0.0, bond_length)];
    for i in 0..3 {
        let angle = std::f64::consts::TAU * (i as f64) / 3.0;
        directions.push(DVec3::new(radius * angle.cos(), radius * angle.sin(), z));
    }
    for d in directions {
        let h = s.add_atom(H, d);
        s.add_bond(c, h, 1);
    }
    s
}

/// `methane()` with the +z hydrogen gone.
fn methyl_radical() -> AtomicStructure {
    let mut s = methane();
    s.delete_atom(atom_at(&s, DVec3::new(0.0, 0.0, 1.09)));
    s
}

// ============================================================================
// Parsing and validation
// ============================================================================

#[test]
fn valid_library_lands_where_the_schema_says() {
    let lib = library("valid_ops.json");
    assert_eq!(lib.tolerance, Some(0.25));
    assert_eq!(lib.ops.len(), 2);

    let keep = lib.get("keep_only").expect("keep_only present");
    assert_eq!(keep.before.atoms.len(), 1);
    assert_eq!(keep.before.atoms[0].id, 1);
    assert_eq!(keep.before.atoms[0].element, PatternElement::Element(C));
    assert_eq!(keep.before.atoms[0].pos, DVec3::ZERO);
    assert!(keep.after.bonds.is_empty());

    let chain = lib.get("add_chain").expect("add_chain present");
    assert_eq!(chain.before.atoms[0].element, PatternElement::Any);
    assert_eq!(chain.after.atoms.len(), 3);
    assert_eq!(chain.after.atoms[1].element, PatternElement::Element(O));
    assert_eq!(chain.after.atoms[2].pos, DVec3::new(0.0, 0.0, 2.8));
    // `order` defaults to 1 and is read when given.
    assert_eq!(chain.after.bonds.len(), 2);
    assert_eq!(chain.after.bonds[0].order, 1);
    assert_eq!(chain.after.bonds[1].order, 2);

    // Unknown top-level, per-op and per-atom keys were ignored, not rejected.
    assert!(lib.get("comment").is_none());
}

#[test]
fn valid_script_lands_where_the_schema_says() {
    let s = script("valid_build.json");
    assert_eq!(s.tolerance, Some(0.15));
    assert_eq!(s.steps.len(), 2);

    assert_eq!(s.steps[0].op, "keep_only");
    assert_eq!(s.steps[0].t, DVec3::new(1.0, 2.0, 3.0));
    assert_eq!(s.steps[0].note.as_deref(), Some("first"));
    // `r` absent means identity.
    assert_eq!(s.steps[0].r, DMat3::IDENTITY);

    // `r` is read as three rows: this one is a +90 deg turn about z.
    assert_eq!(s.steps[1].note, None);
    let turned = s.steps[1].r * DVec3::X;
    assert!((turned - DVec3::Y).length() < EXACT, "got {turned:?}");
}

#[test]
fn script_tolerance_overrides_library_tolerance() {
    let lib = library("valid_ops.json");
    let build = script("valid_build.json");
    assert_eq!(resolve_tolerance(&lib, &build), 0.15);
}

#[test]
fn absent_tolerances_fall_back_to_the_default() {
    let lib = library("methylate_ops.json");
    let build = script("methylate_build.json");
    assert_eq!(lib.tolerance, None);
    assert_eq!(build.tolerance, None);
    assert_eq!(resolve_tolerance(&lib, &build), DEFAULT_TOLERANCE);
    assert_eq!(DEFAULT_TOLERANCE, 0.3);
}

#[test]
fn library_tolerance_applies_when_the_script_states_none() {
    let lib = library("valid_ops.json");
    let build = parse_build_script(
        r#"{ "format": "atomcad-msbuild/1", "steps": [] }"#,
        "build.json",
    )
    .expect("parses");
    assert_eq!(resolve_tolerance(&lib, &build), 0.25);
}

/// Every validation error names the file, the operation (or step) and the
/// offending field.
fn library_error(json: &str) -> String {
    parse_library(json, "ops.json")
        .expect_err("should be rejected")
        .to_string()
}

fn script_error(json: &str) -> String {
    parse_build_script(json, "build.json")
        .expect_err("should be rejected")
        .to_string()
}

#[test]
fn library_rejects_a_wrong_format() {
    let message = library_error(r#"{ "format": "atomcad-msops/2", "ops": [] }"#);
    assert!(message.contains("ops.json"), "{message}");
    assert!(message.contains("format"), "{message}");
    assert!(message.contains("atomcad-msops/1"), "{message}");
}

#[test]
fn script_rejects_a_wrong_format() {
    let message = script_error(r#"{ "format": "atomcad-msops/1", "steps": [] }"#);
    assert!(message.contains("build.json"), "{message}");
    assert!(message.contains("format"), "{message}");
    assert!(message.contains("atomcad-msbuild/1"), "{message}");
}

#[test]
fn library_rejects_a_duplicate_operation_name() {
    let message = library_error(
        r#"{ "format": "atomcad-msops/1", "ops": [
             { "name": "habst", "before": { "atoms": [], "bonds": [] }, "after": { "atoms": [], "bonds": [] } },
             { "name": "habst", "before": { "atoms": [], "bonds": [] }, "after": { "atoms": [], "bonds": [] } }
           ] }"#,
    );
    assert!(message.contains("ops.json"), "{message}");
    assert!(message.contains("habst"), "{message}");
    assert!(message.contains("duplicate operation name"), "{message}");
}

#[test]
fn library_rejects_a_duplicate_id_within_a_pattern() {
    let message = library_error(
        r#"{ "format": "atomcad-msops/1", "ops": [ { "name": "habst",
             "before": { "atoms": [ { "id": 1, "el": "H", "pos": [0,0,0] },
                                    { "id": 1, "el": "C", "pos": [0,0,1] } ], "bonds": [] },
             "after": { "atoms": [], "bonds": [] } } ] }"#,
    );
    assert!(message.contains("ops.json"), "{message}");
    assert!(message.contains("operation 'habst'"), "{message}");
    assert!(message.contains("before pattern"), "{message}");
    assert!(message.contains("duplicate atom id 1"), "{message}");
}

#[test]
fn library_rejects_a_bond_to_an_unknown_id() {
    let message = library_error(
        r#"{ "format": "atomcad-msops/1", "ops": [ { "name": "hdon",
             "before": { "atoms": [ { "id": 1, "el": "C", "pos": [0,0,0] } ], "bonds": [] },
             "after": { "atoms": [ { "id": 1, "el": "C", "pos": [0,0,0] } ], "bonds": [ [1, 5] ] } } ] }"#,
    );
    assert!(message.contains("ops.json"), "{message}");
    assert!(message.contains("operation 'hdon'"), "{message}");
    assert!(message.contains("after pattern"), "{message}");
    assert!(message.contains("unknown atom id 5"), "{message}");
}

#[test]
fn library_rejects_a_self_bond() {
    let message = library_error(
        r#"{ "format": "atomcad-msops/1", "ops": [ { "name": "hdon",
             "before": { "atoms": [ { "id": 1, "el": "C", "pos": [0,0,0] } ], "bonds": [] },
             "after": { "atoms": [ { "id": 1, "el": "C", "pos": [0,0,0] } ], "bonds": [ [1, 1] ] } } ] }"#,
    );
    assert!(message.contains("ops.json"), "{message}");
    assert!(message.contains("operation 'hdon'"), "{message}");
    assert!(message.contains("bond [1, 1]"), "{message}");
    assert!(message.contains("itself"), "{message}");
}

#[test]
fn library_rejects_an_unsupported_bond_order() {
    let message = library_error(
        r#"{ "format": "atomcad-msops/1", "ops": [ { "name": "dimerp",
             "before": { "atoms": [], "bonds": [] },
             "after": { "atoms": [ { "id": 1, "el": "C", "pos": [0,0,0] },
                                   { "id": 2, "el": "C", "pos": [0,0,1.3] } ],
                        "bonds": [ [1, 2, 9] ] } } ] }"#,
    );
    assert!(message.contains("ops.json"), "{message}");
    assert!(message.contains("operation 'dimerp'"), "{message}");
    assert!(message.contains("bond order 9"), "{message}");
}

#[test]
fn library_rejects_a_wildcard_on_an_added_atom() {
    let message = library_error(
        r#"{ "format": "atomcad-msops/1", "ops": [ { "name": "hdon",
             "before": { "atoms": [ { "id": 1, "el": "C", "pos": [0,0,0] } ], "bonds": [] },
             "after": { "atoms": [ { "id": 1, "el": "C", "pos": [0,0,0] },
                                   { "id": 2, "el": "*", "pos": [0,0,1.09] } ], "bonds": [] } } ] }"#,
    );
    assert!(message.contains("ops.json"), "{message}");
    assert!(message.contains("operation 'hdon'"), "{message}");
    assert!(message.contains("after pattern"), "{message}");
    assert!(message.contains("atom id 2"), "{message}");
    assert!(message.contains("\"el\""), "{message}");
}

#[test]
fn library_rejects_an_unknown_element_symbol() {
    let message = library_error(
        r#"{ "format": "atomcad-msops/1", "ops": [ { "name": "habst",
             "before": { "atoms": [ { "id": 1, "el": "Xx", "pos": [0,0,0] } ], "bonds": [] },
             "after": { "atoms": [], "bonds": [] } } ] }"#,
    );
    assert!(message.contains("ops.json"), "{message}");
    assert!(message.contains("operation 'habst'"), "{message}");
    assert!(message.contains("before pattern"), "{message}");
    assert!(
        message.contains("unknown element symbol \"Xx\""),
        "{message}"
    );
}

#[test]
fn script_rejects_a_matrix_that_is_not_three_by_three() {
    let message = script_error(
        r#"{ "format": "atomcad-msbuild/1", "steps": [
             { "op": "habst", "t": [0,0,0], "r": [ [1,0,0], [0,1,0] ] } ] }"#,
    );
    assert!(message.contains("build.json"), "{message}");
    assert!(message.contains("step 1"), "{message}");
    assert!(message.contains("\"r\""), "{message}");
    assert!(message.contains("3×3"), "{message}");
}

#[test]
fn script_rejects_a_malformed_translation() {
    let message = script_error(
        r#"{ "format": "atomcad-msbuild/1", "steps": [ { "op": "habst", "t": [0,0] } ] }"#,
    );
    assert!(message.contains("build.json"), "{message}");
    assert!(message.contains("step 1"), "{message}");
    assert!(message.contains("\"t\""), "{message}");
}

#[test]
fn a_step_naming_an_unknown_operation_is_rejected_against_the_library() {
    let lib = library("methylate_ops.json");
    let build = parse_build_script(
        r#"{ "format": "atomcad-msbuild/1", "steps": [
             { "op": "habst", "t": [0,0,0] },
             { "op": "no_such_op", "t": [0,0,0] } ] }"#,
        "build.json",
    )
    .expect("shape is fine; the op name is a cross-file question");

    let message = validate_script_ops(&build, &lib)
        .expect_err("unknown op")
        .to_string();
    assert!(message.contains("build.json"), "{message}");
    assert!(message.contains("step 2"), "{message}");
    assert!(message.contains("no_such_op"), "{message}");

    // And the same check fires from `replay`, before anything is applied.
    let message = replay(&methane(), &lib, &build, -1, HighlightTags::default())
        .expect_err("unknown op")
        .to_string();
    assert!(message.contains("no_such_op"), "{message}");
}

#[test]
fn invalid_json_names_the_file() {
    let message = library_error("{ not json");
    assert!(message.contains("ops.json"), "{message}");
    assert!(message.contains("invalid JSON"), "{message}");
}

// ============================================================================
// The id-rule matrix
// ============================================================================

/// A lone carbon 0.1 Å off the pattern origin, so "kept" and "snapped to the
/// pattern position" are distinguishable outcomes.
fn offset_carbon() -> (AtomicStructure, DVec3) {
    let offset = DVec3::new(0.1, 0.0, 0.0);
    let mut s = AtomicStructure::new();
    s.add_atom(C, offset);
    (s, offset)
}

#[test]
fn a_kept_atom_is_not_snapped_to_the_pattern_position() {
    let lib = library("id_rules_ops.json");
    let (mut s, offset) = offset_carbon();
    apply_named(&mut s, &lib, "keep", DVec3::ZERO, 0.3);

    assert_eq!(s.get_num_of_atoms(), 1);
    let atom = s.atoms_values().next().unwrap();
    assert_eq!(atom.position, offset, "a kept atom stays where it was");
    assert_eq!(atom.atomic_number, C);
}

#[test]
fn a_moved_atom_lands_exactly_at_the_after_position() {
    let lib = library("id_rules_ops.json");
    // The workpiece atom sits 0.1 Å off the step's origin, well inside the
    // tolerance, so "moved to r*after.pos + t" and "displaced by the pattern's
    // own delta" give different answers.
    let mut s = AtomicStructure::new();
    s.add_atom(C, DVec3::new(5.1, 0.0, 0.0));
    apply_named(&mut s, &lib, "move", DVec3::new(5.0, 0.0, 0.0), 0.3);

    // r*after.pos + t, not "old position plus the pattern's displacement".
    assert_eq!(s.get_num_of_atoms(), 1);
    assert_eq!(
        s.atoms_values().next().unwrap().position,
        DVec3::new(5.0, 0.0, 2.0)
    );
}

#[test]
fn a_replaced_atom_changes_element_in_place() {
    let lib = library("id_rules_ops.json");
    let (mut s, offset) = offset_carbon();
    apply_named(&mut s, &lib, "replace", DVec3::ZERO, 0.3);

    let atom = s.atoms_values().next().unwrap();
    assert_eq!(atom.atomic_number, N);
    assert_eq!(atom.position, offset);
}

#[test]
fn a_deleted_atom_takes_its_outside_bonds_with_it() {
    let lib = library("id_rules_ops.json");
    let mut s = AtomicStructure::new();
    let target = s.add_atom(C, DVec3::ZERO);
    let outsider = s.add_atom(C, DVec3::new(0.0, 0.0, 1.54));
    s.add_bond(target, outsider, 1);
    assert_eq!(s.get_num_of_bonds(), 1);

    let touched = apply_named(&mut s, &lib, "delete", DVec3::ZERO, 0.3);

    assert_eq!(s.get_num_of_atoms(), 1);
    assert_eq!(s.get_num_of_bonds(), 0);
    assert!(s.get_atom(target).is_none());
    assert!(s.get_atom(outsider).is_some());
    // The deleted atom is gone, so its bonded neighbour stands in for it.
    assert_eq!(touched, vec![outsider]);
}

#[test]
fn a_deletions_touched_list_is_its_surviving_neighbours_each_once() {
    let lib = library("id_rules_ops.json");

    // `move_and_delete` keeps id 1 (the carbon) and deletes id 2 (the +z
    // hydrogen), which is bonded to the carbon and to an outside atom. The
    // carbon is already touched as a kept atom and must not be listed twice;
    // the outsider joins as a deletion neighbour; the deleted atom itself is
    // absent.
    let mut s = AtomicStructure::new();
    let c = s.add_atom(C, DVec3::ZERO);
    let doomed = s.add_atom(H, DVec3::new(0.0, 0.0, 1.09));
    let outsider = s.add_atom(C, DVec3::new(0.0, 0.0, 2.63));
    let bystander = s.add_atom(H, DVec3::new(0.0, 1.09, 0.0));
    s.add_bond(c, doomed, 1);
    s.add_bond(doomed, outsider, 1);
    s.add_bond(c, bystander, 1);

    let touched = apply_named(&mut s, &lib, "move_and_delete", DVec3::ZERO, 0.3);

    let mut sorted = touched.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), touched.len(), "no id is reported twice");
    assert!(touched.contains(&c), "the kept atom is touched");
    assert!(
        touched.contains(&outsider),
        "the deleted atom's neighbour is touched"
    );
    assert!(!touched.contains(&doomed), "a deleted atom is not touched");
    assert!(
        !touched.contains(&bystander),
        "an atom bonded only to a kept atom is not a deletion neighbour"
    );
    assert_eq!(touched.len(), 2);

    // A deletion with no bonds touches nothing at all.
    let mut s = AtomicStructure::new();
    s.add_atom(C, DVec3::ZERO);
    assert!(apply_named(&mut s, &lib, "delete", DVec3::ZERO, 0.3).is_empty());
}

#[test]
fn an_added_atom_arrives_bonded() {
    let lib = library("id_rules_ops.json");
    let mut s = AtomicStructure::new();
    let host = s.add_atom(C, DVec3::ZERO);
    let touched = apply_named(&mut s, &lib, "add", DVec3::ZERO, 0.3);

    assert_eq!(s.get_num_of_atoms(), 2);
    let added = atom_at(&s, DVec3::new(0.0, 0.0, 1.09));
    assert_eq!(s.get_atom(added).unwrap().atomic_number, H);
    assert_eq!(bond_order(&s, host, added), Some(1));
    // The step touched both the kept host and the new hydrogen.
    assert_eq!(touched.len(), 2);
    assert!(touched.contains(&host) && touched.contains(&added));
}

#[test]
fn bonds_are_added_deleted_and_reordered() {
    let lib = library("id_rules_ops.json");

    // Added.
    let mut s = AtomicStructure::new();
    let a = s.add_atom(C, DVec3::ZERO);
    let b = s.add_atom(C, DVec3::new(0.0, 0.0, 1.54));
    apply_named(&mut s, &lib, "bond_add", DVec3::ZERO, 0.3);
    assert_eq!(bond_order(&s, a, b), Some(1));
    assert_eq!(s.get_num_of_bonds(), 1);

    // Deleted.
    apply_named(&mut s, &lib, "bond_delete", DVec3::ZERO, 0.3);
    assert_eq!(bond_order(&s, a, b), None);
    assert_eq!(s.get_num_of_bonds(), 0);

    // Order changed (on a workpiece that already carries the order-1 bond, so
    // this is also the "adding a bond the workpiece already has" case).
    s.add_bond(a, b, 1);
    apply_named(&mut s, &lib, "bond_order", DVec3::ZERO, 0.3);
    assert_eq!(bond_order(&s, a, b), Some(2));
    assert_eq!(s.get_num_of_bonds(), 1, "still one bond, not two");
}

#[test]
fn deleting_an_absent_bond_is_a_no_op_and_a_before_bond_is_never_verified() {
    let lib = library("id_rules_ops.json");
    let mut s = AtomicStructure::new();
    let a = s.add_atom(C, DVec3::ZERO);
    let b = s.add_atom(C, DVec3::new(0.0, 0.0, 1.54));

    // `bond_delete`'s `before` claims a bond the workpiece does not have. The
    // match must still succeed (matching ignores bonds) and the deletion is a
    // no-op rather than an error.
    apply_named(&mut s, &lib, "bond_delete", DVec3::ZERO, 0.3);
    assert_eq!(s.get_num_of_bonds(), 0);
    assert_eq!(s.get_num_of_atoms(), 2);
    assert!(s.get_atom(a).is_some() && s.get_atom(b).is_some());

    // A bond present in both patterns with the same order leaves the workpiece
    // exactly as it found it.
    s.add_bond(a, b, 2);
    apply_named(&mut s, &lib, "bond_unchanged", DVec3::ZERO, 0.3);
    assert_eq!(
        bond_order(&s, a, b),
        Some(2),
        "an unchanged bond is not rewritten to the pattern's order"
    );
}

#[test]
fn bonds_to_atoms_outside_the_pattern_survive_keeps_and_moves() {
    let lib = library("id_rules_ops.json");

    // Kept atom: `keep` touches only the carbon; its bond to the outside
    // hydrogen is untouched.
    let mut s = AtomicStructure::new();
    let c = s.add_atom(C, DVec3::ZERO);
    let outsider = s.add_atom(H, DVec3::new(0.0, 1.09, 0.0));
    s.add_bond(c, outsider, 1);
    apply_named(&mut s, &lib, "keep", DVec3::ZERO, 0.3);
    assert_eq!(bond_order(&s, c, outsider), Some(1));

    // Moved atom: `move_and_delete` moves the carbon to (3,0,0) and deletes the
    // +z hydrogen. The bond to the outside hydrogen follows the carbon.
    let mut s = AtomicStructure::new();
    let c = s.add_atom(C, DVec3::ZERO);
    let doomed = s.add_atom(H, DVec3::new(0.0, 0.0, 1.09));
    let outsider = s.add_atom(H, DVec3::new(0.0, 1.09, 0.0));
    s.add_bond(c, doomed, 1);
    s.add_bond(c, outsider, 1);
    apply_named(&mut s, &lib, "move_and_delete", DVec3::ZERO, 0.3);

    assert_eq!(s.get_atom(c).unwrap().position, DVec3::new(3.0, 0.0, 0.0));
    assert!(s.get_atom(doomed).is_none());
    assert_eq!(
        bond_order(&s, c, outsider),
        Some(1),
        "a moved atom keeps its bonds to atoms outside the pattern"
    );
    assert_eq!(s.get_num_of_bonds(), 1);
}

// ============================================================================
// The wildcard element
// ============================================================================

#[test]
fn a_wildcard_before_matches_any_element() {
    let lib = library("wildcard_ops.json");
    for element in [C, SI] {
        let mut s = AtomicStructure::new();
        s.add_atom(element, DVec3::ZERO);
        apply_named(&mut s, &lib, "star_to_star", DVec3::ZERO, 0.3);
        let atom = s.atoms_values().next().unwrap();
        assert_eq!(atom.position, DVec3::new(0.0, 0.0, 1.0), "step ran");
        assert_eq!(atom.atomic_number, element, "* -> * keeps the element");
    }
}

#[test]
fn a_wildcard_after_keeps_and_a_concrete_after_replaces() {
    let lib = library("wildcard_ops.json");

    // `*` -> `C` replaces.
    let mut s = AtomicStructure::new();
    s.add_atom(SI, DVec3::ZERO);
    apply_named(&mut s, &lib, "star_to_carbon", DVec3::ZERO, 0.3);
    assert_eq!(s.atoms_values().next().unwrap().atomic_number, C);

    // `C` -> `*` keeps.
    let mut s = AtomicStructure::new();
    s.add_atom(C, DVec3::ZERO);
    apply_named(&mut s, &lib, "carbon_to_star", DVec3::ZERO, 0.3);
    let atom = s.atoms_values().next().unwrap();
    assert_eq!(atom.atomic_number, C);
    assert_eq!(atom.position, DVec3::new(0.0, 0.0, 1.0), "step ran");

    // …and a concrete `before` does not match a different element at all.
    let mut s = AtomicStructure::new();
    s.add_atom(SI, DVec3::ZERO);
    let op = lib.get("carbon_to_star").unwrap();
    let step = Step::new("carbon_to_star", DVec3::ZERO);
    assert!(apply_step(&mut s, op, &step, 1, 0.3).is_err());
}

// ============================================================================
// Matching
// ============================================================================

#[test]
fn the_nearest_candidate_within_tolerance_wins() {
    let lib = library("matching_ops.json");
    let mut s = AtomicStructure::new();
    let far = s.add_atom(C, DVec3::new(0.0, 0.0, 0.2));
    let near = s.add_atom(C, DVec3::new(0.0, 0.0, 0.05));
    apply_named(&mut s, &lib, "mark", DVec3::ZERO, 0.3);

    assert_eq!(
        s.get_atom(near).unwrap().atomic_number,
        N,
        "nearest matched"
    );
    assert_eq!(s.get_atom(far).unwrap().atomic_number, C);
}

#[test]
fn the_element_filter_rejects_a_nearer_atom_of_the_wrong_element() {
    let lib = library("matching_ops.json");
    let mut s = AtomicStructure::new();
    let hydrogen = s.add_atom(H, DVec3::new(0.0, 0.0, 0.05));
    let carbon = s.add_atom(C, DVec3::new(0.0, 0.0, 0.2));
    apply_named(&mut s, &lib, "mark", DVec3::ZERO, 0.3);

    assert_eq!(s.get_atom(carbon).unwrap().atomic_number, N);
    assert_eq!(s.get_atom(hydrogen).unwrap().atomic_number, H);
}

#[test]
fn matching_is_injective() {
    let lib = library("matching_ops.json");
    // `pair` wants two atoms 0.1 Å apart; the workpiece has one, sitting
    // nearest to both. The second before atom must fail rather than claim it
    // twice.
    let mut s = AtomicStructure::new();
    s.add_atom(C, DVec3::new(0.0, 0.0, 0.05));

    let op = lib.get("pair").unwrap();
    let step = Step::new("pair", DVec3::ZERO);
    let message = apply_step(&mut s, op, &step, 1, 0.3)
        .expect_err("cannot match one atom twice")
        .to_string();
    assert!(message.contains("id 2"), "{message}");
    // Nothing was applied: the failure comes before any edit.
    assert_eq!(s.get_num_of_atoms(), 1);
}

#[test]
fn the_tolerance_boundary_holds_on_both_sides() {
    let lib = library("matching_ops.json");
    let tolerance = 0.3;
    let epsilon = 1e-9;

    let mut inside = AtomicStructure::new();
    inside.add_atom(C, DVec3::new(0.0, 0.0, tolerance - epsilon));
    apply_named(&mut inside, &lib, "mark", DVec3::ZERO, tolerance);
    assert_eq!(inside.atoms_values().next().unwrap().atomic_number, N);

    let mut outside = AtomicStructure::new();
    outside.add_atom(C, DVec3::new(0.0, 0.0, tolerance + epsilon));
    let op = lib.get("mark").unwrap();
    let step = Step::new("mark", DVec3::ZERO);
    assert!(apply_step(&mut outside, op, &step, 1, tolerance).is_err());
}

#[test]
fn a_match_failure_message_carries_everything_the_author_needs() {
    let lib = library("matching_ops.json");
    let mut s = AtomicStructure::new();
    s.add_atom(H, DVec3::new(4.5, 0.892, 11.31));

    let op = lib.get("mark").unwrap();
    let step = Step::new("mark", DVec3::new(3.567, 0.892, 11.31));
    let message = apply_step(&mut s, op, &step, 17, 0.3)
        .expect_err("no carbon near")
        .to_string();

    assert!(message.contains("step 17"), "{message}");
    assert!(message.contains("mark"), "{message}");
    assert!(message.contains("3.567"), "{message}");
    assert!(message.contains("0.892"), "{message}");
    assert!(message.contains("11.310"), "{message}");
    assert!(message.contains("id 1"), "{message}");
    assert!(message.contains("(C)"), "{message}");
    assert!(message.contains("0.30"), "{message}");
    assert!(message.contains("nearest atom is H at 0.93"), "{message}");
}

#[test]
fn a_match_failure_on_an_empty_workpiece_says_so() {
    let lib = library("matching_ops.json");
    let mut s = AtomicStructure::new();
    let op = lib.get("mark_any").unwrap();
    let step = Step::new("mark_any", DVec3::ZERO);
    let message = apply_step(&mut s, op, &step, 1, 0.3)
        .expect_err("nothing to match")
        .to_string();
    assert!(message.contains("(*)"), "{message}");
    assert!(message.contains("no atoms"), "{message}");
}

// ============================================================================
// Transforms
// ============================================================================

#[test]
fn steps_place_added_atoms_at_r_times_pos_plus_t() {
    let lib = library("transform_ops.json");
    let build = script("transform_build.json");

    let mut base = AtomicStructure::new();
    for x in [0.0, 10.0, 20.0] {
        base.add_atom(C, DVec3::new(x, 0.0, 0.0));
    }

    let result = replay(&base, &lib, &build, -1, HighlightTags::default()).expect("replays");

    // Step 1, identity: local +x and +y stay put.
    assert_eq!(element_at(&result, DVec3::new(1.0, 0.0, 0.0)), O);
    assert_eq!(element_at(&result, DVec3::new(0.0, 1.0, 0.0)), N);

    // Step 2, +90 deg about z: local +x becomes +y, local +y becomes -x.
    assert_eq!(element_at(&result, DVec3::new(10.0, 1.0, 0.0)), O);
    assert_eq!(element_at(&result, DVec3::new(9.0, 0.0, 0.0)), N);

    // Step 3, an improper rotation (det -1): accepted, and it mirrors.
    assert!((build.steps[2].r.determinant() + 1.0).abs() < EXACT);
    assert_eq!(element_at(&result, DVec3::new(19.0, 0.0, 0.0)), O);
    assert_eq!(element_at(&result, DVec3::new(20.0, 1.0, 0.0)), N);

    assert_eq!(result.get_num_of_atoms(), 9);
    assert_eq!(result.get_num_of_bonds(), 6);
}

// ============================================================================
// Replay semantics
// ============================================================================

/// The expected workpiece after each of the three methylation steps.
fn methylate_expected(step: usize) -> AtomicStructure {
    let mut s = if step == 0 {
        methane()
    } else {
        methyl_radical()
    };
    if step >= 2 {
        let host = atom_at(&s, DVec3::ZERO);
        let carbon = s.add_atom(C, DVec3::new(0.0, 0.0, 1.54));
        let h1 = s.add_atom(H, DVec3::new(0.89, 0.0, 2.17));
        let h2 = s.add_atom(H, DVec3::new(-0.89, 0.0, 2.17));
        s.add_bond(host, carbon, 1);
        s.add_bond(carbon, h1, 1);
        s.add_bond(carbon, h2, 1);
    }
    if step >= 3 {
        let carbon = atom_at(&s, DVec3::new(0.0, 0.0, 1.54));
        let cap = s.add_atom(H, DVec3::new(0.0, 0.0, 2.63));
        s.add_bond(carbon, cap, 1);
    }
    s
}

#[test]
fn replaying_the_multi_step_fixture_matches_a_hand_built_result() {
    let lib = library("methylate_ops.json");
    let build = script("methylate_build.json");
    let base = methane();

    for step in 0..=3 {
        let result = replay(&base, &lib, &build, step as i32, HighlightTags::default())
            .unwrap_or_else(|e| panic!("step {step} should replay: {e}"));
        let expected = methylate_expected(step);
        let mismatches = compare_structures(&result, &expected, EXACT);
        assert!(
            mismatches.is_empty(),
            "step {step} differs from the hand-built structure:\n{}",
            describe_mismatches(&mismatches)
        );
    }
}

#[test]
fn step_zero_is_the_untouched_base_and_out_of_range_clamps_to_the_full_build() {
    let lib = library("methylate_ops.json");
    let build = script("methylate_build.json");
    let base = methane();

    assert_same(
        &replay(&base, &lib, &build, 0, HighlightTags::default()).unwrap(),
        &base,
    );

    let full = methylate_expected(3);
    for step in [3, 4, 99, -1, -7] {
        assert_same(
            &replay(&base, &lib, &build, step, HighlightTags::default()).unwrap(),
            &full,
        );
    }

    assert_eq!(steps_applied(-1, 3), 3);
    assert_eq!(steps_applied(0, 3), 0);
    assert_eq!(steps_applied(2, 3), 2);
    assert_eq!(steps_applied(99, 3), 3);
}

#[test]
fn the_base_is_never_mutated() {
    let lib = library("methylate_ops.json");
    let build = script("methylate_build.json");
    let base = methane();
    let pristine = methane();

    replay(&base, &lib, &build, -1, current_only("ms_current")).expect("replays");
    assert_same(&base, &pristine);
    assert!(base.tag_names().is_empty());
}

#[test]
fn a_failing_step_aborts_the_replay_and_names_its_one_based_index() {
    let lib = library("methylate_ops.json");
    // Step 2 asks for a hydrogen that step 1 already took.
    let build = parse_build_script(
        r#"{ "format": "atomcad-msbuild/1", "steps": [
             { "op": "habst", "t": [0.0, 0.0, 1.09] },
             { "op": "habst", "t": [0.0, 0.0, 1.09] } ] }"#,
        "build.json",
    )
    .expect("parses");

    let message = replay(&methane(), &lib, &build, -1, HighlightTags::default())
        .expect_err("second abstraction has nothing to take")
        .to_string();
    assert!(message.contains("step 2"), "{message}");
    assert!(message.contains("habst"), "{message}");

    // The partial state is reachable by asking for one step fewer.
    let partial = replay(&methane(), &lib, &build, 1, HighlightTags::default())
        .expect("step 1 alone replays");
    assert_same(&partial, &methyl_radical());
}

// ============================================================================
// The highlight tag
// ============================================================================

const HIGHLIGHT: &str = "ms_current";

#[test]
fn the_highlight_marks_exactly_the_current_steps_surviving_atoms() {
    let lib = library("methylate_ops.json");
    let build = script("methylate_build.json");
    let base = methane();

    // Step 3 (`hdon`): the carbon it matched plus the hydrogen it added.
    let result = replay(&base, &lib, &build, 3, current_only(HIGHLIGHT)).unwrap();
    let tagged = result.atoms_with_tag(HIGHLIGHT);
    assert_eq!(tagged.len(), 2);
    assert!(result.atom_has_tag(atom_at(&result, DVec3::new(0.0, 0.0, 1.54)), HIGHLIGHT));
    assert!(result.atom_has_tag(atom_at(&result, DVec3::new(0.0, 0.0, 2.63)), HIGHLIGHT));

    // Step 2 (`gm_methylate`): the host carbon plus the three atoms it added.
    let result = replay(&base, &lib, &build, 2, current_only(HIGHLIGHT)).unwrap();
    let tagged = result.atoms_with_tag(HIGHLIGHT);
    assert_eq!(tagged.len(), 4);
    for pos in [
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, 1.54),
        DVec3::new(0.89, 0.0, 2.17),
        DVec3::new(-0.89, 0.0, 2.17),
    ] {
        assert!(
            result.atom_has_tag(atom_at(&result, pos), HIGHLIGHT),
            "expected the highlight at {pos:?}"
        );
    }

    // Step 1 (`habst`) deletes its only atom. The hydrogen is gone, so the
    // highlight falls on the carbon it was bonded to — the radical site the
    // abstraction produced — and on nothing else.
    let result = replay(&base, &lib, &build, 1, current_only(HIGHLIGHT)).unwrap();
    let tagged = result.atoms_with_tag(HIGHLIGHT);
    assert_eq!(tagged, vec![atom_at(&result, DVec3::ZERO)]);
}

#[test]
fn a_pre_existing_highlight_on_the_base_is_cleared() {
    let lib = library("methylate_ops.json");
    let build = script("methylate_build.json");

    // As if an upstream `mechanosynth` node had already tagged the base.
    let mut base = methane();
    for id in base.atom_ids().copied().collect::<Vec<_>>() {
        base.add_atom_tag(id, HIGHLIGHT).expect("intern ok");
    }
    assert_eq!(base.atoms_with_tag(HIGHLIGHT).len(), 5);

    // Cleared even at n = 0, where no step contributes a replacement.
    let result = replay(&base, &lib, &build, 0, current_only(HIGHLIGHT)).unwrap();
    assert!(result.atoms_with_tag(HIGHLIGHT).is_empty());

    // And at n > 0 only the current step's atoms carry it.
    let result = replay(&base, &lib, &build, 3, current_only(HIGHLIGHT)).unwrap();
    assert_eq!(result.atoms_with_tag(HIGHLIGHT).len(), 2);
}

#[test]
fn no_tag_is_interned_when_no_highlight_is_requested() {
    let lib = library("methylate_ops.json");
    let build = script("methylate_build.json");

    let result = replay(&methane(), &lib, &build, 3, HighlightTags::default()).unwrap();
    assert!(result.tag_names().is_empty());

    let result = replay(&methane(), &lib, &build, 3, current_only(HIGHLIGHT)).unwrap();
    assert_eq!(result.tag_names(), &[HIGHLIGHT.to_string()]);
}

// ============================================================================
// compare_structures
// ============================================================================

/// Two carbons 1.54 Å apart, singly bonded.
fn ethane_core() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(C, DVec3::ZERO);
    let b = s.add_atom(C, DVec3::new(0.0, 0.0, 1.54));
    s.add_bond(a, b, 1);
    s
}

#[test]
fn equal_structures_compare_equal() {
    assert!(compare_structures(&ethane_core(), &ethane_core(), 0.1).is_empty());
    assert!(compare_structures(&methane(), &methane(), 0.1).is_empty());
}

#[test]
fn ids_tags_and_transient_flags_are_ignored() {
    let mut a = ethane_core();
    let mut b = AtomicStructure::new();
    // Build b in the opposite order, so ids do not line up, and pad a slot.
    let second = b.add_atom(C, DVec3::new(0.0, 0.0, 1.54));
    let padding = b.add_atom(H, DVec3::new(9.0, 9.0, 9.0));
    let first = b.add_atom(C, DVec3::ZERO);
    b.delete_atom(padding);
    b.add_bond(first, second, 1);

    a.add_atom_tag(1, "surface").expect("intern ok");
    b.set_atom_selected(first, true);

    assert!(
        compare_structures(&a, &b, 0.1).is_empty(),
        "{}",
        describe_mismatches(&compare_structures(&a, &b, 0.1))
    );
}

#[test]
fn each_mismatch_kind_is_reported_exactly_once() {
    // Unmatched in b: a has an extra atom.
    let mut a = ethane_core();
    a.add_atom(H, DVec3::new(5.0, 0.0, 0.0));
    let mismatches = compare_structures(&a, &ethane_core(), 0.1);
    assert_eq!(
        mismatches,
        vec![Mismatch::UnmatchedInB {
            position: DVec3::new(5.0, 0.0, 0.0),
            element: H,
        }]
    );

    // Unmatched in a: the same, the other way round.
    let mismatches = compare_structures(&ethane_core(), &a, 0.1);
    assert_eq!(
        mismatches,
        vec![Mismatch::UnmatchedInA {
            position: DVec3::new(5.0, 0.0, 0.0),
            element: H,
        }]
    );

    // Element differs.
    let mut b = ethane_core();
    b.set_atomic_number(atom_at(&b, DVec3::ZERO), SI);
    let mismatches = compare_structures(&ethane_core(), &b, 0.1);
    assert_eq!(
        mismatches,
        vec![Mismatch::ElementDiffers {
            position: DVec3::ZERO,
            a_element: C,
            b_element: SI,
        }]
    );

    // Bond only in a.
    let mut b = ethane_core();
    b.delete_bond(&BondReference {
        atom_id1: atom_at(&b, DVec3::ZERO),
        atom_id2: atom_at(&b, DVec3::new(0.0, 0.0, 1.54)),
    });
    let mismatches = compare_structures(&ethane_core(), &b, 0.1);
    assert_eq!(
        mismatches,
        vec![Mismatch::BondOnlyInA {
            position1: DVec3::ZERO,
            position2: DVec3::new(0.0, 0.0, 1.54),
        }]
    );

    // Bond only in b.
    let mismatches = compare_structures(&b, &ethane_core(), 0.1);
    assert_eq!(
        mismatches,
        vec![Mismatch::BondOnlyInB {
            position1: DVec3::ZERO,
            position2: DVec3::new(0.0, 0.0, 1.54),
        }]
    );

    // Bond order differs.
    let mut b = ethane_core();
    b.add_bond_checked(
        atom_at(&b, DVec3::ZERO),
        atom_at(&b, DVec3::new(0.0, 0.0, 1.54)),
        2,
    );
    let mismatches = compare_structures(&ethane_core(), &b, 0.1);
    assert_eq!(
        mismatches,
        vec![Mismatch::BondOrderDiffers {
            position1: DVec3::ZERO,
            position2: DVec3::new(0.0, 0.0, 1.54),
            a_order: 1,
            b_order: 2,
        }]
    );
}

#[test]
fn comparison_respects_its_tolerance_boundary() {
    let tolerance = 0.1;
    let epsilon = 1e-9;

    let mut a = AtomicStructure::new();
    a.add_atom(C, DVec3::ZERO);

    let mut inside = AtomicStructure::new();
    inside.add_atom(C, DVec3::new(0.0, 0.0, tolerance - epsilon));
    assert!(compare_structures(&a, &inside, tolerance).is_empty());

    let mut outside = AtomicStructure::new();
    outside.add_atom(C, DVec3::new(0.0, 0.0, tolerance + epsilon));
    let mismatches = compare_structures(&a, &outside, tolerance);
    assert_eq!(mismatches.len(), 2, "one unmatched on each side");
    assert!(matches!(mismatches[0], Mismatch::UnmatchedInB { .. }));
    assert!(matches!(mismatches[1], Mismatch::UnmatchedInA { .. }));
}

#[test]
fn a_mismatch_renders_as_one_readable_line() {
    let rendered = Mismatch::ElementDiffers {
        position: DVec3::new(1.0, 2.0, 3.0),
        a_element: C,
        b_element: SI,
    }
    .to_string();
    assert!(rendered.contains("(1.000, 2.000, 3.000)"), "{rendered}");
    assert!(rendered.contains('C'), "{rendered}");
    assert!(rendered.contains("Si"), "{rendered}");
}

#[test]
fn a_bare_workpiece_and_a_bare_target_compare_equal() {
    let empty = AtomicStructure::new();
    assert!(compare_structures(&empty, &empty, 0.1).is_empty());
    assert!(has_atom_at(&methane(), DVec3::ZERO));
}

// ============================================================================
// Step metadata: the four optional fields and the two derived tags
// (`doc/design_mechanosynth_step_metadata.md`)
// ============================================================================

const ADDED: &str = "ms_added";
const LAYER: &str = "ms_layer";

/// Every tag the node asks for, under the test's own names.
fn all_tags() -> HighlightTags<'static> {
    HighlightTags {
        current: Some(HIGHLIGHT),
        added: Some(ADDED),
        layer: Some(LAYER),
    }
}

/// The base of `metadata_build.json`: three carbons on the x axis, unbonded.
/// Steps address them by position, so nothing else is needed.
fn three_carbons() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    for x in [0.0, 5.0, 10.0] {
        s.add_atom(C, DVec3::new(x, 0.0, 0.0));
    }
    s
}

#[test]
fn absent_metadata_fields_take_their_defaults() {
    // Step 1 of the fixture states none of the four; the pre-metadata fixtures
    // state none anywhere, and both must read the same.
    let s = script("metadata_build.json");
    assert_eq!(s.steps[0].method, "");
    assert_eq!(s.steps[0].phase, "");
    assert_eq!(s.steps[0].layer, NO_LAYER);
    assert_eq!(s.steps[0].site, NO_SITE);
    assert_eq!(NO_LAYER, -1);
    assert_eq!(NO_SITE, -1);

    let old = script("valid_build.json");
    assert_eq!(old.steps[0].method, "");
    assert_eq!(old.steps[0].layer, NO_LAYER);

    // And a step built in code carries the same defaults.
    let step = Step::new("habst", DVec3::ZERO);
    assert_eq!(step.phase, "");
    assert_eq!(step.site, NO_SITE);
}

#[test]
fn present_metadata_fields_land_where_the_schema_says() {
    let s = script("metadata_build.json");
    assert_eq!(s.steps[1].method, "probe");
    assert_eq!(s.steps[1].phase, "layer1");
    assert_eq!(s.steps[1].layer, 1);
    assert_eq!(s.steps[1].site, 0);
    assert_eq!(s.steps[2].site, 1);
    assert_eq!(s.steps[3].phase, "cleanup");
    assert_eq!(s.steps[4].site, 2);
}

#[test]
fn a_metadata_field_of_the_wrong_type_names_the_step_and_the_field() {
    // A wrong type is an `Invalid` error like any other malformed step — not a
    // serde message about the whole document.
    for (field, value) in [
        ("method", "7"),
        ("phase", "[\"a\"]"),
        ("layer", "\"one\""),
        ("site", "1.5"),
    ] {
        let text = format!(
            r#"{{ "format": "atomcad-msbuild/1", "steps": [
                 {{ "op": "a", "t": [0, 0, 0] }},
                 {{ "op": "b", "t": [0, 0, 0], "{field}": {value} }}
               ] }}"#
        );
        let message = parse_build_script(&text, "build.json")
            .expect_err("a wrong-typed metadata field is rejected")
            .to_string();
        assert!(message.contains("build.json"), "{message}");
        assert!(message.contains("step 2"), "{message}");
        assert!(message.contains(field), "{message}");
    }

    // An explicit null is "absent", so it takes the default rather than failing.
    let nulled = parse_build_script(
        r#"{ "format": "atomcad-msbuild/1", "steps": [
             { "op": "a", "t": [0, 0, 0], "phase": null, "layer": null }
           ] }"#,
        "build.json",
    )
    .expect("an explicit null is absent");
    assert_eq!(nulled.steps[0].phase, "");
    assert_eq!(nulled.steps[0].layer, NO_LAYER);
}

#[test]
fn ms_added_holds_every_surviving_created_atom_and_nothing_else() {
    let lib = library("metadata_ops.json");
    let build = script("metadata_build.json");
    let result = replay(&three_carbons(), &lib, &build, -1, all_tags()).unwrap();

    // Steps 1-3 each created one hydrogen; step 4 deleted the one step 2 made.
    let added = result.atoms_with_tag(ADDED);
    assert_eq!(added.len(), 2, "the deleted hydrogen is not in the set");
    for pos in [DVec3::new(10.0, 0.0, 1.09), DVec3::new(5.0, 0.0, 1.09)] {
        assert!(
            result.atom_has_tag(atom_at(&result, pos), ADDED),
            "expected ms_added at {pos:?}"
        );
    }
    // No base atom is in it, including the one step 5 moved.
    assert!(!has_atom_at(&result, DVec3::new(0.0, 0.0, 1.09)));
    for pos in [
        DVec3::ZERO,
        DVec3::new(5.0, 0.0, 0.0),
        DVec3::new(10.5, 0.0, 0.0),
    ] {
        assert!(
            !result.atom_has_tag(atom_at(&result, pos), ADDED),
            "a base atom is not created at {pos:?}"
        );
    }
}

#[test]
fn ms_layer_holds_what_the_current_layer_created() {
    let lib = library("metadata_ops.json");
    let build = script("metadata_build.json");
    let result = replay(&three_carbons(), &lib, &build, -1, all_tags()).unwrap();

    // The last applied step is in layer 1, so the set is what steps 2 and 3
    // created — minus the hydrogen step 4 took back. Step 1's hydrogen belongs
    // to no layer and step 5's carbon was moved, not created.
    let tagged = result.atoms_with_tag(LAYER);
    assert_eq!(
        tagged,
        vec![atom_at(&result, DVec3::new(5.0, 0.0, 1.09))],
        "only the surviving layer-1 creation"
    );
    assert!(
        !result.atom_has_tag(atom_at(&result, DVec3::new(10.0, 0.0, 1.09)), LAYER),
        "a creation of another layer stays out"
    );
    assert!(
        !result.atom_has_tag(atom_at(&result, DVec3::new(10.5, 0.0, 0.0)), LAYER),
        "an atom the layer merely moved stays out"
    );

    // `ms_current` still means the last step alone: the carbon step 5 moved.
    assert_eq!(
        result.atoms_with_tag(HIGHLIGHT),
        vec![atom_at(&result, DVec3::new(10.5, 0.0, 0.0))]
    );
}

#[test]
fn ms_layer_is_empty_when_the_current_step_names_no_layer() {
    let lib = library("metadata_ops.json");
    let build = script("metadata_build.json");

    // Step 1 states no layer, so there is no terrace to highlight — even though
    // it created an atom, which `ms_added` does pick up.
    let result = replay(&three_carbons(), &lib, &build, 1, all_tags()).unwrap();
    assert!(result.atoms_with_tag(LAYER).is_empty());
    assert_eq!(result.atoms_with_tag(ADDED).len(), 1);

    // Same at step 0: nothing has run at all.
    let result = replay(&three_carbons(), &lib, &build, 0, all_tags()).unwrap();
    assert!(result.atoms_with_tag(LAYER).is_empty());
    assert!(result.atoms_with_tag(ADDED).is_empty());
}

#[test]
fn the_derived_tags_are_cleared_from_a_pre_tagged_base() {
    let lib = library("metadata_ops.json");
    let build = script("metadata_build.json");

    // As if an upstream `mechanosynth` node had painted all three.
    let mut base = three_carbons();
    for id in base.atom_ids().copied().collect::<Vec<_>>() {
        for tag in [HIGHLIGHT, ADDED, LAYER] {
            base.add_atom_tag(id, tag).expect("intern ok");
        }
    }

    // Cleared even at n = 0, where no step contributes a replacement.
    let result = replay(&base, &lib, &build, 0, all_tags()).unwrap();
    for tag in [HIGHLIGHT, ADDED, LAYER] {
        assert!(result.atoms_with_tag(tag).is_empty(), "{tag} not cleared");
    }

    // And at n > 0 no base atom keeps one it was given upstream.
    let result = replay(&base, &lib, &build, -1, all_tags()).unwrap();
    assert!(!result.atom_has_tag(atom_at(&result, DVec3::ZERO), ADDED));
    assert!(!result.atom_has_tag(atom_at(&result, DVec3::ZERO), LAYER));
}

#[test]
fn the_default_highlight_request_interns_none_of_the_three_tags() {
    let lib = library("metadata_ops.json");
    let build = script("metadata_build.json");

    let result = replay(&three_carbons(), &lib, &build, -1, HighlightTags::default()).unwrap();
    assert!(result.tag_names().is_empty());

    // Asking for one does not intern the other two.
    let result = replay(&three_carbons(), &lib, &build, -1, current_only(HIGHLIGHT)).unwrap();
    assert_eq!(result.tag_names(), &[HIGHLIGHT.to_string()]);
}

#[test]
fn a_steps_effect_separates_what_it_created_from_what_it_touched() {
    let lib = library("metadata_ops.json");
    let mut s = three_carbons();
    let host = atom_at(&s, DVec3::ZERO);

    let op = lib.get("grow").expect("op in fixture");
    let effect = apply_step(&mut s, op, &Step::new("grow", DVec3::ZERO), 1, 0.3).unwrap();

    let hydrogen = atom_at(&s, DVec3::new(0.0, 0.0, 1.09));
    assert_eq!(effect.added, vec![hydrogen], "only the new atom is created");
    assert_eq!(effect.touched, vec![host, hydrogen]);

    // A move creates nothing, however much it touches.
    let op = lib.get("nudge").expect("op in fixture");
    let effect = apply_step(&mut s, op, &Step::new("nudge", DVec3::ZERO), 2, 0.3).unwrap();
    assert!(effect.added.is_empty());
    assert_eq!(effect.touched.len(), 1);
}
