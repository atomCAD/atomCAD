// Regression tests: top-level wiring edits must re-validate.
//
// Validity is not a property of the one wire being edited. A polymorphic
// output (`SameAsInput`, e.g. `tag`'s HasAtoms passthrough) takes its type from
// its whole upstream chain, so connecting or deleting an ordinary data wire can
// add or remove errors anywhere in the downstream cone — which the connect-time
// type gate (`can_connect_nodes`) never looks at. Reported by mechadense: a
// `tag` stayed red after its input was fixed, until a downstream wire was
// redrawn (the delete validated) or the node was replaced.

use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;

const NET: &str = "Main";

fn setup() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NET);
    designer.set_active_node_network_name(Some(NET.to_string()));
    designer
}

fn errors(designer: &StructureDesigner) -> Vec<String> {
    designer
        .node_type_registry
        .node_networks
        .get(NET)
        .unwrap()
        .validation_errors
        .iter()
        .map(|e| format!("{:?}: {}", e.node_id, e.error_text))
        .collect()
}

fn errors_on(designer: &StructureDesigner, node_id: u64) -> Vec<String> {
    designer
        .node_type_registry
        .node_networks
        .get(NET)
        .unwrap()
        .validation_errors
        .iter()
        .filter(|e| e.node_id == Some(node_id))
        .map(|e| e.error_text.clone())
        .collect()
}

/// `atom_edit → tag → tag`, validated clean. `atom_edit` with no input
/// resolves to Molecule via its fallback.
fn chain(designer: &mut StructureDesigner) -> (u64, u64, u64) {
    let src = designer.add_node("atom_edit", DVec2::new(0.0, 0.0));
    let tag = designer.add_node("tag", DVec2::new(200.0, 0.0));
    let sink = designer.add_node("tag", DVec2::new(400.0, 0.0));
    designer.connect_nodes(src, 0, tag, 0);
    designer.connect_nodes(tag, 0, sink, 0);
    designer.validate_active_network();
    assert!(
        errors(designer).is_empty(),
        "start clean: {:?}",
        errors(designer)
    );
    (src, tag, sink)
}

#[test]
fn reconnecting_polymorphic_input_clears_unresolved_error() {
    let mut designer = setup();
    let (src, tag, _sink) = chain(&mut designer);

    assert!(designer.select_wire(src, 0, tag, 0));
    designer.delete_selected();
    // Any validating edit flags the now-unresolved chain.
    designer.validate_active_network();
    assert!(!errors_on(&designer, tag).is_empty());

    // The user fixes it: the graph is the clean starting graph again.
    designer.connect_nodes(src, 0, tag, 0);
    assert!(
        errors(&designer).is_empty(),
        "stale error after reconnecting the input: {:?}",
        errors(&designer)
    );
}

/// mechadense's workaround — delete + redraw a downstream wire. The delete
/// validates mid-redraw (downstream unresolved), so the re-connect must
/// validate too or the error just moves one node down.
#[test]
fn redrawing_downstream_wire_leaves_no_stale_error() {
    let mut designer = setup();
    let src = designer.add_node("atom_edit", DVec2::new(0.0, 0.0));
    let tag = designer.add_node("tag", DVec2::new(200.0, 0.0));
    let sink = designer.add_node("tag", DVec2::new(400.0, 0.0));
    designer.connect_nodes(tag, 0, sink, 0);
    // Errors exist (tag has no input), so the delete below validates.
    designer.validate_active_network();
    designer.connect_nodes(src, 0, tag, 0);

    assert!(designer.select_wire(tag, 0, sink, 0));
    designer.delete_selected();
    designer.connect_nodes(tag, 0, sink, 0);
    assert!(
        errors(&designer).is_empty(),
        "downstream node left stale: {:?}",
        errors(&designer)
    );
}

#[test]
fn deleting_input_wire_flags_downstream_immediately() {
    let mut designer = setup();
    let (src, tag, sink) = chain(&mut designer);

    assert!(designer.select_wire(src, 0, tag, 0));
    designer.delete_selected();
    assert!(
        !errors_on(&designer, tag).is_empty(),
        "tag is unresolved but not flagged: {:?}",
        errors(&designer)
    );
    assert!(!errors_on(&designer, sink).is_empty());
}

#[test]
fn deleting_source_node_flags_downstream_immediately() {
    let mut designer = setup();
    let (src, tag, _sink) = chain(&mut designer);

    assert!(designer.select_node(src));
    designer.delete_selected();
    assert!(
        !errors_on(&designer, tag).is_empty(),
        "tag is unresolved but not flagged: {:?}",
        errors(&designer)
    );
}

/// A connect the type gate accepts (Molecule into `tag`'s HasAtoms pin) can
/// still break a wire further down: `tag` turns Molecule, and its existing
/// wire into `exit_structure` (Crystal-only) becomes a mismatch.
#[test]
fn replacing_input_wire_flags_downstream_type_mismatch() {
    let mut designer = setup();
    let mol = designer.add_node("atom_edit", DVec2::new(0.0, 0.0));
    let enter = designer.add_node("enter_structure", DVec2::new(200.0, 0.0));
    let tag = designer.add_node("tag", DVec2::new(400.0, 0.0));
    let exit = designer.add_node("exit_structure", DVec2::new(600.0, 0.0));
    designer.connect_nodes(mol, 0, enter, 0);
    designer.connect_nodes(enter, 0, tag, 0);
    designer.connect_nodes(tag, 0, exit, 0);
    designer.validate_active_network();
    assert!(
        errors(&designer).is_empty(),
        "start clean: {:?}",
        errors(&designer)
    );

    // Bypass `enter_structure`: replaces the Crystal wire with a Molecule one.
    designer.connect_nodes(mol, 0, tag, 0);
    assert!(
        errors_on(&designer, exit)
            .iter()
            .any(|e| e.contains("Data type mismatch")),
        "downstream mismatch not flagged: {:?}",
        errors(&designer)
    );
}

/// Undo/redo of a wire edit must re-validate like the forward edit does.
#[test]
fn undo_and_redo_of_reconnect_revalidate() {
    let mut designer = setup();
    let (src, tag, sink) = chain(&mut designer);

    assert!(designer.select_wire(src, 0, tag, 0));
    designer.delete_selected();
    designer.connect_nodes(src, 0, tag, 0);
    assert!(errors(&designer).is_empty());

    // Undo the reconnect: tag's input is gone again.
    assert!(designer.undo());
    assert!(
        !errors_on(&designer, tag).is_empty() && !errors_on(&designer, sink).is_empty(),
        "undo left the unresolved chain unflagged: {:?}",
        errors(&designer)
    );

    // Redo it: the chain resolves again.
    assert!(designer.redo());
    assert!(
        errors(&designer).is_empty(),
        "redo left a stale error: {:?}",
        errors(&designer)
    );
}
