//! Miller-index arithmetic and symmetry families.
//!
//! Pure crystallography: reducing `(h,k,l)` triples to lowest terms,
//! enumerating the indices within a bound, and expanding an index into its
//! symmetry family `{hkl}`. Nothing here renders, hit-tests or
//! knows about the node network — the next `simplify_*`-shaped helper belongs
//! here rather than in a node file or a tessellator.

use glam::i32::IVec3;
use std::collections::HashSet;

pub fn simplify_miller_index(miller_index: IVec3) -> IVec3 {
    // Get absolute values for checking divisibility
    let abs_x = miller_index.x.abs();
    let abs_y = miller_index.y.abs();
    let abs_z = miller_index.z.abs();

    // Set max_divisor to the maximum of the absolute values of the components
    // This is an optimization as we don't need to check divisors larger than the largest component
    let max_divisor = abs_x.max(abs_y).max(abs_z);
    for divisor in (2..=max_divisor).rev() {
        // Check if all components are divisible by the divisor
        if abs_x % divisor == 0 && abs_y % divisor == 0 && abs_z % divisor == 0 {
            return IVec3::new(
                miller_index.x / divisor,
                miller_index.y / divisor,
                miller_index.z / divisor,
            );
        }
    }

    // If no common divisor found, return the original miller index
    miller_index
}

pub fn generate_possible_miller_indices(max_miller_index: i32) -> HashSet<IVec3> {
    let mut possible_miller_indices: HashSet<IVec3> = HashSet::new();

    // Iterate through all combinations within the max_miller_index range
    for h in -max_miller_index..=max_miller_index {
        for k in -max_miller_index..=max_miller_index {
            for l in -max_miller_index..=max_miller_index {
                // Skip the origin (0,0,0) as it's not a valid direction
                if h == 0 && k == 0 && l == 0 {
                    continue;
                }

                // Create the miller index and reduce it to simplest form
                let miller = IVec3::new(h, k, l);
                let simplified = simplify_miller_index(miller);

                // Add the simplified miller index to the set
                possible_miller_indices.insert(simplified);
            }
        }
    }

    // Return the set of possible miller indices
    possible_miller_indices
}

/// Returns the six permutations of `(a, b, c)`, deduplicated.
///
/// The result is sorted so downstream consumers (e.g. snapshot tests over the
/// resulting intersection geometry) see a stable element ordering.
pub fn generate_unique_permutations(a: i32, b: i32, c: i32) -> Vec<(i32, i32, i32)> {
    // Use a HashSet to automatically handle uniqueness of permutations.
    let mut unique_perms: HashSet<(i32, i32, i32)> = HashSet::new();

    // Manually list all 3! = 6 possible permutations for three elements.
    // The HashSet will ensure that only unique combinations are stored,
    // which is crucial if the input numbers themselves contain duplicates.
    unique_perms.insert((a, b, c));
    unique_perms.insert((a, c, b));
    unique_perms.insert((b, a, c));
    unique_perms.insert((b, c, a));
    unique_perms.insert((c, a, b));
    unique_perms.insert((c, b, a));

    // Convert the HashSet into a Vec, sorted for deterministic order
    // so downstream consumers (e.g. snapshot tests over the resulting
    // intersection geometry) see a stable element ordering.
    let mut perms: Vec<(i32, i32, i32)> = unique_perms.into_iter().collect();
    perms.sort();
    perms
}

/// Enumerates the symmetry family `{hkl}` of a Miller index `(hkl)`.
///
/// The family is every permutation of the *absolute* components combined with
/// every sign combination, skipping the sign flip on a zero component. So
/// `(1,1,0)` yields the 12 members of `{110}` and `(1,0,0)` the 6 of `{100}`.
/// The input's own signs are irrelevant: `(-1,2,-3)` and `(1,2,3)` name the same
/// family.
///
/// The order is deterministic — it follows `generate_unique_permutations`'s sort
/// — and the result contains no duplicates.
pub fn symmetry_equivalent_indices(miller: IVec3) -> Vec<IVec3> {
    let mut ret: Vec<IVec3> = Vec::new();

    // Store absolute values to identify the family type
    let abs_h = miller.x.abs();
    let abs_k = miller.y.abs();
    let abs_l = miller.z.abs();

    // Generate all permutations with sign combinations
    // This covers all cases: {100}, {110}, {111}, {hhl}, and general {hkl}

    // Generate permutations of the absolute values
    let abs_permutations = generate_unique_permutations(abs_h, abs_k, abs_l);

    // For each base permutation, generate all sign combinations
    for (x, y, z) in abs_permutations {
        // Add all sign combinations
        ret.push(IVec3::new(x, y, z));

        if x != 0 {
            ret.push(IVec3::new(-x, y, z));
        }

        if y != 0 {
            ret.push(IVec3::new(x, -y, z));

            if x != 0 {
                ret.push(IVec3::new(-x, -y, z));
            }
        }

        if z != 0 {
            ret.push(IVec3::new(x, y, -z));

            if x != 0 {
                ret.push(IVec3::new(-x, y, -z));
            }

            if y != 0 {
                ret.push(IVec3::new(x, -y, -z));

                if x != 0 {
                    ret.push(IVec3::new(-x, -y, -z));
                }
            }
        }
    }

    ret
}
