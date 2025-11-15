use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use crate::common::csg_types::{CSGMesh, CSGSketch};

/// Statistics for cache performance monitoring
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub mesh_hits: u64,
    pub mesh_misses: u64,
    pub sketch_hits: u64,
    pub sketch_misses: u64,
}

impl CacheStats {
    /// Calculate mesh hit rate (0.0 to 1.0)
    pub fn mesh_hit_rate(&self) -> f64 {
        let total = self.mesh_hits + self.mesh_misses;
        if total == 0 {
            0.0
        } else {
            self.mesh_hits as f64 / total as f64
        }
    }

    /// Calculate sketch hit rate (0.0 to 1.0)
    pub fn sketch_hit_rate(&self) -> f64 {
        let total = self.sketch_hits + self.sketch_misses;
        if total == 0 {
            0.0
        } else {
            self.sketch_hits as f64 / total as f64
        }
    }

    /// Get total number of cache lookups
    pub fn total_lookups(&self) -> u64 {
        self.mesh_hits + self.mesh_misses + self.sketch_hits + self.sketch_misses
    }
}

/// Cache for CSG conversion results with LRU eviction policy
pub struct CsgConversionCache {
    mesh_cache: LruCache<blake3::Hash, Arc<CSGMesh>>,
    sketch_cache: LruCache<blake3::Hash, Arc<CSGSketch>>,
    stats: CacheStats,
}

impl CsgConversionCache {
    /// Create a new cache with specified capacities (number of entries)
    /// 
    /// # Arguments
    /// * `mesh_capacity` - Maximum number of mesh entries (default: 200)
    /// * `sketch_capacity` - Maximum number of sketch entries (default: 500)
    pub fn new(mesh_capacity: usize, sketch_capacity: usize) -> Self {
        Self {
            mesh_cache: LruCache::new(NonZeroUsize::new(mesh_capacity).unwrap()),
            sketch_cache: LruCache::new(NonZeroUsize::new(sketch_capacity).unwrap()),
            stats: CacheStats::default(),
        }
    }

    /// Create a cache with default capacities
    pub fn with_defaults() -> Self {
        Self::new(200, 500)
    }

    /// Get a cached mesh, returns None if not found
    pub fn get_mesh(&mut self, hash: &blake3::Hash) -> Option<Arc<CSGMesh>> {
        if let Some(mesh) = self.mesh_cache.get(hash) {
            self.stats.mesh_hits += 1;
            Some(Arc::clone(mesh))
        } else {
            self.stats.mesh_misses += 1;
            None
        }
    }

    /// Insert a mesh into the cache
    pub fn insert_mesh(&mut self, hash: blake3::Hash, mesh: CSGMesh) {
        self.mesh_cache.put(hash, Arc::new(mesh));
    }

    /// Get a cached sketch, returns None if not found
    pub fn get_sketch(&mut self, hash: &blake3::Hash) -> Option<Arc<CSGSketch>> {
        if let Some(sketch) = self.sketch_cache.get(hash) {
            self.stats.sketch_hits += 1;
            Some(Arc::clone(sketch))
        } else {
            self.stats.sketch_misses += 1;
            None
        }
    }

    /// Insert a sketch into the cache
    pub fn insert_sketch(&mut self, hash: blake3::Hash, sketch: CSGSketch) {
        self.sketch_cache.put(hash, Arc::new(sketch));
    }

    /// Clear all cached entries
    pub fn clear(&mut self) {
        self.mesh_cache.clear();
        self.sketch_cache.clear();
        self.stats = CacheStats::default();
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Get current number of cached meshes
    pub fn mesh_count(&self) -> usize {
        self.mesh_cache.len()
    }

    /// Get current number of cached sketches
    pub fn sketch_count(&self) -> usize {
        self.sketch_cache.len()
    }

    /// Get estimated memory usage in bytes (rough approximation)
    /// 
    /// Note: This is a rough estimate based on typical mesh/sketch sizes.
    /// Actual memory usage may vary significantly.
    pub fn estimated_memory_usage(&self) -> usize {
        // Rough estimates:
        // - Average CSGMesh: ~50KB (varies widely based on complexity)
        // - Average CSGSketch: ~10KB (typically smaller than meshes)
        const AVG_MESH_SIZE: usize = 50_000;
        const AVG_SKETCH_SIZE: usize = 10_000;
        
        self.mesh_count() * AVG_MESH_SIZE + self.sketch_count() * AVG_SKETCH_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
