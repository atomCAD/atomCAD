use glam::f64::DVec3;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::geo_tree::batched_implicit_evaluator::BatchedImplicitEvaluator;

#[test]
fn test_multi_threaded_evaluation() {
    // Create a simple sphere for testing
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);

    // Create evaluators with different threading configurations
    let mut single_threaded = BatchedImplicitEvaluator::new(&sphere);
    let mut multi_threaded = BatchedImplicitEvaluator::new_with_threading(&sphere, true);

    // Add enough points to trigger multi-threading (> MIN_POINTS_FOR_THREADING = 2048)
    let test_points: Vec<DVec3> = (0..3000)
        .map(|i| {
            let angle = i as f64 * 0.01;
            DVec3::new(angle.cos(), angle.sin(), 0.0)
        })
        .collect();

    // Add points to both evaluators
    for point in &test_points {
        single_threaded.add_point(*point);
        multi_threaded.add_point(*point);
    }

    // Evaluate with both methods
    let single_results = single_threaded.flush();
    let multi_results = multi_threaded.flush();

    // Results should be identical
    assert_eq!(single_results.len(), multi_results.len());
    for (i, (single, multi)) in single_results.iter().zip(multi_results.iter()).enumerate() {
        assert!(
            (single - multi).abs() < 1e-10,
            "Point {}: Single-threaded and multi-threaded results should be identical: {} vs {}",
            i,
            single,
            multi
        );
    }
}

#[test]
fn test_threading_configuration() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);

    // Test single-threaded by default
    let single_threaded = BatchedImplicitEvaluator::new(&sphere);
    assert_eq!(single_threaded.pending_count(), 0);

    // Test explicit threading configuration
    let multi_threaded = BatchedImplicitEvaluator::new_with_threading(&sphere, true);
    assert_eq!(multi_threaded.pending_count(), 0);

    let single_threaded_explicit = BatchedImplicitEvaluator::new_with_threading(&sphere, false);
    assert_eq!(single_threaded_explicit.pending_count(), 0);
}

#[test]
fn test_fallback_to_single_threaded() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);

    // Create multi-threaded evaluator but with small workload
    let mut evaluator = BatchedImplicitEvaluator::new_with_threading(&sphere, true);

    // Add only a few points (less than MIN_POINTS_FOR_THREADING)
    for i in 0..10 {
        evaluator.add_point(DVec3::new(i as f64 * 0.1, 0.0, 0.0));
    }

    // Should fall back to single-threaded evaluation
    let results = evaluator.flush();
    assert_eq!(results.len(), 10);

    // Verify results are correct
    for (i, &result) in results.iter().enumerate() {
        let expected = (i as f64 * 0.1) - 1.0; // Distance to sphere surface
        assert!(
            (result - expected).abs() < 1e-10,
            "Point {}: expected {}, got {}",
            i,
            expected,
            result
        );
    }
}

#[test]
fn test_complex_geometry_multi_threaded() {
    // Create a more complex geometry tree
    let sphere1 = Box::new(GeoNode::sphere(DVec3::new(-0.5, 0.0, 0.0), 0.8));

    let sphere2 = Box::new(GeoNode::sphere(DVec3::new(0.5, 0.0, 0.0), 0.8));

    let union = GeoNode::union_3d(vec![*sphere1, *sphere2]);

    // Test with both single and multi-threaded evaluation
    let mut single_threaded = BatchedImplicitEvaluator::new(&union);
    let mut multi_threaded = BatchedImplicitEvaluator::new_with_threading(&union, true);

    // Add enough points to trigger multi-threading
    let test_points: Vec<DVec3> = (0..2500)
        .map(|i| {
            let x = (i as f64 / 1000.0) - 1.25; // Points from -1.25 to 1.25
            DVec3::new(x, 0.0, 0.0)
        })
        .collect();

    // Add points to both evaluators
    for point in &test_points {
        single_threaded.add_point(*point);
        multi_threaded.add_point(*point);
    }

    // Evaluate with both methods
    let single_results = single_threaded.flush();
    let multi_results = multi_threaded.flush();

    // Results should be identical
    assert_eq!(single_results.len(), multi_results.len());
    for (i, (single, multi)) in single_results.iter().zip(multi_results.iter()).enumerate() {
        assert!(
            (single - multi).abs() < 1e-10,
            "Point {}: Complex geometry results should be identical: {} vs {}",
            i,
            single,
            multi
        );
    }
}
