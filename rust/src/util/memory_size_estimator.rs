/// Trait for types that can estimate their memory usage in bytes
/// 
/// This is used for cache management and memory-bounded data structures.
/// The estimate should include:
/// - The size of the struct itself (std::mem::size_of::<Self>())
/// - Heap-allocated data (Vec, HashMap, Box, String, etc.)
/// - Recursively estimated sizes of contained types
/// 
/// The estimate does not need to be exact, but should be:
/// - Reasonably accurate (within ~20% of actual usage)
/// - Fast to compute (avoid expensive traversals if possible)
/// - Conservative (slightly overestimate rather than underestimate)
pub trait MemorySizeEstimator {
    /// Returns an estimate of the memory usage of this value in bytes
    fn estimate_memory_bytes(&self) -> usize;
}

// Implementations for common standard library types

impl MemorySizeEstimator for String {
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<String>() + self.capacity()
    }
}

impl<T> MemorySizeEstimator for Vec<T> {
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<Vec<T>>() + (self.capacity() * std::mem::size_of::<T>())
    }
}

impl<T> MemorySizeEstimator for Option<T> 
where
    T: MemorySizeEstimator,
{
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<Option<T>>() + self.as_ref().map_or(0, |v| v.estimate_memory_bytes())
    }
}

impl<T> MemorySizeEstimator for Box<T> 
where
    T: MemorySizeEstimator,
{
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<Box<T>>() + self.as_ref().estimate_memory_bytes()
    }
}

// Primitive types - just their stack size
impl MemorySizeEstimator for u8 {
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<u8>()
    }
}

impl MemorySizeEstimator for u16 {
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<u16>()
    }
}

impl MemorySizeEstimator for u32 {
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<u32>()
    }
}

impl MemorySizeEstimator for u64 {
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<u64>()
    }
}

impl MemorySizeEstimator for i32 {
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<i32>()
    }
}

impl MemorySizeEstimator for f32 {
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<f32>()
    }
}

impl MemorySizeEstimator for f64 {
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<f64>()
    }
}

impl MemorySizeEstimator for bool {
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<bool>()
    }
}




