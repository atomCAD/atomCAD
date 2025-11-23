use rust_lib_flutter_cad::util::memory_bounded_lru_cache::MemoryBoundedLruCache;

fn string_size_estimator(s: &String) -> usize {
    s.len()
}

#[test]
fn test_basic_insert_and_get() {
    let mut cache = MemoryBoundedLruCache::new(100, string_size_estimator);
    
    cache.insert("key1", "value1".to_string());
    assert_eq!(cache.get(&"key1"), Some(&"value1".to_string()));
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.current_memory_bytes(), 6); // "value1".len()
}

#[test]
fn test_memory_eviction() {
    let mut cache = MemoryBoundedLruCache::new(10, string_size_estimator);
    
    cache.insert("key1", "12345".to_string()); // 5 bytes
    cache.insert("key2", "67890".to_string()); // 5 bytes, total = 10
    assert_eq!(cache.len(), 2);
    
    // This should evict key1 (LRU)
    cache.insert("key3", "abc".to_string()); // 3 bytes, would be 13 total
    
    assert_eq!(cache.get(&"key1"), None); // Evicted
    assert_eq!(cache.get(&"key2"), Some(&"67890".to_string()));
    assert_eq!(cache.get(&"key3"), Some(&"abc".to_string()));
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.current_memory_bytes(), 8); // 5 + 3
}

#[test]
fn test_replacement() {
    let mut cache = MemoryBoundedLruCache::new(100, string_size_estimator);
    
    cache.insert("key1", "value1".to_string());
    assert_eq!(cache.current_memory_bytes(), 6);
    
    let old = cache.insert("key1", "new_value".to_string());
    assert_eq!(old, Some("value1".to_string()));
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.current_memory_bytes(), 9); // "new_value".len()
}

#[test]
fn test_clear() {
    let mut cache = MemoryBoundedLruCache::new(100, string_size_estimator);
    
    cache.insert("key1", "value1".to_string());
    cache.insert("key2", "value2".to_string());
    
    cache.clear();
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.current_memory_bytes(), 0);
}

#[test]
fn test_pop() {
    let mut cache = MemoryBoundedLruCache::new(100, string_size_estimator);
    
    cache.insert("key1", "value1".to_string());
    assert_eq!(cache.current_memory_bytes(), 6);
    
    let value = cache.pop(&"key1");
    assert_eq!(value, Some("value1".to_string()));
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.current_memory_bytes(), 0);
}

#[test]
fn test_resize() {
    let mut cache = MemoryBoundedLruCache::new(100, string_size_estimator);
    
    cache.insert("key1", "12345".to_string()); // 5 bytes
    cache.insert("key2", "67890".to_string()); // 5 bytes
    cache.insert("key3", "abcde".to_string()); // 5 bytes
    assert_eq!(cache.len(), 3);
    
    // Resize to smaller limit, should evict LRU entries
    cache.resize(8);
    
    assert!(cache.len() <= 2); // At most 2 entries can fit
    assert!(cache.current_memory_bytes() <= 8);
}

#[test]
fn test_memory_usage_ratio() {
    let mut cache = MemoryBoundedLruCache::new(100, string_size_estimator);
    
    cache.insert("key1", "12345".to_string()); // 5 bytes
    assert_eq!(cache.memory_usage_ratio(), 0.05); // 5/100
    
    cache.insert("key2", "1234567890".to_string()); // 10 bytes
    assert_eq!(cache.memory_usage_ratio(), 0.15); // 15/100
}

#[test]
fn test_oversized_value() {
    let mut cache = MemoryBoundedLruCache::new(5, string_size_estimator);
    
    // Insert a value larger than the max capacity
    // It should still be inserted (to avoid data loss)
    cache.insert("key1", "1234567890".to_string()); // 10 bytes > 5 max
    
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.get(&"key1"), Some(&"1234567890".to_string()));
}







