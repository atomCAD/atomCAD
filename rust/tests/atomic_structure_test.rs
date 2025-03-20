use rust_lib_flutter_cad::common::atomic_structure::AtomicStructure;
use glam::f64::DVec3;

#[test]
fn it_gets_atoms_in_radius() {
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0), 1);
    structure.add_atom(6, DVec3::new(4.0, 0.0, 0.0), 1);
    
    assert_eq!(structure.get_atoms_in_radius(&DVec3::new(-0.5, 0.0, 0.0), 1.0).len(), 1);
    assert_eq!(structure.get_atoms_in_radius(&DVec3::new(3.5, 0.0, 0.0), 2.8).len(), 1);
    assert_eq!(structure.get_atoms_in_radius(&DVec3::new(2.0, 0.0, 0.0), 2.8).len(), 2);
    assert_eq!(structure.get_atoms_in_radius(&DVec3::new(8.0, 0.0, 0.0), 2.8).len(), 0);
}
