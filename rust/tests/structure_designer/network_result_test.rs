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
