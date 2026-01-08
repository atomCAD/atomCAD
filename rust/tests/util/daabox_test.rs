use rust_lib_flutter_cad::util::daabox::DAABox;
use glam::f64::DVec3;

#[test]
fn test_new_with_unordered_corners() {
    let box1 = DAABox::new(
        DVec3::new(10.0, 20.0, 30.0),
        DVec3::new(5.0, 15.0, 25.0)
    );
    
    assert_eq!(box1.min, DVec3::new(5.0, 15.0, 25.0));
    assert_eq!(box1.max, DVec3::new(10.0, 20.0, 30.0));
}

#[test]
fn test_from_start_and_size_negative() {
    let box1 = DAABox::from_start_and_size(
        DVec3::new(10.0, 10.0, 10.0),
        DVec3::new(-5.0, 5.0, -3.0)
    );
    
    assert_eq!(box1.min, DVec3::new(5.0, 10.0, 7.0));
    assert_eq!(box1.max, DVec3::new(10.0, 15.0, 10.0));
}

#[test]
fn test_overlaps() {
    let box1 = DAABox::from_min_max(
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(10.0, 10.0, 10.0)
    );
    
    let box2 = DAABox::from_min_max(
        DVec3::new(5.0, 5.0, 5.0),
        DVec3::new(15.0, 15.0, 15.0)
    );
    
    assert!(box1.overlaps(&box2));
    assert!(box2.overlaps(&box1));
}

#[test]
fn test_no_overlap() {
    let box1 = DAABox::from_min_max(
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(5.0, 5.0, 5.0)
    );
    
    let box2 = DAABox::from_min_max(
        DVec3::new(10.0, 10.0, 10.0),
        DVec3::new(15.0, 15.0, 15.0)
    );
    
    assert!(!box1.overlaps(&box2));
    assert!(!box2.overlaps(&box1));
}

#[test]
fn test_conservative_overlap() {
    let box1 = DAABox::from_min_max(
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(5.0, 5.0, 5.0)
    );
    
    let box2 = DAABox::from_min_max(
        DVec3::new(5.1, 5.1, 5.1),
        DVec3::new(10.0, 10.0, 10.0)
    );
    
    // Without epsilon, they don't overlap
    assert!(!box1.overlaps(&box2));
    
    // With sufficient epsilon, they do overlap conservatively
    assert!(box1.conservative_overlap(&box2, 0.2));
}

#[test]
fn test_expand() {
    let box1 = DAABox::from_min_max(
        DVec3::new(5.0, 5.0, 5.0),
        DVec3::new(10.0, 10.0, 10.0)
    );
    
    let expanded = box1.expand(1.0);
    
    assert_eq!(expanded.min, DVec3::new(4.0, 4.0, 4.0));
    assert_eq!(expanded.max, DVec3::new(11.0, 11.0, 11.0));
}

#[test]
fn test_contains_point() {
    let box1 = DAABox::from_min_max(
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(10.0, 10.0, 10.0)
    );
    
    assert!(box1.contains_point(DVec3::new(5.0, 5.0, 5.0)));
    assert!(box1.contains_point(DVec3::new(0.0, 0.0, 0.0))); // boundary
    assert!(box1.contains_point(DVec3::new(10.0, 10.0, 10.0))); // boundary
    assert!(!box1.contains_point(DVec3::new(15.0, 5.0, 5.0)));
}







