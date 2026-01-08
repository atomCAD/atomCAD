use rust_lib_flutter_cad::geo_tree::batched_implicit_evaluator::BatchedImplicitEvaluator;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::implicit_eval::implicit_geometry::{ImplicitGeometry3D, ImplicitGeometry2D, BATCH_SIZE};
use glam::f64::{DVec2, DVec3};

#[test]
fn test_batched_evaluator_basic() {
    // Create a simple sphere for testing
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);
    
    let mut evaluator = BatchedImplicitEvaluator::new(&sphere);
    
    // Add some test points
    let idx1 = evaluator.add_point(DVec3::new(0.0, 0.0, 0.0)); // center - should be negative
    let idx2 = evaluator.add_point(DVec3::new(2.0, 0.0, 0.0)); // outside - should be positive
    
    assert_eq!(idx1, 0);
    assert_eq!(idx2, 1);
    assert_eq!(evaluator.pending_count(), 2);
    
    // Evaluate batch
    let results = evaluator.flush();
    
    assert_eq!(results.len(), 2);
    assert!(results[0] < 0.0); // Inside sphere
    assert!(results[1] > 0.0); // Outside sphere
    assert_eq!(evaluator.pending_count(), 0);
}

#[test]
fn test_immediate_evaluation() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);
    
    let evaluator = BatchedImplicitEvaluator::new(&sphere);
    
    let result = evaluator.eval_immediate(&DVec3::ZERO);
    assert!(result < 0.0); // Inside sphere
}

#[test]
fn test_empty_flush() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);
    
    let mut evaluator = BatchedImplicitEvaluator::new(&sphere);
    let results = evaluator.flush();
    
    assert!(results.is_empty());
}

#[test]
fn test_large_batch() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);
    
    let mut evaluator = BatchedImplicitEvaluator::new(&sphere);
    
    // Add more points than BATCH_SIZE to test padding
    let num_points = 2500; // More than 2 * BATCH_SIZE (2048)
    let mut expected_results = Vec::new();
    
    for i in 0..num_points {
        let x = (i as f64) / 1000.0; // Points from 0.0 to 2.5
        let point = DVec3::new(x, 0.0, 0.0);
        evaluator.add_point(point);
        
        // Calculate expected result (distance to sphere surface)
        expected_results.push(x - 1.0);
    }
    
    let results = evaluator.flush();
    
    assert_eq!(results.len(), num_points);
    
    // Check that results are approximately correct
    for (i, &result) in results.iter().enumerate() {
        let expected = expected_results[i];
        assert!((result - expected).abs() < 1e-10, 
                "Point {}: expected {}, got {}", i, expected, result);
    }
}

#[test]
fn test_utility_methods() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);
    
    let mut evaluator = BatchedImplicitEvaluator::new(&sphere);
    
    // Initially empty
    assert_eq!(evaluator.pending_count(), 0);
    assert!(!evaluator.has_pending());
    
    // Add some points
    evaluator.add_point(DVec3::ZERO);
    evaluator.add_point(DVec3::new(1.0, 0.0, 0.0));
    
    assert_eq!(evaluator.pending_count(), 2);
    assert!(evaluator.has_pending());
    
    // Clear without evaluating
    evaluator.clear();
    
    assert_eq!(evaluator.pending_count(), 0);
    assert!(!evaluator.has_pending());
}

#[test]
fn test_direct_2d_batch() {
    let circle = GeoNode::circle(DVec2::ZERO, 1.0);
    
    // Test direct 2D batch evaluation
    let mut batch_points = [DVec2::ZERO; BATCH_SIZE];
    let mut batch_results = [0.0; BATCH_SIZE];
    
    // Fill with test points
    for i in 0..10 {
        batch_points[i] = DVec2::new(i as f64 * 0.1, 0.0); // Points from 0.0 to 0.9
    }
    
    // Evaluate 2D batch inplace
    circle.implicit_eval_2d_batch(&batch_points, &mut batch_results);
    
    // Check first few results
    for i in 0..10 {
        let expected = (i as f64 * 0.1) - 1.0; // Distance to circle edge
        assert!((batch_results[i] - expected).abs() < 1e-10, 
                "Point {}: expected {}, got {}", i, expected, batch_results[i]);
    }
}

#[test]
fn test_direct_inplace_batch() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);
    
    // Test direct inplace batch evaluation
    let mut batch_points = [DVec3::ZERO; BATCH_SIZE];
    let mut batch_results = [0.0; BATCH_SIZE];
    
    // Fill with test points
    for i in 0..10 {
        batch_points[i] = DVec3::new(i as f64 * 0.1, 0.0, 0.0); // Points from 0.0 to 0.9
    }
    
    // Evaluate inplace
    sphere.implicit_eval_3d_batch(&batch_points, &mut batch_results);
    
    // Check first few results
    for i in 0..10 {
        let expected = (i as f64 * 0.1) - 1.0; // Distance to sphere surface
        assert!((batch_results[i] - expected).abs() < 1e-10, 
                "Point {}: expected {}, got {}", i, expected, batch_results[i]);
    }
}







