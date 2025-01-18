use rust_lib_flutter_cad::api::kernel::kernel::Kernel;
use glam::f32::Vec3;

// cmd: cargo test
#[test]
fn it_adds_atom() {
    let mut k = Kernel::new();

    assert_eq!(k.get_model().get_num_of_atoms(), 0); 
    assert_eq!(k.get_history_size(), 0);

    let atomic_number = 6;
    let pos = Vec3::new(1.0, 2.0, 3.0);

    let atom_id = k.add_atom(atomic_number, pos);

    assert_eq!(k.get_history_size(), 1);
    assert_eq!(k.get_model().get_num_of_atoms(), 1);

    let atom = &k.get_model().get_atom(atom_id);
    assert!(atom.is_some());
    assert_eq!(atom.unwrap().atomic_number, atomic_number);
    assert_eq!(atom.unwrap().position, pos);
    assert_eq!(atom.unwrap().bond_ids.len(), 0);

    assert!(k.undo());

    assert_eq!(k.get_history_size(), 1);    

    assert_eq!(k.get_model().get_num_of_atoms(), 0);
    assert!(k.redo());
    assert_eq!(k.get_model().get_num_of_atoms(), 1);
}

#[test]
fn it_adds_atom_do_undo_do() {
    let mut k = Kernel::new();

    assert_eq!(k.get_history_size(), 0);
    assert_eq!(k.get_model().get_num_of_atoms(), 0);

    let atomic_number = 6;
    let pos = Vec3::new(1.0, 2.0, 3.0);

    k.add_atom(atomic_number, pos);

    assert_eq!(k.get_history_size(), 1);
    assert_eq!(k.get_model().get_num_of_atoms(), 1);

    assert!(k.undo());

    assert_eq!(k.get_history_size(), 1);
    assert_eq!(k.get_model().get_num_of_atoms(), 0);

    k.add_atom(atomic_number, pos);

    assert_eq!(k.get_history_size(), 1);
    assert_eq!(k.get_model().get_num_of_atoms(), 1);
}

#[test]
fn it_adds_bond() {
  let mut k = Kernel::new();

  let atom_id1 = k.add_atom(6, Vec3::new(1.0, 2.0, 3.0));
  let atom_id2 = k.add_atom(8, Vec3::new(2.0, 1.0, 1.0));

  let multiplicity: i32 = 2;
  let bond_id = k.add_bond(atom_id1, atom_id2, multiplicity);

  assert_eq!(k.get_history_size(), 3);
  assert_eq!(k.get_model().get_num_of_bonds(), 1);

  let bond = &k.get_model().get_bond(bond_id);
  assert!(bond.is_some());
  assert_eq!(bond.unwrap().atom_id1, atom_id1);
  assert_eq!(bond.unwrap().atom_id2, atom_id2);
  assert_eq!(bond.unwrap().multiplicity, multiplicity);

  assert!(k.undo());    

  assert_eq!(k.get_model().get_num_of_bonds(), 0);
  assert!(k.redo());
  assert_eq!(k.get_model().get_num_of_bonds(), 1);
}

#[test]
fn it_selects_an_atom() {
  let mut k = Kernel::new();

  let atom_id1 = k.add_atom(6, Vec3::new(1.0, 2.0, 3.0));
  let atom_id2 = k.add_atom(8, Vec3::new(2.0, 1.0, 1.0));

  k.select(vec!(atom_id2), vec!(), false);

  let atom1 = &k.get_model().get_atom(atom_id1);
  assert!(atom1.is_some());
  assert!(!atom1.unwrap().selected);  

  let atom2 = &k.get_model().get_atom(atom_id2);
  assert!(atom2.is_some());
  assert!(atom2.unwrap().selected);  
}

#[test]
fn it_selects_a_bond() {
  let mut k = Kernel::new();

  let atom_id1 = k.add_atom(6, Vec3::new(1.0, 2.0, 3.0));
  let atom_id2 = k.add_atom(8, Vec3::new(2.0, 1.0, 1.0));
  let atom_id3 = k.add_atom(6, Vec3::new(5.0, 5.0, 5.0));

  let bond_id1 = k.add_bond(atom_id1, atom_id2, 1);
  let bond_id2 = k.add_bond(atom_id2, atom_id3, 1);

  k.select(vec!(), vec!(bond_id2), false);

  let bond1 = &k.get_model().get_bond(bond_id1);
  assert!(bond1.is_some());
  assert!(!bond1.unwrap().selected);  

  let bond2 = &k.get_model().get_bond(bond_id2);
  assert!(bond2.is_some());
  assert!(bond2.unwrap().selected);  
}
