use std::sync::Arc;
use std::mem;
use crate::common::csg_types::{CSGMesh, CSGSketch};
use crate::util::memory_bounded_lru_cache::MemoryBoundedLruCache;

/// Statistics for cache performance monitoring
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub mesh_hits: u64,
    pub mesh_misses: u64,
    pub sketch_hits: u64,
    pub sketch_misses: u64,
    pub mesh_memory_bytes: usize,
    pub sketch_memory_bytes: usize,
    pub mesh_capacity_bytes: usize,
    pub sketch_capacity_bytes: usize,
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

    /// Print a formatted summary of cache statistics
    pub fn print_summary(&self) {
        println!("=== CSG Conversion Cache Statistics ===");
        println!("Meshes:   {} hits, {} misses ({:.1}% hit rate)", 
            self.mesh_hits, self.mesh_misses, self.mesh_hit_rate() * 100.0);
        println!("          {:.2} MB / {:.2} MB ({:.1}% memory usage)",
            self.mesh_memory_bytes as f64 / 1_048_576.0,
            self.mesh_capacity_bytes as f64 / 1_048_576.0,
            (self.mesh_memory_bytes as f64 / self.mesh_capacity_bytes as f64) * 100.0);
        println!("Sketches: {} hits, {} misses ({:.1}% hit rate)", 
            self.sketch_hits, self.sketch_misses, self.sketch_hit_rate() * 100.0);
        println!("          {:.2} MB / {:.2} MB ({:.1}% memory usage)",
            self.sketch_memory_bytes as f64 / 1_048_576.0,
            self.sketch_capacity_bytes as f64 / 1_048_576.0,
            (self.sketch_memory_bytes as f64 / self.sketch_capacity_bytes as f64) * 100.0);
        println!("Total lookups: {}", self.total_lookups());
        println!("Total memory: {:.2} MB / {:.2} MB",
            (self.mesh_memory_bytes + self.sketch_memory_bytes) as f64 / 1_048_576.0,
            (self.mesh_capacity_bytes + self.sketch_capacity_bytes) as f64 / 1_048_576.0);
    }
}

/// Cache for CSG conversion results with memory-bounded LRU eviction policy
pub struct CsgConversionCache {
    mesh_cache: MemoryBoundedLruCache<blake3::Hash, Arc<CSGMesh>>,
    sketch_cache: MemoryBoundedLruCache<blake3::Hash, Arc<CSGSketch>>,
    stats: CacheStats,
}

impl CsgConversionCache {
    /// Create a new cache with specified memory capacities (in bytes)
    /// 
    /// # Arguments
    /// * `mesh_capacity_bytes` - Maximum memory for mesh cache (default: 200 MB)
    /// * `sketch_capacity_bytes` - Maximum memory for sketch cache (default: 56 MB)
    pub fn new(mesh_capacity_bytes: usize, sketch_capacity_bytes: usize) -> Self {
        Self {
            mesh_cache: MemoryBoundedLruCache::new(mesh_capacity_bytes, estimate_arc_csg_mesh_size),
            sketch_cache: MemoryBoundedLruCache::new(sketch_capacity_bytes, estimate_arc_csg_sketch_size),
            stats: CacheStats {
                mesh_capacity_bytes,
                sketch_capacity_bytes,
                ..Default::default()
            },
        }
    }

    /// Create a cache with default capacities (200 MB for meshes, 56 MB for sketches)
    pub fn with_defaults() -> Self {
        const DEFAULT_MESH_CAPACITY: usize = 200 * 1024 * 1024; // 200 MB
        const DEFAULT_SKETCH_CAPACITY: usize = 56 * 1024 * 1024; // 56 MB
        Self::new(DEFAULT_MESH_CAPACITY, DEFAULT_SKETCH_CAPACITY)
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
        self.mesh_cache.insert(hash, Arc::new(mesh));
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
        self.sketch_cache.insert(hash, Arc::new(sketch));
    }

    /// Clear all cached entries
    pub fn clear(&mut self) {
        self.mesh_cache.clear();
        self.sketch_cache.clear();
        // Preserve capacity settings when clearing stats
        let mesh_capacity = self.stats.mesh_capacity_bytes;
        let sketch_capacity = self.stats.sketch_capacity_bytes;
        self.stats = CacheStats {
            mesh_capacity_bytes: mesh_capacity,
            sketch_capacity_bytes: sketch_capacity,
            ..Default::default()
        };
    }

    /// Get cache statistics (updated with current memory usage)
    pub fn stats(&self) -> CacheStats {
        let mut stats = self.stats.clone();
        stats.mesh_memory_bytes = self.mesh_cache.current_memory_bytes();
        stats.sketch_memory_bytes = self.sketch_cache.current_memory_bytes();
        stats
    }

    /// Get current number of cached meshes
    pub fn mesh_count(&self) -> usize {
        self.mesh_cache.len()
    }

    /// Get current number of cached sketches
    pub fn sketch_count(&self) -> usize {
        self.sketch_cache.len()
    }

    /// Get current memory usage in bytes
    pub fn current_memory_usage(&self) -> usize {
        self.mesh_cache.current_memory_bytes() + self.sketch_cache.current_memory_bytes()
    }
    
    /// Get total memory capacity in bytes
    pub fn total_capacity(&self) -> usize {
        self.stats.mesh_capacity_bytes + self.stats.sketch_capacity_bytes
    }
}

/// Estimates the memory usage of a CSGMesh in bytes.
///
/// This function calculates the approximate heap memory used by a mesh,
/// including all its polygons, vertices, planes, and metadata.
///
/// # Memory Layout
/// - `Mesh` struct overhead
/// - `Vec<Polygon>` capacity and elements
/// - For each `Polygon`:
///   - `Vec<Vertex>` capacity and elements
///   - `Plane` (3 × Point3<f64>)
///   - `OnceLock<Aabb>` (if initialized)
///   - Metadata (empty for CSGMesh with `()` metadata)
/// - `OnceLock<Aabb>` for mesh bounding box (if initialized)
///
/// # Arguments
/// * `mesh` - The CSGMesh to estimate
///
/// # Returns
/// Estimated memory usage in bytes
pub fn estimate_csg_mesh_size(mesh: &CSGMesh) -> usize {
    let mut total_size = 0;

    // Base struct size (stack-allocated part)
    total_size += mem::size_of::<CSGMesh>();

    // Vec<Polygon> heap allocation
    // Capacity might be larger than length, so we use capacity for accuracy
    // Polygon<()> contains: Vec<Vertex>, Plane, OnceLock<Aabb>, Option<()>
    // Approximate size: ~120 bytes for the struct itself (stack part)
    const POLYGON_STRUCT_SIZE: usize = 120;
    total_size += mesh.polygons.capacity() * POLYGON_STRUCT_SIZE;

    // Size of all vertices in all polygons
    for polygon in &mesh.polygons {
        // Vec<Vertex> heap allocation
        // Vertex is Point3<f64> + Vector3<f64> = 3*8 + 3*8 = 48 bytes
        const VERTEX_SIZE: usize = 48;
        total_size += polygon.vertices.capacity() * VERTEX_SIZE;
        
        // Plane is already counted in Polygon size above (it's inline)
        // OnceLock<Aabb> is already counted in Polygon size above
        
        // If the polygon's bounding box is initialized, it's already in the struct
        // OnceLock doesn't allocate extra heap memory, it's inline
    }

    // Mesh-level bounding box (OnceLock<Aabb>) is already counted in base struct size
    // Metadata Option<()> is zero-sized

    total_size
}

/// Estimates the memory usage of a CSGSketch in bytes.
///
/// This function calculates the approximate heap memory used by a sketch,
/// including its geometry collection.
///
/// # Memory Layout
/// - `Sketch` struct overhead
/// - `GeometryCollection` which is `Vec<Geometry<Real>>`
/// - For each `Geometry` (typically `Polygon` or `MultiPolygon`):
///   - `Polygon`: exterior `LineString` + Vec of interior `LineString`s
///   - `MultiPolygon`: Vec of `Polygon`s
///   - `LineString`: Vec of `Coord<f64>` (each Coord is 2 × f64 = 16 bytes)
///
/// # Arguments
/// * `sketch` - The CSGSketch to estimate
///
/// # Returns
/// Estimated memory usage in bytes
pub fn estimate_csg_sketch_size(sketch: &CSGSketch) -> usize {
    let mut total_size = 0;

    // Base struct size (Sketch<()>)
    total_size += mem::size_of::<CSGSketch>();

    // GeometryCollection is a newtype wrapper around Vec<Geometry<Real>>
    // Account for the Vec capacity
    total_size += sketch.geometry.0.capacity() * mem::size_of::<geo::Geometry<f64>>();

    // Now estimate the heap allocations within each Geometry
    for geometry in sketch.geometry.iter() {
        match geometry {
            geo::Geometry::Polygon(polygon) => {
                // Exterior LineString: Vec<Coord<f64>>
                total_size += polygon.exterior().0.capacity() * mem::size_of::<geo::Coord<f64>>();
                
                // Interior LineStrings (holes): Vec<LineString>
                for interior in polygon.interiors() {
                    total_size += interior.0.capacity() * mem::size_of::<geo::Coord<f64>>();
                }
                // Vec for interiors vector itself (use len since we only have a slice)
                total_size += polygon.interiors().len() * mem::size_of::<geo::LineString<f64>>();
            }
            geo::Geometry::MultiPolygon(multi_polygon) => {
                // Vec capacity for the polygons vector
                total_size += multi_polygon.0.capacity() * mem::size_of::<geo::Polygon<f64>>();
                
                // Each polygon in the multi-polygon
                for polygon in &multi_polygon.0 {
                    // Exterior LineString
                    total_size += polygon.exterior().0.capacity() * mem::size_of::<geo::Coord<f64>>();
                    
                    // Interior LineStrings
                    for interior in polygon.interiors() {
                        total_size += interior.0.capacity() * mem::size_of::<geo::Coord<f64>>();
                    }
                    total_size += polygon.interiors().len() * mem::size_of::<geo::LineString<f64>>();
                }
            }
            // Other geometry types are rare in CSGSketch, but we'll add a small overhead
            _ => {
                total_size += 100; // Small overhead for other types
            }
        }
    }

    total_size
}

/// Size estimator for Arc<CSGMesh> used by MemoryBoundedLruCache
fn estimate_arc_csg_mesh_size(mesh: &Arc<CSGMesh>) -> usize {
    // Arc overhead (reference count, weak count, data pointer)
    let arc_overhead = mem::size_of::<Arc<CSGMesh>>();
    
    // Actual mesh size
    let mesh_size = estimate_csg_mesh_size(mesh.as_ref());
    
    arc_overhead + mesh_size
}

/// Size estimator for Arc<CSGSketch> used by MemoryBoundedLruCache
fn estimate_arc_csg_sketch_size(sketch: &Arc<CSGSketch>) -> usize {
    // Arc overhead (reference count, weak count, data pointer)
    let arc_overhead = mem::size_of::<Arc<CSGSketch>>();
    
    // Actual sketch size
    let sketch_size = estimate_csg_sketch_size(sketch.as_ref());
    
    arc_overhead + sketch_size
}
