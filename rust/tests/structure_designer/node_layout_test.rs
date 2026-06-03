use glam::DVec2;
use rust_lib_flutter_cad::structure_designer::node_layout::{
    estimate_hof_node_size, estimate_node_height, nodes_overlap,
};

#[test]
fn test_estimate_node_height_no_inputs() {
    // Node with 0 inputs: title(30) + output(25) + padding(8) = 63
    let height = estimate_node_height(0, 1, false);
    assert!((height - 63.0).abs() < 0.001);
}

#[test]
fn test_estimate_node_height_with_inputs() {
    // Node with 3 inputs: title(30) + 3*22(66) + padding(8) = 104
    let height = estimate_node_height(3, 1, false);
    assert!((height - 104.0).abs() < 0.001);
}

#[test]
fn test_estimate_node_height_with_subtitle() {
    // Node with 0 inputs and subtitle: title(30) + output(25) + subtitle(20) + padding(8) = 83
    let height = estimate_node_height(0, 1, true);
    assert!((height - 83.0).abs() < 0.001);
}

#[test]
fn test_estimate_node_height_inputs_less_than_output() {
    // Node with 1 input: title(30) + max(22, 25)=25 + padding(8) = 63
    // 1 input = 22px, but output = 25px, so max is 25
    let height = estimate_node_height(1, 1, false);
    assert!((height - 63.0).abs() < 0.001);
}

#[test]
fn test_estimate_node_height_inputs_more_than_output() {
    // Node with 2 inputs: title(30) + max(44, 25)=44 + padding(8) = 82
    let height = estimate_node_height(2, 1, false);
    assert!((height - 82.0).abs() < 0.001);
}

#[test]
fn test_estimate_hof_node_size_body_dominates() {
    // A four-HOF (map) with 1 external in, 1 out, 1 zone-in, 1 zone-out and a
    // 600x400 body. Width = 70(left) + 600 + 70(right) = 740. Height =
    // title(30) + max(pins…, body 400, 25) + subtitle(20) + padding(8) = 458.
    let size = estimate_hof_node_size(1, 1, 1, 1, 600.0, 400.0, true, false);
    assert!((size.x - 740.0).abs() < 0.001, "width {}", size.x);
    assert!((size.y - 458.0).abs() < 0.001, "height {}", size.y);
}

#[test]
fn test_estimate_hof_node_size_closure_trims_chrome() {
    // The closure node uses 16px side pads instead of 70px columns.
    // Width = 16 + 300 + 16 = 332.
    let size = estimate_hof_node_size(0, 0, 1, 1, 300.0, 100.0, true, true);
    assert!((size.x - 332.0).abs() < 0.001, "width {}", size.x);
}

#[test]
fn test_estimate_hof_node_size_pins_can_exceed_body() {
    // With a tiny body, the tallest pin column drives the mid-band: 5 zone-out
    // pins * 22 = 110. Height = 30 + 110 + 20 + 8 = 168.
    let size = estimate_hof_node_size(1, 1, 1, 5, 100.0, 10.0, true, false);
    assert!((size.y - 168.0).abs() < 0.001, "height {}", size.y);
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
