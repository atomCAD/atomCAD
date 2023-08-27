pub type FeatureId = usize;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FeatureCopyId {
    pub feature_id: FeatureId,
    pub copy_index: usize,
}

/// Robustly describes an atom in a molecule by identifying its feature path.
/// Using an AtomSpecifier to label an atom allows features to be changed,
/// the timeline to be altered, and atoms to be traced to their origin, all in a way
/// that is stable and intuitive.
///
/// AtomSpecifier forces you to avoid using atom indexes, which are brittle. For example,
/// if a feature depends on "atom six", then editing the timeline and adding more atoms
/// before that feature will cause the reference to break.
///
/// In the simplest case, an AtomSpecifier includes a child index and a feature ID. In
/// a benzene feature, for example, there would be six children (0 <= child index <= 5),
/// and one feature ID. Note that feature IDs must be stable: changing the timeline must not
/// ever change a feature's ID.
///
/// This alone is a significant improvement: atom indexes inside a feature are stable unless
/// the feature changes. However, in derived features like mirroring and patterning, the
/// contents of a feature is not stable against changes to the timeline.
///
/// One part of combating this problem is to use a FeatureCopyId rather than a FeatureID.
/// For owned, non-copied features, every AtomSpecifier created will have a FeatureCopyId::copy_index
/// of 0, indicating no copying. But in a feature that makes multiple copies of some data (say that
/// you pattern a single carbon atom 10 times to make a chain), the copy index will increase
/// each time. Assume that the pattern has a feature ID of 1: your copies will then have atom
/// specifiers with feature copy IDs with a feature id of 1 and copy indexes from 0 to 9.
/// The fifth carbon could be referred to with a feature id of 1, child index of 0, and copy index of 4:
/// then if we adjust feature 1 to pattern a group of atoms ten times, the FeatureCopyId (1, 4) and child
/// index 0 will still point at the same atom as before (even though the global atom indexes are wildly different).
///
/// Finally, note that feature_path is a Vec<FeatureCopyId>, not a single feature copy ID as was
/// hinted at. This is used to store the full feature lineage of an atom (for example, if an
/// atom is a mirrored copy of a mirrored copy of some primitive, it's feature path will include
/// all of those features). Trivially, this is useful for figuring out where an atom comes from,
/// but it is less obvious that edge cases with repeated patterning arise if it is not included.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AtomSpecifier {
    pub feature_path: Vec<FeatureCopyId>,
    pub child_index: usize,
}

impl AtomSpecifier {
    // Creates the trivial AtomSpecifier for the first atom created by feature `feature_id`.
    pub fn new(feature_id: FeatureId) -> Self {
        AtomSpecifier {
            feature_path: vec![FeatureCopyId {
                feature_id,
                copy_index: 0,
            }],
            child_index: 0,
        }
    }

    // Uses this `AtomSpecifier` like an iterator: mutates self to increment the child index,
    // and returns a clone of this AtomSpecifier that can be used to name an atom.
    pub fn next(&mut self) -> Self {
        let ret = AtomSpecifier {
            feature_path: self.feature_path.clone(),
            child_index: self.child_index,
        };

        self.child_index += 1;
        ret
    }
}
