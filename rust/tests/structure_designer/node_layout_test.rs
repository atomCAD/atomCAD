use glam::DVec2;
use rust_lib_flutter_cad::structure_designer::node_layout::{estimate_node_height, nodes_overlap};

#[test]
fn test_estimate_node_height_no_inputs() {
    // Node with 0 inputs: title(30) + output(25) + padding(8) = 63
    let height = estimate_node_height(0, false);
    assert!((height - 63.0).abs() < 0.001);
}

#[test]
fn test_estimate_node_height_with_inputs() {
    // Node with 3 inputs: title(30) + 3*22(66) + padding(8) = 104
    let height = estimate_node_height(3, false);
    assert!((height - 104.0).abs() < 0.001);
}

#[test]
fn test_estimate_node_height_with_subtitle() {
    // Node with 0 inputs and subtitle: title(30) + output(25) + subtitle(20) + padding(8) = 83
    let height = estimate_node_height(0, true);
    assert!((height - 83.0).abs() < 0.001);
}

#[test]
fn test_estimate_node_height_inputs_less_than_output() {
    // Node with 1 input: title(30) + max(22, 25)=25 + padding(8) = 63
    // 1 input = 22px, but output = 25px, so max is 25
    let height = estimate_node_height(1, false);
    assert!((height - 63.0).abs() < 0.001);
}

#[test]
fn test_estimate_node_height_inputs_more_than_output() {
    // Node with 2 inputs: title(30) + max(44, 25)=44 + padding(8) = 82
    let height = estimate_node_height(2, false);
    assert!((height - 82.0).abs() < 0.001);
}

#[test]
fn test_nodes_overlap_no_overlap() {
    let pos1 = DVec2::new(0.0, 0.0);
    let size1 = DVec2::new(100.0, 50.0);
    let pos2 = DVec2::new(150.0, 0.0); // 50 units to the right
    let size2 = DVec2::new(100.0, 50.0);

    assert!(!nodes_overlap(pos1, size1, pos2, size2, 0.0));
}

#[test]
fn test_nodes_overlap_with_gap() {
    let pos1 = DVec2::new(0.0, 0.0);
    let size1 = DVec2::new(100.0, 50.0);
    let pos2 = DVec2::new(110.0, 0.0); // 10 units gap
    let size2 = DVec2::new(100.0, 50.0);

    // No overlap with 0 gap
    assert!(!nodes_overlap(pos1, size1, pos2, size2, 0.0));
    // Overlap when requiring 20 unit gap (10 < 20)
    assert!(nodes_overlap(pos1, size1, pos2, size2, 20.0));
}

#[test]
fn test_nodes_overlap_direct_overlap() {
    let pos1 = DVec2::new(0.0, 0.0);
    let size1 = DVec2::new(100.0, 50.0);
    let pos2 = DVec2::new(50.0, 25.0); // Overlapping
    let size2 = DVec2::new(100.0, 50.0);

    assert!(nodes_overlap(pos1, size1, pos2, size2, 0.0));
}
