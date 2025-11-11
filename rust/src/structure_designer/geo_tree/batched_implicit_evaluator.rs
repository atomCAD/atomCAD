use glam::f64::DVec3;
use crate::structure_designer::implicit_eval::implicit_geometry::{ImplicitGeometry3D, BATCH_SIZE};
use crate::structure_designer::geo_tree::GeoNode;

/// BatchedImplicitEvaluator accumulates sample points and evaluates them in batches
/// for improved performance by reducing function call overhead and enabling future SIMD optimizations.
pub struct BatchedImplicitEvaluator<'a> {
    geo_tree: &'a GeoNode,
    pending_points: Vec<DVec3>,
}

impl<'a> BatchedImplicitEvaluator<'a> {
    /// Create a new BatchedImplicitEvaluator for the given geometry tree
    pub fn new(geo_tree: &'a GeoNode) -> Self {
        Self {
            geo_tree,
            pending_points: Vec::new(),
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
    
    /// Process all pending points in batches and return results
    /// The returned vector contains results in the same order as points were added
    /// After calling this method, the pending points are cleared
    pub fn flush(&mut self) -> Vec<f64> {
        if self.pending_points.is_empty() {
            return Vec::new();
        }
        
        let original_len = self.pending_points.len();
        
        // Pad to multiple of BATCH_SIZE for efficient batch processing
        let padded_len = ((original_len + BATCH_SIZE - 1) / BATCH_SIZE) * BATCH_SIZE;
        
        // Pad with the last point to avoid evaluating invalid positions
        // If somehow empty (shouldn't happen due to check above), use zero
        let padding_point = self.pending_points.last().copied().unwrap_or(DVec3::ZERO);
        self.pending_points.resize(padded_len, padding_point);
        
        // Pre-allocate results for all batches (including padding)
        let mut results = vec![0.0; padded_len];
        
        for chunk_start in (0..padded_len).step_by(BATCH_SIZE) {
            // Convert Vec slice to fixed array for batch evaluation
            let mut batch_points = [DVec3::ZERO; BATCH_SIZE];
            for i in 0..BATCH_SIZE {
                batch_points[i] = self.pending_points[chunk_start + i];
            }
            
            // Get mutable slice directly into results Vec and convert to fixed array reference
            let results_slice = &mut results[chunk_start..chunk_start + BATCH_SIZE];
            let batch_results: &mut [f64; BATCH_SIZE] = results_slice.try_into().unwrap();
            
            // Evaluate directly into the results Vec - no intermediate array needed!
            self.geo_tree.implicit_eval_3d_batch(&batch_points, batch_results);
        }
        
        // Truncate to original length to remove padding results
        results.truncate(original_len);
        
        // Clear pending points for next batch
        self.pending_points.clear();
        
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

