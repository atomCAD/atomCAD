use crate::{
    atoms::{AtomRepr, Atoms},
    utils::BoundingBox,
    GlobalRenderResources,
};
use common::AsBytes;
use indexmap::IndexMap;
use std::{
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU64, Ordering},
};
use ultraviolet::{Rotor3, Vec3};

macro_rules! declare_id {
    ($id_name:ident) => {
        #[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
        #[repr(transparent)]
        pub struct $id_name(u64);

        unsafe impl AsBytes for $id_name {}

        impl $id_name {
            pub fn new() -> Self {
                static COUNTER: AtomicU64 = AtomicU64::new(1);

                let id = COUNTER.fetch_add(1, Ordering::Relaxed);
                Self(id)
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
    pub fn from_atoms<I>(gpu_resources: &GlobalRenderResources, atoms: I) -> Self
    where
        I: IntoIterator<Item = AtomRepr>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut point_sum = Vec3::zero();
        let mut max_point = Vec3::new(-f32::INFINITY, -f32::INFINITY, -f32::INFINITY);
        let mut min_point = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);

        let fragment_id = FragmentId::new();

        let atoms = Atoms::new(
            gpu_resources,
            fragment_id,
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
            id: fragment_id,
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

    pub fn copy_new(&self, render_resources: &GlobalRenderResources) -> Self {
        let id = FragmentId::new();
        Self {
            id,
            atoms: self.atoms.copy_new(render_resources, id),
            ..*self
        }
    }
}

pub struct Part {
    name: String,
    id: PartId,
    fragments: Vec<FragmentId>,
    bounding_box: BoundingBox,
    center: Vec3,
    offset: Vec3,
    rotation: Rotor3,
}

impl Part {
    pub fn from_fragments<S, I>(world: &mut World, name: S, fragments: I) -> Self
    where
        S: ToString,
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

        let center = center / fragments.len() as f32;

        assert!(
            fragments.len() > 0,
            "must have at least one fragment in a part"
        );

        Part {
            name: name.to_string(),
            id: part_id,
            fragments,
            bounding_box,
            center,
            offset: Vec3::zero(),
            rotation: Rotor3::default(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
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

    pub fn offset_by(&mut self, x: f32, y: f32, z: f32) {
        self.offset += Vec3::new(x, y, z);
    }

    pub fn move_to(&mut self, x: f32, y: f32, z: f32) {
        self.offset = Vec3::new(x, y, z) - self.center;
    }

    /// Takes angles in degrees.
    pub fn rotate_by(&mut self, roll: f32, pitch: f32, yaw: f32) {
        self.rotation =
            Rotor3::from_euler_angles(roll.to_radians(), pitch.to_radians(), yaw.to_radians())
                * self.rotation;
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
    pub(crate) modified_fragments: Vec<FragmentId>,
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

    pub fn copy_part(
        &mut self,
        render_resources: &GlobalRenderResources,
        part_id: PartId,
    ) -> PartId {
        let part = &self.parts[&part_id];
        let id = PartId::new();
        let name = format!("{} (Copy)", part.name);

        let mut fragments = Vec::new();

        for fragment_id in &part.fragments {
            let fragment = &self.fragments[fragment_id];
            let fragment = fragment.copy_new(render_resources);

            let id = fragment.id;
            self.fragments.insert(id, fragment);
            self.added_fragments.push((part_id, id));

            fragments.push(id);
        }

        let part = Part {
            id,
            name,
            fragments,
            ..*part
        };

        self.spawn_part(part)
    }

    pub fn spawn_part(&mut self, part: Part) -> PartId {
        let id = part.id;
        assert!(self.parts.insert(id, part).is_none());
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

    pub fn part_mut(&mut self, id: PartId) -> &mut Part {
        let part = &mut self.parts[&id];
        self.modified_parts.push(id);
        part
    }

    pub fn find_part<S: AsRef<str>>(&self, name: S) -> Option<PartId> {
        let name = name.as_ref();
        self.parts
            .iter()
            .find(|(_, p)| p.name == name)
            .map(|(&id, _)| id)
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
