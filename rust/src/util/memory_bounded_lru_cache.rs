use lru::LruCache;
use std::hash::Hash;

/// A memory-bounded LRU cache that evicts entries based on total memory usage
/// rather than entry count.
///
/// This cache wraps the standard `LruCache` but tracks memory usage and evicts
/// least-recently-used entries when the total memory exceeds the specified limit.
///
/// # Type Parameters
/// * `K` - Key type (must be `Hash + Eq`)
/// * `V` - Value type
///
/// # Example
/// ```
/// use rust_lib_flutter_cad::util::memory_bounded_lru_cache::MemoryBoundedLruCache;
///
/// fn estimate_string_size(s: &String) -> usize {
///     s.len()
/// }
///
/// let mut cache = MemoryBoundedLruCache::new(
///     1024,  // 1 KB max memory
///     estimate_string_size
/// );
///
/// cache.insert("key1".to_string(), "value1".to_string());
/// ```
pub struct MemoryBoundedLruCache<K: Hash + Eq, V> {
    /// Underlying LRU cache with no count limit
    cache: LruCache<K, V>,
    
    /// Current total memory usage in bytes
    current_memory_bytes: usize,
    
    /// Maximum allowed memory usage in bytes
    max_memory_bytes: usize,
    
    /// Function to estimate the memory size of a value in bytes
    size_estimator: fn(&V) -> usize,
}

impl<K: Hash + Eq, V> MemoryBoundedLruCache<K, V> {
    /// Creates a new memory-bounded LRU cache.
    ///
    /// # Arguments
    /// * `max_memory_bytes` - Maximum total memory usage in bytes
    /// * `size_estimator` - Function that estimates the size of a value in bytes
    ///
    /// # Example
    /// ```
    /// use rust_lib_flutter_cad::util::memory_bounded_lru_cache::MemoryBoundedLruCache;
    /// 
    /// let cache: MemoryBoundedLruCache<String, String> = MemoryBoundedLruCache::new(
    ///     256 * 1024 * 1024,  // 256 MB
    ///     |value| std::mem::size_of_val(value)
    /// );
    /// ```
    pub fn new(max_memory_bytes: usize, size_estimator: fn(&V) -> usize) -> Self {
        Self {
            cache: LruCache::unbounded(),
            current_memory_bytes: 0,
            max_memory_bytes,
            size_estimator,
        }
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the key already exists, the old value is replaced and its memory is freed.
    /// If adding the new value would exceed the memory limit, least-recently-used
    /// entries are evicted until there is sufficient space.
    ///
    /// # Arguments
    /// * `key` - The key to insert
    /// * `value` - The value to insert
    ///
    /// # Returns
    /// The old value if the key already existed, otherwise `None`
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let value_size = (self.size_estimator)(&value);
        
        // Evict LRU entries until we have enough space for the new value
        while self.current_memory_bytes + value_size > self.max_memory_bytes {
            if let Some((_, evicted_value)) = self.cache.pop_lru() {
                let evicted_size = (self.size_estimator)(&evicted_value);
                self.current_memory_bytes = self.current_memory_bytes.saturating_sub(evicted_size);
            } else {
                // Cache is empty, but value is still too large
                // We allow insertion anyway to avoid data loss
                break;
            }
        }
        
        // Insert the new value and handle replacement
        let old_value = self.cache.put(key, value);
        
        if let Some(ref old_val) = old_value {
            // Key existed, subtract old value's size
            let old_size = (self.size_estimator)(old_val);
            self.current_memory_bytes = self.current_memory_bytes.saturating_sub(old_size);
        }
        
        // Add new value's size
        self.current_memory_bytes += value_size;
        
        old_value
    }

    /// Gets a reference to the value associated with the key.
    ///
    /// This marks the entry as recently used.
    ///
    /// # Arguments
    /// * `key` - The key to look up
    ///
    /// # Returns
    /// A reference to the value if found, otherwise `None`
    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.cache.get(key)
    }

    /// Gets a mutable reference to the value associated with the key.
    ///
    /// This marks the entry as recently used.
    ///
    /// # Arguments
    /// * `key` - The key to look up
    ///
    /// # Returns
    /// A mutable reference to the value if found, otherwise `None`
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.cache.get_mut(key)
    }

    /// Peeks at the value associated with the key without marking it as used.
    ///
    /// # Arguments
    /// * `key` - The key to look up
    ///
    /// # Returns
    /// A reference to the value if found, otherwise `None`
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.cache.peek(key)
    }

    /// Removes and returns the value associated with the key.
    ///
    /// # Arguments
    /// * `key` - The key to remove
    ///
    /// # Returns
    /// The value if found, otherwise `None`
    pub fn pop(&mut self, key: &K) -> Option<V> {
        if let Some(value) = self.cache.pop(key) {
            let value_size = (self.size_estimator)(&value);
            self.current_memory_bytes = self.current_memory_bytes.saturating_sub(value_size);
            Some(value)
        } else {
            None
        }
    }

    /// Removes and returns the least recently used entry.
    ///
    /// # Returns
    /// The least recently used key-value pair if the cache is not empty, otherwise `None`
    pub fn pop_lru(&mut self) -> Option<(K, V)> {
        if let Some((key, value)) = self.cache.pop_lru() {
            let value_size = (self.size_estimator)(&value);
            self.current_memory_bytes = self.current_memory_bytes.saturating_sub(value_size);
            Some((key, value))
        } else {
            None
        }
    }

    /// Clears all entries from the cache.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.current_memory_bytes = 0;
    }

    /// Returns the number of entries in the cache.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Returns the current total memory usage in bytes.
    pub fn current_memory_bytes(&self) -> usize {
        self.current_memory_bytes
    }

    /// Returns the maximum allowed memory usage in bytes.
    pub fn max_memory_bytes(&self) -> usize {
        self.max_memory_bytes
    }

    /// Returns the current memory usage as a percentage of the maximum (0.0 to 1.0).
    pub fn memory_usage_ratio(&self) -> f64 {
        if self.max_memory_bytes == 0 {
            0.0
        } else {
            self.current_memory_bytes as f64 / self.max_memory_bytes as f64
        }
    }

    /// Checks if the key exists in the cache without marking it as used.
    pub fn contains(&self, key: &K) -> bool {
        self.cache.contains(key)
    }

    /// Resizes the cache to a new maximum memory limit.
    ///
    /// If the new limit is smaller than the current usage, entries are evicted
    /// until the usage is within the new limit.
    ///
    /// # Arguments
    /// * `new_max_memory_bytes` - The new maximum memory limit in bytes
    pub fn resize(&mut self, new_max_memory_bytes: usize) {
        self.max_memory_bytes = new_max_memory_bytes;
        
        // Evict entries if we're over the new limit
        while self.current_memory_bytes > self.max_memory_bytes {
            if let Some((_, evicted_value)) = self.cache.pop_lru() {
                let evicted_size = (self.size_estimator)(&evicted_value);
                self.current_memory_bytes = self.current_memory_bytes.saturating_sub(evicted_size);
            } else {
                break;
            }
        }
    }
}




