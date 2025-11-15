use rust_lib_flutter_cad::structure_designer::geo_tree::csg_cache::CsgConversionCache;
use rust_lib_flutter_cad::common::csg_types::CSGMesh;
use csgrs::traits::CSG;

#[test]
fn test_cache_creation() {
    let cache = CsgConversionCache::new(10, 20);
    assert_eq!(cache.mesh_count(), 0);
    assert_eq!(cache.sketch_count(), 0);
}

#[test]
fn test_cache_stats() {
    let mut cache = CsgConversionCache::new(10, 10);
    let hash = blake3::hash(b"test");
    
    // Miss
    assert!(cache.get_mesh(&hash).is_none());
    assert_eq!(cache.stats().mesh_misses, 1);
    assert_eq!(cache.stats().mesh_hits, 0);
    
    // Insert and hit
    cache.insert_mesh(hash, CSGMesh::new());
    assert!(cache.get_mesh(&hash).is_some());
    assert_eq!(cache.stats().mesh_hits, 1);
}

#[test]
fn test_cache_clear() {
    let mut cache = CsgConversionCache::new(10, 10);
    let hash = blake3::hash(b"test");
    
    cache.insert_mesh(hash, CSGMesh::new());
    assert_eq!(cache.mesh_count(), 1);
    
    cache.clear();
    assert_eq!(cache.mesh_count(), 0);
    assert_eq!(cache.stats().mesh_hits, 0);
}
