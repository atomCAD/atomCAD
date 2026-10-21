//! Hidden stable ids for variadic pin lists, shared by the nodes whose pins are
//! a user-edited list keyed by a unique value (`switch` cases, the
//! `record_get` / `record_set` path rows). The id is what a wire follows, so a
//! list edit must hand each surviving element its old id.
//!
//! The merge is three passes, and they must run separately: resolving *all*
//! key matches before any positional fallback is what stops an early
//! positional fallback from stealing the id a later key match needs
//! (`[1, 2] → [3, 1]` keeps `1`'s id on `1`, not on `3`). See
//! `doc/design_switch_node.md`, "Case-list editing".

/// The three-pass id merge: key match, then positional fallback, then mint.
/// Returns one id per element of `new`. Rejects nothing: callers check
/// duplicates and emptiness first.
///
/// 1. **Key match.** A new key equal (`same_key`) to an old element's key
///    keeps that element's id (removal + reorder).
/// 2. **Positional fallback.** A still-unmatched new key inherits the id of
///    the old element at its index, if that element has an id the first pass
///    did not consume (an in-place edit — the wire follows).
/// 3. **Mint.** Anything still without an id takes `*next_id`, which is then
///    bumped. `next_id` only ever grows; ids are never recycled.
///
/// Old elements with no id take part in neither pass.
pub fn merge_ids_by_key<K>(
    old: &[(Option<u64>, K)],
    new: &[K],
    same_key: impl Fn(&K, &K) -> bool,
    next_id: &mut u64,
) -> Vec<u64> {
    let new: Vec<Option<&K>> = new.iter().map(Some).collect();
    merge_ids_by_optional_key(old, &new, |a, b| same_key(a, b), next_id)
        .into_iter()
        .map(|id| id.expect("every keyed element gets an id"))
        .collect()
}

/// [`merge_ids_by_key`] where some new elements are **not merged**: a `None`
/// entry keeps its index (so positional fallback still lines up the
/// elements around it) but takes part in no pass, consumes no old id and
/// gets no minted one (`None` in the result). `record_get` / `record_set`
/// use this for the dangling rows a path list names verbatim; the old rows
/// those claim are passed with `id: None`, which withholds them likewise.
pub fn merge_ids_by_optional_key<K, Q>(
    old: &[(Option<u64>, K)],
    new: &[Option<Q>],
    same_key: impl Fn(&K, &Q) -> bool,
    next_id: &mut u64,
) -> Vec<Option<u64>> {
    let mut consumed = vec![false; old.len()];
    let mut assigned: Vec<Option<u64>> = vec![None; new.len()];

    // Pass 1: key match.
    for (ni, nk) in new.iter().enumerate() {
        let Some(nk) = nk else { continue };
        if let Some(oi) = old
            .iter()
            .enumerate()
            .position(|(oi, (id, ok))| !consumed[oi] && id.is_some() && same_key(ok, nk))
        {
            assigned[ni] = old[oi].0;
            consumed[oi] = true;
        }
    }

    // Pass 2: positional fallback. `consumed` / `old` are old-length, so `ni`
    // can run past them when the list grows — an out-of-range index means
    // "no old element here" (mint below).
    for (ni, slot) in assigned.iter_mut().enumerate() {
        if new[ni].is_none() || slot.is_some() || consumed.get(ni).copied().unwrap_or(true) {
            continue;
        }
        if let Some(id) = old.get(ni).and_then(|(id, _)| *id) {
            *slot = Some(id);
            consumed[ni] = true;
        }
    }

    // Pass 3: mint.
    assigned
        .into_iter()
        .zip(new)
        .map(|(id, nk)| {
            nk.as_ref()?;
            Some(id.unwrap_or_else(|| {
                let fresh = *next_id;
                *next_id += 1;
                fresh
            }))
        })
        .collect()
}
