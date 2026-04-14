use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    CrystalData, MoleculeData, NetworkResult,
};

fn make_crystal() -> NetworkResult {
    NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms: AtomicStructure::new(),
        geo_tree_root: None,
    })
}

fn make_molecule() -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms: AtomicStructure::new(),
        geo_tree_root: None,
    })
}

#[test]
fn infer_data_type_crystal() {
    assert_eq!(make_crystal().infer_data_type(), Some(DataType::Crystal));
}

#[test]
fn infer_data_type_molecule() {
    assert_eq!(make_molecule().infer_data_type(), Some(DataType::Molecule));
}

#[test]
fn extract_atomic_accepts_crystal_and_molecule() {
    assert!(make_crystal().extract_atomic().is_some());
    assert!(make_molecule().extract_atomic().is_some());
    assert!(NetworkResult::Int(42).extract_atomic().is_none());
    assert!(NetworkResult::None.extract_atomic().is_none());
}

#[test]
fn extract_crystal_only_matches_crystal() {
    assert!(make_crystal().extract_crystal().is_some());
    assert!(make_molecule().extract_crystal().is_none());
    assert!(NetworkResult::Int(1).extract_crystal().is_none());
}

#[test]
fn extract_molecule_only_matches_molecule() {
    assert!(make_molecule().extract_molecule().is_some());
    assert!(make_crystal().extract_molecule().is_none());
    assert!(NetworkResult::Int(1).extract_molecule().is_none());
}

/// Runtime-guard invariant (§6.5 / OQ3): no `NetworkResult` variant should
/// ever infer an abstract data type. The evaluator's post-eval guard
/// (`evaluate_all_outputs`) replaces any value whose `infer_data_type()` is
/// abstract with `NetworkResult::Error`; this test proves the guard's
/// invariant holds for every concrete-typed variant today. If a new variant
/// is added that breaks this, the guard fires in release and asserts in debug.
#[test]
fn no_network_result_variant_infers_abstract_type() {
    let samples: Vec<NetworkResult> = vec![
        NetworkResult::None,
        NetworkResult::Bool(true),
        NetworkResult::Int(0),
        NetworkResult::Float(0.0),
        NetworkResult::String(String::new()),
        make_crystal(),
        make_molecule(),
        NetworkResult::Error("e".into()),
        NetworkResult::Array(vec![]),
    ];
    for s in samples {
        if let Some(t) = s.infer_data_type() {
            assert!(
                !t.is_abstract(),
                "NetworkResult variant unexpectedly inferred abstract type {:?}",
                t
            );
        }
    }
}
