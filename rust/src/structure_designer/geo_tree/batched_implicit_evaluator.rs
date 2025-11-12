use glam::f64::DVec3;
use crate::structure_designer::implicit_eval::implicit_geometry::{ImplicitGeometry3D, BATCH_SIZE};
use crate::structure_designer::geo_tree::GeoNode;
use rayon::prelude::*;

/// Minimum number of points required to consider multi-threading
const MIN_POINTS_FOR_THREADING: usize = BATCH_SIZE * 2; // ~2048 points
/// Maximum number of threads to use for parallel evaluation
const MAX_THREADS: usize = 7;

/// BatchedImplicitEvaluator accumulates sample points and evaluates them in batches
/// for improved performance by reducing function call overhead and enabling parallel processing.
pub struct BatchedImplicitEvaluator<'a> {
    geo_tree: &'a GeoNode,
    pending_points: Vec<DVec3>,
    use_multithreading: bool,
}

impl<'a> BatchedImplicitEvaluator<'a> {
    /// Create a new BatchedImplicitEvaluator for the given geometry tree (single-threaded by default)
    pub fn new(geo_tree: &'a GeoNode) -> Self {
        Self {
            geo_tree,
            pending_points: Vec::new(),
            use_multithreading: false,
        }
    }
    
    /// Create a new BatchedImplicitEvaluator with explicit threading configuration
    pub fn new_with_threading(geo_tree: &'a GeoNode, use_multithreading: bool) -> Self {
        Self {
            geo_tree,
            pending_points: Vec::new(),
            use_multithreading,
        }
    }
    
    /// Add a point to be evaluated later in a batch
    /// Returns the index where the result will be stored in the results vector
    pub fn add_point(&mut self, point: DVec3) -> usize {
        let index = self.pending_points.len();
        self.pending_points.push(point);
        index
    }
    
    /// Evaluate a single point immediately (for cases that need immediate results)
    /// This bypasses batching and evaluates the point directly
    pub fn eval_immediate(&self, point: &DVec3) -> f64 {
        self.geo_tree.implicit_eval_3d(point)
    }
    
    /// Determine the optimal number of threads to use for parallel evaluation
    fn determine_thread_count() -> usize {
        let available_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        
        println!("Available parallel cores: {}", available_cores);
        
        // Use at most (cores - 1), never more than MAX_THREADS, minimum 1
        std::cmp::min(MAX_THREADS, std::cmp::max(1, available_cores.saturating_sub(1)))
    }
    
    /// Process all pending points in batches and return results
    /// Automatically chooses between single-threaded and multi-threaded evaluation
    /// The returned vector contains results in the same order as points were added
    /// After calling this method, the pending points are cleared
    pub fn flush(&mut self) -> Vec<f64> {
        if self.pending_points.is_empty() {
            return Vec::new();
        }
        
        // Debug print the geometry tree structure to analyze node frequency
        // println!("=== Geometry Tree Structure for Batch Evaluation ===");
        // println!("{}", self.geo_tree);
        // println!("=== End Geometry Tree Structure ===");
        
        let total_points = self.pending_points.len();
        let thread_count = Self::determine_thread_count();
        
        // Choose evaluation strategy based on workload size and threading configuration
        if self.use_multithreading && total_points >= MIN_POINTS_FOR_THREADING && thread_count > 1 {
            println!("Using multi-threaded evaluation with {} threads for {} points", thread_count, total_points);
            self.flush_multi_threaded()
        } else {
            if self.use_multithreading {
                println!("Falling back to single-threaded evaluation (points: {}, threads: {})", total_points, thread_count);
            }
            self.flush_single_threaded()
        }
    }
    
    /// Process all pending points using single-threaded batch evaluation
    pub fn flush_single_threaded(&mut self) -> Vec<f64> {
        // Use the same process_chunk method as multi-threaded, but on the entire slice
        let results = self.process_chunk(&self.pending_points);
        
        // Clear pending points for next batch
        self.pending_points.clear();
        
        results
    }
    
    /// Process all pending points using multi-threaded batch evaluation
    pub fn flush_multi_threaded(&mut self) -> Vec<f64> {
        let original_len = self.pending_points.len();
        let thread_count = Self::determine_thread_count();
        
        // Calculate points per thread, ensuring each thread gets at least one full batch
        let points_per_thread = std::cmp::max(BATCH_SIZE, original_len / thread_count);
        
        // Process chunks in parallel using Rayon - no copying, works directly on slices
        let chunk_results: Vec<Vec<f64>> = self.pending_points
            .par_chunks(points_per_thread)
            .map(|chunk| {
                self.process_chunk(chunk)
            })
            .collect();
        
        // Flatten results back into single vector in original order
        let results: Vec<f64> = chunk_results.into_iter().flatten().collect();
        
        // Clear pending points for next batch
        self.pending_points.clear();
        
        results
    }
    
    /// Process a chunk of points using batch evaluation
    /// This method is used by both single-threaded and multi-threaded evaluation
    fn process_chunk(&self, chunk: &[DVec3]) -> Vec<f64> {
        if chunk.is_empty() {
            return Vec::new();
        }
        
        let original_len = chunk.len();
        
        // Pad to multiple of BATCH_SIZE for efficient batch processing
        let padded_len = ((original_len + BATCH_SIZE - 1) / BATCH_SIZE) * BATCH_SIZE;
        
        // Create padded chunk
        let mut padded_chunk = chunk.to_vec();
        let padding_point = chunk.last().copied().unwrap_or(DVec3::ZERO);
        padded_chunk.resize(padded_len, padding_point);
        
        // Pre-allocate results for all batches (including padding)
        let mut results = vec![0.0; padded_len];
        
        for chunk_start in (0..padded_len).step_by(BATCH_SIZE) {
            // Convert Vec slice to fixed array for batch evaluation
            let mut batch_points = [DVec3::ZERO; BATCH_SIZE];
            for i in 0..BATCH_SIZE {
                batch_points[i] = padded_chunk[chunk_start + i];
            }
            
            // Get mutable slice directly into results Vec and convert to fixed array reference
            let results_slice = &mut results[chunk_start..chunk_start + BATCH_SIZE];
            let batch_results: &mut [f64; BATCH_SIZE] = results_slice.try_into().unwrap();
            
            // Evaluate directly into the results Vec - no intermediate array needed!
            self.geo_tree.implicit_eval_3d_batch(&batch_points, batch_results);
        }
        
        // Truncate to original length to remove padding results
        results.truncate(original_len);
        
        results
    }
    
    /// Get the number of pending points waiting to be evaluated
    pub fn pending_count(&self) -> usize {
        self.pending_points.len()
    }
    
    /// Check if there are any pending points
    pub fn has_pending(&self) -> bool {
        !self.pending_points.is_empty()
    }
    
    /// Clear all pending points without evaluating them
    pub fn clear(&mut self) {
        self.pending_points.clear();
    }
}

