pub type FeatureId = usize;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FeatureCopyId {
    pub feature_id: FeatureId,
    pub copy_index: usize,
}

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
