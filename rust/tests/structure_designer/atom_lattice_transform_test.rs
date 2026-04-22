use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

// After Phase 7a the four movement node types were repurposed:
//   lattice_move + atom_lmove  -> structure_move  (HasStructure input)
//   lattice_rot  + atom_lrot   -> structure_rot   (HasStructure input)
//   atom_move                  -> free_move       (HasFreeLinOps input)
//   atom_rot                   -> free_rot        (HasFreeLinOps input)
// The old Molecule+unit_cell integration tests no longer apply under the
// new abstract-typed pin design; we now only smoke-test the registrations.

#[test]
fn structure_move_registration() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("structure_move").unwrap();
    assert_eq!(node_type.name, "structure_move");
    assert!(node_type.public);
    assert_eq!(node_type.parameters.len(), 3);
    assert_eq!(node_type.parameters[0].name, "input");
    assert_eq!(node_type.parameters[1].name, "translation");
    assert_eq!(node_type.parameters[2].name, "subdivision");
}

#[test]
fn structure_rot_registration() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("structure_rot").unwrap();
    assert_eq!(node_type.name, "structure_rot");
    assert!(node_type.public);
    assert_eq!(node_type.parameters.len(), 4);
    assert_eq!(node_type.parameters[0].name, "input");
    assert_eq!(node_type.parameters[1].name, "axis_index");
    assert_eq!(node_type.parameters[2].name, "step");
    assert_eq!(node_type.parameters[3].name, "pivot_point");
}

#[test]
fn free_move_registration() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("free_move").unwrap();
    assert_eq!(node_type.name, "free_move");
    assert!(node_type.public);
    assert_eq!(node_type.parameters.len(), 2);
    assert_eq!(node_type.parameters[0].name, "input");
    assert_eq!(node_type.parameters[1].name, "translation");
}

#[test]
fn free_rot_registration() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("free_rot").unwrap();
    assert_eq!(node_type.name, "free_rot");
    assert!(node_type.public);
    assert_eq!(node_type.parameters.len(), 4);
    assert_eq!(node_type.parameters[0].name, "input");
    assert_eq!(node_type.parameters[1].name, "angle");
    assert_eq!(node_type.parameters[2].name, "rot_axis");
    assert_eq!(node_type.parameters[3].name, "pivot_point");
}

#[test]
fn old_movement_node_types_are_gone() {
    let registry = NodeTypeRegistry::new();
    assert!(registry.get_node_type("atom_move").is_none());
    assert!(registry.get_node_type("atom_rot").is_none());
    assert!(registry.get_node_type("atom_lmove").is_none());
    assert!(registry.get_node_type("atom_lrot").is_none());
    assert!(registry.get_node_type("lattice_move").is_none());
    assert!(registry.get_node_type("lattice_rot").is_none());
}
