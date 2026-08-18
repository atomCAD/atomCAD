//! Miller-index arithmetic and symmetry families.
//!
//! Pure crystallography: reducing `(h,k,l)` triples to lowest terms and
//! enumerating the indices within a bound. Nothing here renders, hit-tests or
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
