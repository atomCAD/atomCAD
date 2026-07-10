//! Shared test support for the per-node diff-output roundtrip (issue #295,
//! `doc/design_diff_outputs_for_atom_ops.md` §3.0).
//!
//! Every node that gains a diff output pin must land the mandatory node-level
//! roundtrip test
//!
//! ```text
//! apply_diff(node_input_atoms, diff_pin_value) ≡ result_pin_value
//! ```
//!
//! evaluated through the real node in a real network. `assert_node_diff_roundtrip`
//! is the one-liner that does exactly that: it evaluates the node's pin 0 and
//! pin 1, applies the pin-1 diff (via the `apply_diff` *function*) to the given
//! input atoms, and asserts structural equivalence with the pin-0 result.
//!
//! `#[path]`-included from `tests/structure_designer.rs` alongside
//! `structure_equivalence` (which provides `≡`).
#![allow(dead_code)]

use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff::apply_diff;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

use crate::structure_equivalence::assert_structures_equivalent;

/// The standard `apply_diff` tolerance (0.1 Å). Extracted anchors are exact base
/// positions, so the roundtrip is insensitive to the exact value.
pub const APPLY_TOLERANCE: f64 = 0.1;

/// Evaluates a single output pin of `node_id` in `network_name`.
pub fn evaluate_pin(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    pin: i32,
) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    // The diff pins exclude frozen atoms via minimize_energy; large frozen
    // bulks need the vdW cutoff to relax within the atom limit. Harmless for the
    // small structures used by these tests.
    context.use_vdw_cutoff = true;
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, pin, registry, false, &mut context)
}

/// Extracts the atoms out of a `Crystal` / `Molecule` result, panicking otherwise.
pub fn expect_atoms(result: NetworkResult, context: &str) -> AtomicStructure {
    match result {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        NetworkResult::Error(e) => panic!("{context}: expected atomic result, got Error: {e}"),
        other => panic!(
            "{context}: expected atomic result, got {:?}",
            other.infer_data_type()
        ),
    }
}

/// The mandatory node-level roundtrip (§3.0): evaluates `node_id`'s pin 0 and
/// pin 1, applies the pin-1 diff to `input_atoms`, and asserts structural
/// equivalence with the pin-0 result.
pub fn assert_node_diff_roundtrip(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    input_atoms: &AtomicStructure,
) {
    let result_atoms = expect_atoms(
        evaluate_pin(designer, network_name, node_id, 0),
        "diff roundtrip pin 0",
    );
    let diff_atoms = expect_atoms(
        evaluate_pin(designer, network_name, node_id, 1),
        "diff roundtrip pin 1",
    );
    assert!(
        diff_atoms.is_diff(),
        "pin 1 must be a diff (is_diff == true)"
    );

    let applied = apply_diff(input_atoms, &diff_atoms, APPLY_TOLERANCE).result;
    assert_structures_equivalent(&applied, &result_atoms, 1e-6);
}
