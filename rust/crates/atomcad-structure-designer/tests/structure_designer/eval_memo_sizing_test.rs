//! Sizing and exclusion for the per-pass evaluation memo
//! (`doc/design_eval_memoization.md` Phase 2, D6).
//!
//! Nothing here can change an evaluation result: the estimator and the
//! iterator predicate have no callers outside these tests until the memo
//! itself lands in Phase 3. What they *can* do is make the memo's byte budget
//! blind to the payload it exists to bound, which is why the assertions below
//! are about **relations** — a big thing sizes above a small thing, a
//! deep-counted payload tracks its content, a pointer-counted one does not.
//! Absolute byte thresholds would be machine-dependent and would say nothing.

use std::collections::HashMap;
use std::sync::Arc;

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::field::{GridGeometry, SampledField, ScalarField};
use atomcad_crystolecule::motif::Motif;
use atomcad_crystolecule::structure::Structure;
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::evaluator::iterator_walker::Walker;
use atomcad_structure_designer::evaluator::network_evaluator::CaptureKey;
use atomcad_structure_designer::evaluator::network_result::{MoleculeData, NetworkResult};
use atomcad_structure_designer::evaluator::zone_closure::ZoneClosure;
use atomcad_structure_designer::node_data::EvalOutput;
use atomcad_structure_designer::node_network::{NodeNetwork, SourcePin};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_util::memory_size_estimator::MemorySizeEstimator;
use glam::DVec3;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn molecule_with(atom_count: usize) -> NetworkResult {
    let mut atoms = AtomicStructure::new();
    for i in 0..atom_count {
        atoms.add_atom(6, DVec3::new(i as f64 * 1.5, 0.0, 0.0));
    }
    NetworkResult::Molecule(MoleculeData {
        atoms,
        geo_tree_root: None,
    })
}

fn sampled_field(dims: [usize; 3]) -> NetworkResult {
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::Z],
        dims,
    };
    let samples = vec![0.5f32; dims[0] * dims[1] * dims[2]];
    let field: Arc<dyn ScalarField> =
        Arc::new(SampledField::new(grid, samples).expect("valid sampled field"));
    NetworkResult::ScalarField(field)
}

/// A closure value over an empty body, carrying `captures`.
///
/// `captures` is an `Arc<HashMap<..>>` shared with every clone of the value and
/// with every other closure built from the same environment — the reason R3
/// counts it by pointer.
fn function_with_captures(captures: HashMap<CaptureKey, NetworkResult>) -> NetworkResult {
    let mut sd = StructureDesigner::new();
    sd.add_node_network("body");
    let body: NodeNetwork = sd
        .node_type_registry
        .node_networks
        .get("body")
        .expect("body network exists")
        .clone();
    NetworkResult::Function(ZoneClosure {
        body: Arc::new(body),
        captures: Arc::new(captures),
        zone_output_wires: Arc::new(Vec::new()),
        owner_node_id: 0,
        param_types: vec![DataType::Int],
        return_type: DataType::Int,
        pre_supplied_args: Arc::new(Vec::new()),
    })
}

fn capture_key(source_node_id: u64) -> CaptureKey {
    CaptureKey {
        source_node_id,
        source_scope_depth: 0,
        source_pin: SourcePin::NodeOutput { pin_index: 0 },
    }
}

// ---------------------------------------------------------------------------
// Relations: a big payload sizes above a small one
// ---------------------------------------------------------------------------

#[test]
fn a_structure_bearing_output_sizes_above_a_scalar_one() {
    let scalar = EvalOutput::single(NetworkResult::Float(1.0));
    let molecule = EvalOutput::single(molecule_with(500));

    assert!(
        molecule.estimate_memory_bytes() > scalar.estimate_memory_bytes(),
        "a 500-atom molecule must size above a Float"
    );
}

#[test]
fn a_large_structure_sizes_above_a_small_one() {
    let small = molecule_with(2);
    let large = molecule_with(1000);

    let small_bytes = small.estimate_memory_bytes();
    let large_bytes = large.estimate_memory_bytes();

    assert!(
        large_bytes > small_bytes,
        "1000 atoms must size above 2: {large_bytes} vs {small_bytes}"
    );
    // The difference has to be dominated by the atoms themselves, or the
    // estimator is reporting struct overhead and calling it a payload.
    assert!(
        large_bytes - small_bytes > 998 * std::mem::size_of::<DVec3>(),
        "the extra 998 atoms must be visible in the estimate"
    );
}

#[test]
fn an_array_of_n_identical_elements_sizes_at_roughly_n_times_one() {
    let element = molecule_with(100);
    let one = element.estimate_memory_bytes();

    let n = 16;
    let array = NetworkResult::Array(vec![element; n]);
    let many = array.estimate_memory_bytes();

    // Stated as a band rather than an equality: the array pays one enum header
    // per slot plus its own, which is real but small next to 100 atoms.
    let ratio = many as f64 / one as f64;
    assert!(
        ratio > n as f64 * 0.9 && ratio < n as f64 * 1.5,
        "an array of {n} identical elements should size at roughly {n}x one; ratio was {ratio}"
    );
}

#[test]
fn display_results_are_counted_as_a_second_payload() {
    // A decorated structure is a full second payload sitting beside the wire
    // value, not a view of it — on an `atom_edit` root it is the larger of the
    // two. An `EvalOutput` carrying both must size above one carrying one.
    let plain = EvalOutput::single(molecule_with(400));

    let mut decorated = EvalOutput::single(molecule_with(400));
    decorated.set_display_override(0, molecule_with(400));

    assert!(
        decorated.estimate_memory_bytes() > plain.estimate_memory_bytes(),
        "a display override must be counted deeply, not skipped"
    );
}

// ---------------------------------------------------------------------------
// R2: recursion into `Array` and `Record` terminates and counts
// ---------------------------------------------------------------------------

#[test]
fn nested_containers_size_above_the_sum_of_their_scalar_leaves() {
    // A `Record` inside an `Array` inside a `Record` — the shape R2 exists for.
    let leaf_a = NetworkResult::Int(1);
    let leaf_b = NetworkResult::Float(2.0);
    let leaf_c = NetworkResult::String("hello, this string owns a heap buffer".to_string());

    let inner_record = NetworkResult::record(vec![
        ("a".to_string(), leaf_a.clone()),
        ("b".to_string(), leaf_b.clone()),
    ]);
    let array = NetworkResult::Array(vec![inner_record, leaf_c.clone()]);
    let outer = NetworkResult::record(vec![("items".to_string(), array)]);

    let leaf_sum = leaf_a.estimate_memory_bytes()
        + leaf_b.estimate_memory_bytes()
        + leaf_c.estimate_memory_bytes();

    assert!(
        outer.estimate_memory_bytes() > leaf_sum,
        "the nested containers must add their own allocations on top of the leaves"
    );
}

#[test]
fn a_record_field_with_a_large_payload_is_counted_through() {
    let small = NetworkResult::record(vec![("m".to_string(), molecule_with(2))]);
    let large = NetworkResult::record(vec![("m".to_string(), molecule_with(1000))]);

    assert!(
        large.estimate_memory_bytes() > small.estimate_memory_bytes(),
        "recursion into a record field must reach the payload"
    );
}

// ---------------------------------------------------------------------------
// R3: the two tiers for `Arc`-backed payloads
// ---------------------------------------------------------------------------

#[test]
fn a_function_value_sizes_at_pointer_cost_regardless_of_its_captures() {
    // Both closures are over the same (empty) body and differ only in how much
    // their capture map holds. Deep-counting `captures` would charge the same
    // map once per closure value alive in a pass, and it recurses back into
    // arbitrary results — so `Function` is the pointer tier.
    let tiny = function_with_captures(HashMap::new());

    let mut big_captures = HashMap::new();
    for i in 0..8u64 {
        big_captures.insert(capture_key(i), molecule_with(1000));
    }
    let heavy = function_with_captures(big_captures);

    assert_eq!(
        heavy.estimate_memory_bytes(),
        tiny.estimate_memory_bytes(),
        "two closures with identical param_types must size identically however \
         large their captures are"
    );
}

#[test]
fn a_scalar_field_value_sizes_with_its_grid() {
    // The deliberate exception to the undercount rule: one field can be
    // megabytes, and it is invisible from outside the trait.
    let small = sampled_field([4, 4, 4]);
    let large = sampled_field([40, 40, 40]);

    let small_bytes = small.estimate_memory_bytes();
    let large_bytes = large.estimate_memory_bytes();

    assert!(
        large_bytes > small_bytes,
        "a scalar field must size with its grid: {large_bytes} vs {small_bytes}"
    );
    assert!(
        large_bytes - small_bytes >= (40 * 40 * 40 - 4 * 4 * 4) * std::mem::size_of::<f32>(),
        "the estimate must cover the extra sample storage"
    );
}

#[test]
fn a_motif_and_a_structure_value_reach_their_heap() {
    let mut motif = Motif {
        parameters: Vec::new(),
        sites: Vec::new(),
        bonds: Vec::new(),
        bonds_by_site1_index: Vec::new(),
        bonds_by_site2_index: Vec::new(),
    };
    let empty_motif_bytes = NetworkResult::Motif(motif.clone()).estimate_memory_bytes();

    motif.bonds_by_site1_index = vec![vec![0usize; 8]; 128];
    assert!(NetworkResult::Motif(motif.clone()).estimate_memory_bytes() > empty_motif_bytes);

    let mut structure = Structure::diamond();
    let plain = NetworkResult::Structure(structure.clone()).estimate_memory_bytes();
    structure.motif = motif;
    assert!(NetworkResult::Structure(structure).estimate_memory_bytes() > plain);
}

// ---------------------------------------------------------------------------
// R4: the iterator exclusion is recursive where the profiler flag is not
// ---------------------------------------------------------------------------

#[test]
fn the_iterator_exclusion_recurses_where_the_profiler_flag_does_not() {
    // The profiler's `RecordFlags::produced_iterator` is a flat
    // `results.iter().any(|r| matches!(r, Iterator(_)))`. That is not
    // sufficient as the memo's skip-insert test: a stored walker pins its
    // `ZoneClosure` for the whole pass, and nesting one inside an `Array` or a
    // `Record` is representable even though no user path constructs one today.
    // This test is what distinguishes the new predicate from that flag —
    // without it the two look interchangeable and the recursion gets optimized
    // away by a later reader.
    let bare = NetworkResult::Iterator(Walker::from_array(vec![NetworkResult::Int(1)]));
    assert!(bare.contains_iterator(), "a bare Iterator is excluded");

    let in_array = NetworkResult::Array(vec![
        NetworkResult::Int(1),
        NetworkResult::Iterator(Walker::range(0, 1, 3)),
    ]);
    assert!(
        in_array.contains_iterator(),
        "an Array with an iterator element is excluded"
    );

    let in_record = NetworkResult::record(vec![
        ("count".to_string(), NetworkResult::Int(3)),
        (
            "stream".to_string(),
            NetworkResult::Iterator(Walker::range(0, 1, 3)),
        ),
    ]);
    assert!(
        in_record.contains_iterator(),
        "a Record with an iterator-valued field is excluded"
    );

    // Two levels down, through both container kinds.
    let deeply_nested = NetworkResult::record(vec![(
        "rows".to_string(),
        NetworkResult::Array(vec![NetworkResult::record(vec![(
            "stream".to_string(),
            NetworkResult::Iterator(Walker::range(0, 1, 3)),
        )])]),
    )]);
    assert!(deeply_nested.contains_iterator());
}

#[test]
fn ordinary_values_are_not_excluded() {
    assert!(!NetworkResult::Int(1).contains_iterator());
    assert!(!molecule_with(10).contains_iterator());
    assert!(!NetworkResult::Array(vec![NetworkResult::Int(1); 4]).contains_iterator());
    assert!(
        !NetworkResult::record(vec![("m".to_string(), molecule_with(4))]).contains_iterator(),
        "a record of ordinary values is storable"
    );
    // A closure cannot capture an iterator, so `Function` needs no arm.
    assert!(!function_with_captures(HashMap::new()).contains_iterator());
}

#[test]
fn the_exclusion_asks_the_whole_eval_output_including_display_overrides() {
    // The memo stores a whole `EvalOutput` under one key (D2), so the question
    // has to be asked of the whole output rather than of one pin — and
    // `display_results` is a second full payload, not a view of `results`.
    let mut output = EvalOutput::single(NetworkResult::Int(1));
    assert!(!output.contains_iterator());

    output.set_display_override(0, NetworkResult::Iterator(Walker::range(0, 1, 3)));
    assert!(
        output.contains_iterator(),
        "an iterator hiding in a display override must still exclude the entry"
    );

    let multi = EvalOutput::multi(vec![
        NetworkResult::Int(1),
        NetworkResult::Iterator(Walker::range(0, 1, 3)),
    ]);
    assert!(multi.contains_iterator());
}
