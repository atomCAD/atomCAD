/*
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::nodes::sphere::SphereData;
use rust_lib_flutter_cad::structure_designer::nodes::cuboid::CuboidData;
use rust_lib_flutter_cad::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use glam::f64::DVec2;
use glam::f64::DVec3;
use glam::i32::IVec3;

// cmd: cargo test
#[test]
fn it_adds_atom() {
    let mut k = StructureDesigner::new();

    assert_eq!(k.get_atomic_structure().get_num_of_atoms(), 0); 
    assert_eq!(k.get_history_size(), 0);

    let atomic_number = 6;
    let pos = DVec3::new(1.0, 2.0, 3.0);

    let atom_id = k.add_atom(atomic_number, pos);

    assert_eq!(k.get_history_size(), 1);
    assert_eq!(k.get_atomic_structure().get_num_of_atoms(), 1);

    let atom = &k.get_atomic_structure().get_atom(atom_id);
    assert!(atom.is_some());
    assert_eq!(atom.unwrap().atomic_number, atomic_number);
    assert_eq!(atom.unwrap().position, pos);
    assert_eq!(atom.unwrap().bond_ids.len(), 0);

    assert!(k.undo());

    assert_eq!(k.get_history_size(), 1);    

    assert_eq!(k.get_atomic_structure().get_num_of_atoms(), 0);
    assert!(k.redo());
    assert_eq!(k.get_atomic_structure().get_num_of_atoms(), 1);
}

#[test]
fn it_adds_atom_do_undo_do() {
    let mut k = StructureDesigner::new();

    assert_eq!(k.get_history_size(), 0);
    assert_eq!(k.get_atomic_structure().get_num_of_atoms(), 0);

    let atomic_number = 6;
    let pos = DVec3::new(1.0, 2.0, 3.0);

    k.add_atom(atomic_number, pos);

    assert_eq!(k.get_history_size(), 1);
    assert_eq!(k.get_atomic_structure().get_num_of_atoms(), 1);

    assert!(k.undo());

    assert_eq!(k.get_history_size(), 1);
    assert_eq!(k.get_atomic_structure().get_num_of_atoms(), 0);

    k.add_atom(atomic_number, pos);

    assert_eq!(k.get_history_size(), 1);
    assert_eq!(k.get_atomic_structure().get_num_of_atoms(), 1);
}

#[test]
fn it_adds_bond() {
  let mut k = StructureDesigner::new();

  let atom_id1 = k.add_atom(6, DVec3::new(1.0, 2.0, 3.0));
  let atom_id2 = k.add_atom(8, DVec3::new(2.0, 1.0, 1.0));

  let multiplicity: i32 = 2;
  let bond_id = k.add_bond(atom_id1, atom_id2, multiplicity);

  assert_eq!(k.get_history_size(), 3);
  assert_eq!(k.get_atomic_structure().get_num_of_bonds(), 1);

  let bond = &k.get_atomic_structure().get_bond(bond_id);
  assert!(bond.is_some());
  assert_eq!(bond.unwrap().atom_id1, atom_id1);
  assert_eq!(bond.unwrap().atom_id2, atom_id2);
  assert_eq!(bond.unwrap().multiplicity, multiplicity);

  assert!(k.undo());    

  assert_eq!(k.get_atomic_structure().get_num_of_bonds(), 0);
  assert!(k.redo());
  assert_eq!(k.get_atomic_structure().get_num_of_bonds(), 1);
}

#[test]
fn it_selects_an_atom() {
  let mut k = StructureDesigner::new();

  let atom_id1 = k.add_atom(6, DVec3::new(1.0, 2.0, 3.0));
  let atom_id2 = k.add_atom(8, DVec3::new(2.0, 1.0, 1.0));

  k.select(vec!(atom_id2), vec!(), false);

  let atom1 = &k.get_atomic_structure().get_atom(atom_id1);
  assert!(atom1.is_some());
  assert!(!atom1.unwrap().selected);  

  let atom2 = &k.get_atomic_structure().get_atom(atom_id2);
  assert!(atom2.is_some());
  assert!(atom2.unwrap().selected);  
}

#[test]
fn it_selects_a_bond() {
  let mut k = StructureDesigner::new();

  let atom_id1 = k.add_atom(6, DVec3::new(1.0, 2.0, 3.0));
  let atom_id2 = k.add_atom(8, DVec3::new(2.0, 1.0, 1.0));
  let atom_id3 = k.add_atom(6, DVec3::new(5.0, 5.0, 5.0));

  let bond_id1 = k.add_bond(atom_id1, atom_id2, 1);
  let bond_id2 = k.add_bond(atom_id2, atom_id3, 1);

  k.select(vec!(), vec!(bond_id2), false);

  let bond1 = &k.get_atomic_structure().get_bond(bond_id1);
  assert!(bond1.is_some());
  assert!(!bond1.unwrap().selected);  

  let bond2 = &k.get_atomic_structure().get_bond(bond_id2);
  assert!(bond2.is_some());
  assert!(bond2.unwrap().selected);  
}

const EPSILON: f64 = 1e-5;

#[test]
fn test_kernel_sphere_evaluation() {
    let mut k = StructureDesigner::new();
    
    // Create a test network
    k.add_node_network("test_network");
    
    // Add a sphere node (default: center at origin, radius 1)
    let sphere_node = k.add_node("test_network", "sphere", DVec2::new(0.0, 0.0));
    
    // Get the evaluator and test evaluation
    let evaluator = k.get_network_evaluator();
    let registry = &k.node_type_registry;
    let network = registry.node_networks.get("test_network").unwrap();

    // Test points:
    // 1. Point on surface (should be 0)
    let surface_point = DVec3::new(1.0, 0.0, 0.0);
    let surface_result = evaluator.implicit_evaluator.eval(&network, sphere_node, &surface_point, registry);
    assert!((surface_result[0]).abs() < EPSILON);
    
    // 2. Point inside sphere (should be negative)
    let inside_point = DVec3::new(0.5, 0.0, 0.0);
    let inside_result = evaluator.implicit_evaluator.eval(&network, sphere_node, &inside_point, registry);
    assert!(inside_result[0] < -EPSILON);
    
    // 3. Point outside sphere (should be positive)
    let outside_point = DVec3::new(2.0, 0.0, 0.0);
    let outside_result = evaluator.implicit_evaluator.eval(&network, sphere_node, &outside_point, registry);
    assert!(outside_result[0] > EPSILON);
}

#[test]
fn test_kernel_cuboid_evaluation() {
    let mut k = StructureDesigner::new();
    
    // Create a test network
    k.add_node_network("test_network");
    
    // Add a cuboid node (default: start at (-1, -1, -1), size: (2, 2, 2))
    let cuboid_node = k.add_node("test_network", "cuboid", DVec2::new(0.0, 0.0));

    // Get the evaluator and test evaluation
    let evaluator = k.get_network_evaluator();
    let registry = &k.node_type_registry;
    let network = registry.node_networks.get("test_network").unwrap();
    let mut network_stack = Vec::new();
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

    // Test points:
    // 1. Point on surface (should be 0)
    let surface_point = DVec3::new(1.0, 0.0, 0.0);
    let surface_result = evaluator.implicit_evaluator.eval(&network, cuboid_node, &surface_point, registry);
    assert!((surface_result[0]).abs() < EPSILON);
    
    // 2. Point inside cuboid (should be negative)
    let inside_point = DVec3::new(0.5, 0.0, 0.0);
    let inside_result = evaluator.implicit_evaluator.eval(&network, cuboid_node, &inside_point, registry);
    assert!(inside_result[0] < -EPSILON);
    
    // 3. Point outside sphere (should be positive)
    let outside_point = DVec3::new(2.0, 0.0, 0.0);
    let outside_result = evaluator.implicit_evaluator.eval(&network, cuboid_node, &outside_point, registry);
    assert!(outside_result[0] > EPSILON);
}

#[test]
fn test_kernel_union_of_spheres() {
    let mut k = StructureDesigner::new();
    
    // Create a test network
    k.add_node_network("test_network");
    
    // Add two sphere nodes (both radius 1, one at origin, one at (2,0,0))
    let sphere1_node = k.add_node("test_network", "sphere", DVec2::new(0.0, 0.0));
    let sphere2_node = k.add_node("test_network", "sphere", DVec2::new(2.0, 0.0));
    let union_node = k.add_node("test_network", "union", DVec2::new(1.0, 1.0));
    
    k.set_node_network_data("test_network", sphere1_node, Box::new(SphereData {
        center: IVec3::new(0, 0, 0),
        radius: 1,
    }));
    
    k.set_node_network_data("test_network", sphere2_node, Box::new(SphereData {
        center: IVec3::new(2, 0, 0),
        radius: 2,
    }));
    
    // Connect nodes to union
    k.connect_nodes("test_network", sphere1_node, union_node, 0);
    k.connect_nodes("test_network", sphere2_node, union_node, 0);
    
    let evaluator = k.get_network_evaluator();
    let registry = &k.node_type_registry;
    let network = registry.node_networks.get("test_network").unwrap();
    let mut network_stack = Vec::new();
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

    // Test points:
    // 1. Point inside both spheres (should be negative)
    let inside_point = DVec3::new(0.5, 0.0, 0.0);
    let inside_result = evaluator.implicit_evaluator.eval(&network, union_node, &inside_point, registry);
    assert!(inside_result[0] < -EPSILON);

    // 1. Point inside only second sphere (should be negative)
    let inside_2_point = DVec3::new(2.5, 0.0, 0.0);
    let inside_2_result = evaluator.implicit_evaluator.eval(&network, union_node, &inside_2_point, registry);
    assert!(inside_2_result[0] < -EPSILON);

    // 3. Point outside both spheres (should be positive)
    let outside_point = DVec3::new(5.0, 0.0, 0.0);
    let outside_result = evaluator.implicit_evaluator.eval(&network, union_node, &outside_point, registry);
    assert!(outside_result[0] > EPSILON);
}

#[test]
fn test_kernel_intersection_with_half_space() {
    let mut k = StructureDesigner::new();
    
    // Create a test network
    k.add_node_network("test_network");
    
    // Add a sphere node (radius 1 at origin) and half-space node (normal along x-axis)
    // Their intersection is a half sphere (the negative x-axis half sphere)
    let sphere_node = k.add_node("test_network", "sphere", DVec2::new(0.0, 0.0));
    let half_space_node = k.add_node("test_network", "half_space", DVec2::new(2.0, 0.0));
    let intersect_node = k.add_node("test_network", "intersect", DVec2::new(1.0, 1.0));
    
    // Connect the nodes
    k.connect_nodes("test_network", 
        sphere_node, 
        intersect_node, 
        0);
    k.connect_nodes("test_network", 
        half_space_node, 
        intersect_node, 
        0);
    
    let evaluator = k.get_network_evaluator();
    let registry = &k.node_type_registry;
    let network = registry.node_networks.get("test_network").unwrap();
    let mut network_stack = Vec::new();
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

    // Test points:
    // 1. Point outside both shapes (should be positive)
    let outside_point = DVec3::new(3.0, 0.0, 0.0);
    let outside_result = evaluator.implicit_evaluator.eval(&network, intersect_node, &outside_point, registry);
    assert!(outside_result[0] > EPSILON);

    // 2. Point inside both shapes (should be negative)
    let inside_both = DVec3::new(-0.5, 0.0, 0.0);
    let inside_both_result = evaluator.implicit_evaluator.eval(&network, intersect_node, &inside_both, registry);
    assert!(inside_both_result[0] < -EPSILON);

    // 3. Point inside sphere but outside half-space (should be positive)
    let inside_sphere_outside_half = DVec3::new(0.5, 0.0, 0.0);
    let mixed_result = evaluator.implicit_evaluator.eval(&network, intersect_node, &inside_sphere_outside_half, registry);
    assert!(mixed_result[0] > EPSILON);
}

#[test]
fn test_kernel_complex_csg_network() {
    let mut k = StructureDesigner::new();
    
    // Create a test network
    k.add_node_network("test_network");
    
    // Add nodes: two spheres and a cuboid
    let sphere1_node = k.add_node("test_network", "sphere", DVec2::new(0.0, 0.0));
    let sphere2_node = k.add_node("test_network", "sphere", DVec2::new(1.0, 0.0));
    let cuboid_node = k.add_node("test_network", "cuboid", DVec2::new(0.0, 0.0));
    let union1_node = k.add_node("test_network", "union", DVec2::new(0.0, 1.0));
    let intersect_node = k.add_node("test_network", "intersect", DVec2::new(0.0, 2.0));
    
    // Set sphere data
    k.set_node_network_data("test_network", sphere1_node, Box::new(SphereData {
        center: IVec3::new(0, 0, 0),
        radius: 1,
    }));
    
    k.set_node_network_data("test_network", sphere2_node, Box::new(SphereData {
        center: IVec3::new(2, 0, 0),
        radius: 1,
    }));
    
    // Set cuboid data
    k.set_node_network_data("test_network", cuboid_node, Box::new(CuboidData {
        min_corner: IVec3::new(-1, -1, -1),
        extent: IVec3::new(4, 2, 2),  // extends from -1 to 3 in x
    }));
    
    // Connect nodes: (sphere1 ∪ sphere2) ∩ cuboid
    k.connect_nodes("test_network", sphere1_node, union1_node, 0);
    k.connect_nodes("test_network", sphere2_node, union1_node, 0);
    k.connect_nodes("test_network", union1_node, intersect_node, 0);
    k.connect_nodes("test_network", cuboid_node, intersect_node, 0);
    
    let evaluator = k.get_network_evaluator();
    let registry = &k.node_type_registry;
    let network = registry.node_networks.get("test_network").unwrap();
    let mut network_stack = Vec::new();
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

    // Test points:
    // 1. Point inside first sphere and cuboid (should be negative)
    let inside_point = DVec3::new(0.0, 0.0, 0.0);
    let inside_result = evaluator.implicit_evaluator.eval(&network, intersect_node, &inside_point, registry);
    assert!(inside_result[0] < -EPSILON);
    
    // 2. Point outside spheres but inside cuboid (should be positive since it's outside the union)
    let outside_spheres = DVec3::new(0.0, 0.0, 2.0);
    let outside_spheres_result = evaluator.implicit_evaluator.eval(&network, intersect_node, &outside_spheres, registry);
    assert!(outside_spheres_result[0] > EPSILON);
}
*/