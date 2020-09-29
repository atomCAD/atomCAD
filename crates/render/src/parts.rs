use crate::{
    atoms::{AtomKind, AtomRepr, Atoms},
    utils::BoundingBox,
};
use periodic_table::Element;
use rand::{
    distributions::{Distribution, Uniform as RandUniform},
    seq::SliceRandom as _,
};
use std::{iter, path::Path};
use ultraviolet::Vec3;

pub struct Fragment {
    atoms: Atoms,

    bounding_box: BoundingBox,
    center: Vec3, // not sure what type of center yet (median, initial atom, etc)
}

impl Fragment {
    fn new_mock(device: &wgpu::Device, bgl: &crate::BindGroupLayouts) -> Self {
        let mut rng = rand::thread_rng();
        let position_sampler = RandUniform::from(-10.0..10.0);
        let allowed_elements = [
            Element::Hydrogen,
            Element::Carbon,
            Element::Oxygen,
            Element::Silicon,
        ];

        let atoms = (0..100).map(|_| AtomRepr {
            pos: Vec3::new(
                position_sampler.sample(&mut rng),
                position_sampler.sample(&mut rng),
                position_sampler.sample(&mut rng),
            ),
            kind: AtomKind::new(*allowed_elements.choose(&mut rng).unwrap()),
        });

        Self::from_atoms(device, bgl, atoms)
    }

    pub fn from_atoms<I>(device: &wgpu::Device, bgl: &crate::BindGroupLayouts, atoms: I) -> Self
    where
        I: IntoIterator<Item = AtomRepr>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut point_sum = Vec3::zero();
        let mut max_point = Vec3::new(-f32::INFINITY, -f32::INFINITY, -f32::INFINITY);
        let mut min_point = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);

        let atoms = Atoms::new(
            device,
            bgl,
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
    fragments: Vec<Fragment>,
    bounding_box: BoundingBox,
    center: Vec3,
}

impl Part {
    /// Create some mock data.
    pub fn new_mock(device: &wgpu::Device, bgl: &crate::BindGroupLayouts) -> Self {
        let fragment = Fragment::new_mock(device, bgl);

        Self::from_fragments(iter::once(fragment))
    }

    pub fn from_fragments<I>(fragments: I) -> Self
    where
        I: IntoIterator<Item = Fragment>,
    {
        let mut bounding_box = BoundingBox {
            min: Vec3::new(-f32::INFINITY, -f32::INFINITY, -f32::INFINITY),
            max: Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
        };
        let mut center = Vec3::zero();

        let fragments: Vec<_> = fragments
            .into_iter()
            .inspect(|fragment| {
                bounding_box = bounding_box.union(&fragment.bounding_box);
                center += fragment.center;
            })
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

    pub fn fragments(&self) -> &[Fragment] {
        &self.fragments
    }

    pub fn offset_by(&mut self, offset: Vec3) {}

    pub fn move_to(&mut self, point: Vec3) {}
}

pub struct Parts {
    parts: Vec<Part>,
}

impl Parts {
    // TODO: Better result error type.
    pub fn load_from_pdb<P: AsRef<Path>>(
        device: &wgpu::Device,
        bgl: &crate::BindGroupLayouts,
        name: &str,
        path: P,
    ) -> Result<Self, String> {
        use lib3dmol::{parser::read_pdb, structures::GetAtom as _};

        let path = path.as_ref();
        if !path.exists() {
            return Err("path does not exist".to_string());
        }

        let structure = read_pdb(&*path.to_string_lossy(), name);

        let parts = structure
            .chains
            .iter()
            .map(|chain| {
                let fragments = chain.lst_res.iter().map(|residue| {
                    let atoms = residue.get_atom();
                    let atoms = atoms.iter().map(|atom| {
                        let element = atom_type_to_element(&atom.a_type);

                        AtomRepr {
                            pos: atom.coord.into(),
                            kind: AtomKind::new(element),
                        }
                    });

                    Fragment::from_atoms(device, bgl, atoms)
                });

                Part::from_fragments(fragments)
            })
            .collect();

        Ok(Self { parts })
    }

    pub fn iter(&self) -> impl Iterator<Item = &Part> {
        self.parts.iter()
    }
}

impl iter::FromIterator<Part> for Parts {
    fn from_iter<T: IntoIterator<Item = Part>>(iter: T) -> Self {
        Self {
            parts: iter.into_iter().collect(),
        }
    }
}

fn atom_type_to_element(atom_type: &lib3dmol::structures::atom::AtomType) -> Element {
    use lib3dmol::structures::atom::AtomType;
    match atom_type {
        AtomType::Hydrogen => Element::Hydrogen,
        AtomType::Carbon => Element::Carbon,
        AtomType::Oxygen => Element::Oxygen,
        AtomType::Silicon => Element::Silicon,
        AtomType::Phosphorus => Element::Phosphorus,
        AtomType::Nitrogen => Element::Nitrogen,
        AtomType::Sulfur => Element::Sulfur,
        _ => Element::MAX,
    }
}
