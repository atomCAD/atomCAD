//! The shared three-pass id merge (`nodes/variadic_ids.rs`) behind `switch`'s
//! case list and the `record_get` / `record_set` path rows. `switch_test.rs`
//! covers it through `merge_cases` too.

use atomcad_structure_designer::nodes::variadic_ids::merge_ids_by_key;

fn merge(old: &[(Option<u64>, &str)], new: &[&str], next_id: &mut u64) -> Vec<u64> {
    let old: Vec<(Option<u64>, &str)> = old.to_vec();
    merge_ids_by_key(&old, new, |a, b| a == b, next_id)
}

#[test]
fn key_match_keeps_ids_across_removal_and_reorder() {
    let mut next = 10;
    let ids = merge(
        &[(Some(1), "a"), (Some(2), "b"), (Some(3), "c")],
        &["c", "a"],
        &mut next,
    );
    assert_eq!(ids, vec![3, 1]);
    assert_eq!(next, 10, "nothing minted");
}

#[test]
fn positional_fallback_follows_an_in_place_edit() {
    let mut next = 10;
    let ids = merge(&[(Some(1), "a"), (Some(2), "b")], &["a", "x"], &mut next);
    assert_eq!(ids, vec![1, 2], "x inherits b's id at index 1");
    assert_eq!(next, 10);
}

#[test]
fn growth_mints_fresh_ids() {
    let mut next = 10;
    let ids = merge(&[(Some(1), "a")], &["a", "b", "c"], &mut next);
    assert_eq!(ids, vec![1, 10, 11]);
    assert_eq!(next, 12);
}

#[test]
fn no_positional_steal() {
    // [1, 2] -> [3, 1]: the key match of `1` runs before any positional
    // fallback and consumes old index 0, so `3` (at index 0) cannot steal
    // `1`'s id and mints a fresh one.
    let mut next = 10;
    let ids = merge(&[(Some(1), "1"), (Some(2), "2")], &["3", "1"], &mut next);
    assert_eq!(ids, vec![10, 1]);
    assert_eq!(next, 11);
}

#[test]
fn old_elements_without_an_id_take_part_in_no_pass() {
    let mut next = 10;
    let ids = merge(&[(None, "a"), (Some(2), "b")], &["a", "b"], &mut next);
    assert_eq!(ids, vec![10, 2]);
    let ids = merge(&[(None, "a")], &["x"], &mut next);
    assert_eq!(ids, vec![11]);
    assert_eq!(next, 12);
}

#[test]
fn next_id_only_grows_and_ids_are_never_recycled() {
    // Remove the highest id, then add an element: it does not get the
    // removed id back.
    let mut next = 4;
    let ids = merge(
        &[(Some(1), "a"), (Some(2), "b"), (Some(3), "c")],
        &["a", "b"],
        &mut next,
    );
    assert_eq!(ids, vec![1, 2]);
    let old: Vec<(Option<u64>, &str)> = vec![(Some(1), "a"), (Some(2), "b")];
    let ids = merge(&old, &["a", "b", "d"], &mut next);
    assert_eq!(ids, vec![1, 2, 4]);
    assert_eq!(next, 5);
}

#[test]
fn empty_lists() {
    let mut next = 7;
    assert!(merge(&[(Some(1), "a")], &[], &mut next).is_empty());
    assert_eq!(merge(&[], &["a"], &mut next), vec![7]);
    assert_eq!(next, 8);
}
