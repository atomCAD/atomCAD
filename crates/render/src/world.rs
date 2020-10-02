use crate::{
    atoms::{AtomRepr, Atoms},
    utils::BoundingBox,
    GlobalGpuResources,
};
use indexmap::IndexMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use ultraviolet::{Rotor3, Vec3};

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
    id: FragmentId,
    atoms: Atoms,

    bounding_box: BoundingBox,
    center: Vec3, // not sure what type of center yet (median, initial atom, etc)
    offset: Vec3,
    rotation: Rotor3,
}

impl Fragment {
    pub fn from_atoms<I>(gpu_resources: &GlobalGpuResources, atoms: I) -> Self
    where
        I: IntoIterator<Item = AtomRepr>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut point_sum = Vec3::zero();
        let mut max_point = Vec3::new(-f32::INFINITY, -f32::INFINITY, -f32::INFINITY);
        let mut min_point = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);

        let atoms = Atoms::new(
            gpu_resources,
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
            id: FragmentId::new(),
            atoms,

            bounding_box,
            center,
            offset: Vec3::zero(),
            rotation: Rotor3::default(),
        }
    }

    pub fn id(&self) -> FragmentId {
        self.id
    }

    pub fn atoms(&self) -> &Atoms {
        &self.atoms
    }

    pub fn offset(&self) -> Vec3 {
        self.offset
    }

    pub fn rotation(&self) -> Rotor3 {
        self.rotation
    }
}

pub struct Part {
    id: PartId,
    fragments: Vec<FragmentId>,
    bounding_box: BoundingBox,
    center: Vec3,
    offset: Vec3,
    rotation: Rotor3,
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
        let part_id = PartId::new();

        let fragments: Vec<_> = fragments
            .into_iter()
            .inspect(|fragment| {
                bounding_box = bounding_box.union(&fragment.bounding_box);
                center += fragment.center;
            })
            .map(move |fragment| world.spawn_fragment(part_id, fragment))
            .collect();

        // let fragments: Vec<_> = world
        //     .spawn_fragment_batch(
        //         id,
        //         fragments.into_iter().inspect(|fragment| {
        //             bounding_box = bounding_box.union(&fragment.bounding_box);
        //             center += fragment.center;
        //         }),
        //     )
        //     .collect();

        let center = center / fragments.len() as f32;

        assert!(
            fragments.len() > 0,
            "must have at least one fragment in a part"
        );

        Part {
            id: part_id,
            fragments,
            bounding_box,
            center,
            offset: -center,
            rotation: Rotor3::default(),
        }
    }

    pub fn id(&self) -> PartId {
        self.id
    }

    pub fn fragments(&self) -> &[FragmentId] {
        &self.fragments
    }

    pub fn offset(&self) -> Vec3 {
        self.offset
    }

    pub fn rotation(&self) -> Rotor3 {
        self.rotation
    }

    pub fn offset_by(&mut self, offset: Vec3) {
        self.offset += offset;
    }

    pub fn move_to(&mut self, point: Vec3) {
        self.offset = point - self.center;
    }
}

/// Represents all the parts and fragments currently alive in a scene.
pub struct World {
    pub(crate) parts: IndexMap<PartId, Part>,
    pub(crate) fragments: IndexMap<FragmentId, Fragment>,

    // These are updated and cleared every frame.
    pub(crate) added_parts: Vec<PartId>,
    pub(crate) added_fragments: Vec<(PartId, FragmentId)>,
    pub(crate) modified_parts: Vec<PartId>,
    pub(crate) modified_fragments: Vec<(PartId, FragmentId)>,
}

impl World {
    pub fn new() -> Self {
        Self {
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
        let id = part.id;
        self.parts.insert(id, part);
        self.added_parts.push(id);
        id
    }

    pub fn spawn_fragment(&mut self, part_id: PartId, fragment: Fragment) -> FragmentId {
        let id = fragment.id;
        self.fragments.insert(id, fragment);
        self.added_fragments.push((part_id, id));
        id
    }

    pub fn merge(&mut self, other: World) {
        self.parts.extend(other.parts);
        self.fragments.extend(other.fragments);

        self.added_parts.extend(other.added_parts);
        self.added_fragments.extend(other.added_fragments);
        self.modified_parts.extend(other.modified_parts);
        self.modified_fragments.extend(other.modified_fragments);
    }

    pub fn parts(&self) -> impl ExactSizeIterator<Item = &Part> {
        self.parts.values()
    }

    pub fn parts_mut(&mut self) -> impl ExactSizeIterator<Item = &mut Part> {
        self.parts.values_mut()
    }

    pub fn fragments(&self) -> impl ExactSizeIterator<Item = &Fragment> {
        self.fragments.values()
    }

    pub fn fragments_mut(&mut self) -> impl ExactSizeIterator<Item = &mut Fragment> {
        self.fragments.values_mut()
    }
}
