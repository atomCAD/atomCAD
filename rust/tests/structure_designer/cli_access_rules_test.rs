use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

#[test]
fn test_default_unlocked() {
    let sd = StructureDesigner::new();
    assert!(!sd.is_cli_write_locked("Foo"));
    assert!(!sd.is_cli_write_locked("Foo.Bar"));
    assert!(!sd.is_cli_write_locked("Anything"));
}

#[test]
fn test_lock_namespace() {
    let mut sd = StructureDesigner::new();
    sd.set_cli_access("Physics", false);

    assert!(sd.is_cli_write_locked("Physics"));
    assert!(sd.is_cli_write_locked("Physics.Mechanics"));
    assert!(sd.is_cli_write_locked("Physics.Mechanics.Spring"));
    assert!(!sd.is_cli_write_locked("Math"));
    assert!(!sd.is_cli_write_locked("PhysicsExtra")); // Not a child of "Physics"
}

#[test]
fn test_lock_then_unlock_child() {
    let mut sd = StructureDesigner::new();
    sd.set_cli_access("Physics", false);
    sd.set_cli_access("Physics.Mechanics", true);

    // Physics subtree is locked except Mechanics
    assert!(sd.is_cli_write_locked("Physics"));
    assert!(sd.is_cli_write_locked("Physics.Optics"));
    assert!(sd.is_cli_write_locked("Physics.Optics.Lens"));
    assert!(!sd.is_cli_write_locked("Physics.Mechanics"));
    assert!(!sd.is_cli_write_locked("Physics.Mechanics.Spring"));
}

#[test]
fn test_lock_leaf_network() {
    let mut sd = StructureDesigner::new();
    sd.set_cli_access("Physics.Mechanics.Spring", false);

    assert!(sd.is_cli_write_locked("Physics.Mechanics.Spring"));
    assert!(!sd.is_cli_write_locked("Physics.Mechanics"));
    assert!(!sd.is_cli_write_locked("Physics.Mechanics.Pendulum"));
    assert!(!sd.is_cli_write_locked("Physics"));
}

#[test]
fn test_prune_descendants_on_set() {
    let mut sd = StructureDesigner::new();

    // Create fine-grained rules
    sd.set_cli_access("Physics", false);
    sd.set_cli_access("Physics.Mechanics", true);
    sd.set_cli_access("Physics.Mechanics.Spring", false);

    assert_eq!(sd.get_cli_access_rules().len(), 3);

    // Now set Physics again — should prune all descendants
    sd.set_cli_access("Physics", false);
    assert_eq!(sd.get_cli_access_rules().len(), 1);
    assert!(sd.is_cli_write_locked("Physics.Mechanics"));
    assert!(sd.is_cli_write_locked("Physics.Mechanics.Spring"));
}

#[test]
fn test_allow_after_deny() {
    let mut sd = StructureDesigner::new();
    sd.set_cli_access("Physics", false);
    assert!(sd.is_cli_write_locked("Physics.Mechanics"));

    sd.set_cli_access("Physics", true);
    assert!(!sd.is_cli_write_locked("Physics.Mechanics"));
}

#[test]
fn test_longest_prefix_wins() {
    let mut sd = StructureDesigner::new();
    sd.set_cli_access("A", false);
    sd.set_cli_access("A.B", true);
    sd.set_cli_access("A.B.C", false);

    assert!(sd.is_cli_write_locked("A"));
    assert!(sd.is_cli_write_locked("A.X"));
    assert!(!sd.is_cli_write_locked("A.B"));
    assert!(!sd.is_cli_write_locked("A.B.Y"));
    assert!(sd.is_cli_write_locked("A.B.C"));
    assert!(sd.is_cli_write_locked("A.B.C.D"));
}

#[test]
fn test_clear_cli_access() {
    let mut sd = StructureDesigner::new();
    sd.set_cli_access("Physics", false);
    sd.set_cli_access("Physics.Mechanics", true);

    // Clear the override on Mechanics — should inherit from Physics (locked)
    sd.clear_cli_access("Physics.Mechanics");
    assert!(sd.is_cli_write_locked("Physics.Mechanics"));
    assert!(sd.is_cli_write_locked("Physics.Mechanics.Spring"));
}

#[test]
fn test_marks_dirty() {
    let mut sd = StructureDesigner::new();
    sd.is_dirty = false;
    sd.set_cli_access("Foo", false);
    assert!(sd.is_dirty);
}
