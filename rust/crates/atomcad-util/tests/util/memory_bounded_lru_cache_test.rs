use atomcad_util::memory_bounded_lru_cache::MemoryBoundedLruCache;

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

// ---------------------------------------------------------------------------
// Instrumentation: high-water marks and the eviction counter.
// `doc/design_eval_memoization.md` D10 — the evaluation memo is created and
// dropped inside one pass, so a stats API that reads a live cache would return
// zeroes; the counters have to be harvestable, and the *peak* is the number a
// budget is sized against.
// ---------------------------------------------------------------------------

#[test]
fn peak_marks_never_decrease_and_bound_the_current_usage() {
    let mut cache = MemoryBoundedLruCache::new(100, string_size_estimator);

    assert_eq!(cache.peak_memory_bytes(), 0);
    assert_eq!(cache.peak_len(), 0);

    cache.insert("key1", "1234567890".to_string()); // 10 bytes
    cache.insert("key2", "1234567890".to_string()); // 10 bytes, total 20
    assert_eq!(cache.current_memory_bytes(), 20);
    assert_eq!(cache.peak_memory_bytes(), 20);
    assert_eq!(cache.peak_len(), 2);

    // Shrinking the live set must not move the high-water marks: the question
    // they answer is "how much did this cache need at its worst?".
    cache.pop(&"key1");
    assert_eq!(cache.current_memory_bytes(), 10);
    assert_eq!(cache.peak_memory_bytes(), 20);
    assert_eq!(cache.peak_len(), 2);

    cache.clear();
    assert_eq!(cache.current_memory_bytes(), 0);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.peak_memory_bytes(), 20);
    assert_eq!(cache.peak_len(), 2);

    // And the invariant that makes them readable at any moment.
    assert!(cache.peak_memory_bytes() >= cache.current_memory_bytes());
    assert!(cache.peak_len() >= cache.len());
}

#[test]
fn eviction_counter_counts_budget_evictions_only() {
    let mut cache = MemoryBoundedLruCache::new(10, string_size_estimator);

    cache.insert("key1", "12345".to_string()); // 5
    cache.insert("key2", "67890".to_string()); // 5, total 10
    assert_eq!(cache.lru_eviction_count(), 0);

    // Over budget: one LRU entry is dropped to make room.
    cache.insert("key3", "abc".to_string());
    assert_eq!(cache.lru_eviction_count(), 1);

    // Explicit removals are the caller's doing, not the budget's.
    cache.pop(&"key3");
    cache.pop_lru();
    cache.clear();
    assert_eq!(cache.lru_eviction_count(), 1);
}

#[test]
fn a_same_key_replacement_is_not_an_eviction() {
    // Replacing a value frees the old one, which is not the budget running
    // out. Given headroom, a replacement must leave the counter alone.
    //
    // (With *no* headroom, `insert` runs its eviction loop before it can know
    // the key already exists, so a replacement can genuinely evict a
    // *different* entry. That is a real budget eviction and is counted — what
    // is never counted is the replaced value's own removal.)
    let mut cache = MemoryBoundedLruCache::new(100, string_size_estimator);

    cache.insert("key1", "12345".to_string());
    cache.insert("key1", "abcdefgh".to_string());

    assert_eq!(cache.lru_eviction_count(), 0);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.current_memory_bytes(), 8);
}

#[test]
fn resizing_down_evicts_immediately_and_counts_as_eviction() {
    let mut cache = MemoryBoundedLruCache::new(100, string_size_estimator);

    cache.insert("key1", "1234567890".to_string()); // 10
    cache.insert("key2", "1234567890".to_string()); // 10
    cache.insert("key3", "1234567890".to_string()); // 10, total 30
    assert_eq!(cache.lru_eviction_count(), 0);

    // A preferences change must take effect at once, not at the next insert
    // (`doc/design_eval_memoization.md` D11).
    cache.resize(15);
    assert_eq!(cache.max_memory_bytes(), 15);
    assert!(cache.current_memory_bytes() <= 15);
    assert_eq!(cache.lru_eviction_count(), 2);

    // The two oldest went; the most recently inserted survives.
    assert_eq!(cache.get(&"key3"), Some(&"1234567890".to_string()));
}

#[test]
fn a_budget_below_one_entry_degrades_to_a_pass_through() {
    // Every cache here recomputes what it evicted, so an absurdly small budget
    // must cost speed, never correctness — the claim the preferences tooltip
    // makes. `insert` admits an over-budget value once the cache is empty.
    let mut cache = MemoryBoundedLruCache::new(1, string_size_estimator);

    cache.insert("key1", "a much larger value than the budget".to_string());
    assert_eq!(
        cache.get(&"key1"),
        Some(&"a much larger value than the budget".to_string())
    );

    // The next insert pushes the previous one out, so it behaves as a
    // one-entry pass-through rather than failing.
    cache.insert("key2", "another oversized value".to_string());
    assert_eq!(cache.get(&"key1"), None);
    assert_eq!(
        cache.get(&"key2"),
        Some(&"another oversized value".to_string())
    );
}
