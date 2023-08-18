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
