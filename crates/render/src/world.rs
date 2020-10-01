use crate::{
    atoms::{AtomRepr, Atoms},
    utils::BoundingBox,
    SharedRenderState,
};
use indexmap::IndexMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use ultraviolet::Vec3;

macro_rules! declare_id {
    ($id_name:ident) => {
        #[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
        pub struct $id_name(usize);

        impl $id_name {
            const COUNTER: AtomicUsize = AtomicUsize::new(1);

            pub fn new() -> Self {
                let id = Self::COUNTER.fetch_add(1, Ordering::Relaxed);
                Self(id)
            }

            pub fn new_many(count: usize) -> impl ExactSizeIterator<Item = Self> + Clone {
                let first_id = Self::COUNTER.fetch_add(count, Ordering::Relaxed);
                (first_id..(first_id + count)).map(|id| Self(id))
            }
        }
    };
    ($($id_name:ident),*) => {
        $(
            declare_id!($id_name);
        )*
    };
}

declare_id!(FragmentId, PartId);

pub struct Fragment {
    atoms: Atoms,

    bounding_box: BoundingBox,
    center: Vec3, // not sure what type of center yet (median, initial atom, etc)
}

impl Fragment {
    pub fn from_atoms<I>(world: &World, atoms: I) -> Self
    where
        I: IntoIterator<Item = AtomRepr>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut point_sum = Vec3::zero();
        let mut max_point = Vec3::new(-f32::INFINITY, -f32::INFINITY, -f32::INFINITY);
        let mut min_point = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);

        let atoms = Atoms::new(
            world,
            atoms.into_iter().inspect(|atom| {
                point_sum += atom.pos;
                max_point.x = atom.pos.x.max(max_point.x);
                max_point.y = atom.pos.x.max(max_point.y);
                max_point.z = atom.pos.x.max(max_point.z);
                min_point.x = atom.pos.x.min(min_point.x);
                min_point.y = atom.pos.x.min(min_point.y);
                min_point.z = atom.pos.x.min(min_point.z);
            }),
        );

        let center = point_sum / atoms.len() as f32;
        let bounding_box = BoundingBox {
            min: min_point,
            max: max_point,
        };

        Self {
            atoms,

            bounding_box,
            center,
        }
    }

    pub fn atoms(&self) -> &Atoms {
        &self.atoms
    }
}

pub struct Part {
    fragments: Vec<FragmentId>,
    bounding_box: BoundingBox,
    center: Vec3,
}

impl Part {
    pub fn from_fragments<I>(world: &mut World, fragments: I) -> Self
    where
        I: IntoIterator<Item = Fragment>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut bounding_box = BoundingBox {
            min: Vec3::new(-f32::INFINITY, -f32::INFINITY, -f32::INFINITY),
            max: Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
        };
        let mut center = Vec3::zero();

        let fragments: Vec<_> = world
            .spawn_fragment_batch(fragments.into_iter().inspect(|fragment| {
                bounding_box = bounding_box.union(&fragment.bounding_box);
                center += fragment.center;
            }))
            .collect();

        let center = center / fragments.len() as f32;

        assert!(
            fragments.len() > 0,
            "must have at least one fragment in a part"
        );

        Part {
            fragments,
            bounding_box,
            center,
        }
    }

    pub fn center(&self) -> Vec3 {
        self.center
    }

    pub fn fragments(&self) -> &[FragmentId] {
        &self.fragments
    }

    pub fn offset_by(&mut self, offset: Vec3) {
        todo!()
    }

    pub fn move_to(&mut self, point: Vec3) {
        todo!()
    }
}

/// Represents all the parts and fragments currently alive in a scene.
pub struct World {
    pub(crate) shared_render: Arc<SharedRenderState>,

    pub(crate) parts: IndexMap<PartId, Part>,
    pub(crate) fragments: IndexMap<FragmentId, Fragment>,

    // These are updated and cleared every frame.
    pub(crate) added_parts: Vec<PartId>,
    pub(crate) added_fragments: Vec<FragmentId>,
    pub(crate) modified_parts: Vec<PartId>,
    pub(crate) modified_fragments: Vec<FragmentId>,
}

impl World {
    pub(crate) fn new(shared_render: Arc<SharedRenderState>) -> Self {
        Self {
            shared_render,

            parts: IndexMap::new(),
            fragments: IndexMap::new(),

            added_parts: Vec::new(),
            added_fragments: Vec::new(),
            modified_parts: Vec::new(),
            modified_fragments: Vec::new(),
        }
    }

    // pub fn split_empty(&self) -> Self {
    //     Self::new(Arc::clone(&self.shared_render))
    // }

    pub fn spawn_part(&mut self, part: Part) -> PartId {
        let id = PartId::new();
        self.parts.insert(id, part);
        self.added_parts.push(id);
        id
    }

    pub fn spawn_fragment(&mut self, fragment: Fragment) -> FragmentId {
        let id = FragmentId::new();
        self.fragments.insert(id, fragment);
        self.added_fragments.push(id);
        id
    }

    pub fn spawn_part_batch<I>(&mut self, parts: I) -> impl ExactSizeIterator<Item = PartId>
    where
        I: IntoIterator<Item = Part>,
        I::IntoIter: ExactSizeIterator,
    {
        let parts = parts.into_iter();
        let ids = PartId::new_many(parts.len());

        self.parts.extend(ids.clone().zip(parts));
        self.added_parts.extend(ids.clone());
        ids
    }

    pub fn spawn_fragment_batch<I>(
        &mut self,
        fragments: I,
    ) -> impl ExactSizeIterator<Item = FragmentId>
    where
        I: IntoIterator<Item = Fragment>,
        I::IntoIter: ExactSizeIterator,
    {
        let fragments = fragments.into_iter();
        let ids = FragmentId::new_many(fragments.len());

        self.fragments.extend(ids.clone().zip(fragments));
        self.added_fragments.extend(ids.clone());
        ids
    }

    // pub fn consume(&mut self, other: World) {
    //     self.parts.extend(other.parts);
    //     self.fragments.extend(other.fragments);
    // }

    pub fn fragments(&self) -> impl Iterator<Item = &Fragment> {
        self.fragments.values()
    }
}