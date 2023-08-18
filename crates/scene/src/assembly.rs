use std::collections::VecDeque;

use ultraviolet::Mat4;

use crate::{molecule::MoleculeRepr, Molecule};

enum ComponentType {
    Molecule(Molecule),
    SubAssembly(Assembly),
}

pub struct Component {
    transform: Mat4,
    data: ComponentType,
}

impl Component {
    pub fn from_molecule(molecule: Molecule, transform: Mat4) -> Self {
        Self {
            transform,
            data: ComponentType::Molecule(molecule),
        }
    }

    pub fn from_assembly(assembly: Assembly, transform: Mat4) -> Self {
        Self {
            transform,
            data: ComponentType::SubAssembly(assembly),
        }
    }
}

#[derive(Default)]
pub struct Assembly {
    components: Vec<Component>,
}

impl Assembly {
    pub fn from_components(components: impl IntoIterator<Item = Component>) -> Self {
        Self {
            components: components.into_iter().collect(),
        }
    }

    fn walk_mut(&self, mut f: impl FnMut(&Molecule, Mat4)) {
        let mut stack: Vec<(&Assembly, Mat4)> = vec![(self, Mat4::default())];

        while let Some((assembly, acc_transform)) = stack.pop() {
            for component in &assembly.components {
                let new_transform = component.transform * acc_transform;
                match &component.data {
                    ComponentType::Molecule(molecule) => {
                        f(molecule, new_transform);
                    }
                    ComponentType::SubAssembly(sub_assembly) => {
                        stack.push((sub_assembly, new_transform));
                    }
                }
            }
        }
    }

    pub fn collect_molecules_and_transforms(&self) -> (Vec<&MoleculeRepr>, Vec<Mat4>) {
        // The number of direct children of the world is an estimate of the
        // lower bound of the number of molecules. It is only possible for this to
        // overestimate if a child assembly contains zero children (which is unusual).
        let mut transforms = Vec::<Mat4>::with_capacity(self.components.len());
        let mut molecules = Vec::<&MoleculeRepr>::with_capacity(self.components.len());

        // DFS
        let mut stack: Vec<(&Assembly, Mat4)> = vec![(self, Mat4::default())];

        while let Some((assembly, acc_transform)) = stack.pop() {
            for component in &assembly.components {
                let new_transform = component.transform * acc_transform;
                match &component.data {
                    ComponentType::Molecule(molecule) => {
                        molecules.push(&molecule.repr);
                        transforms.push(new_transform);
                    }
                    ComponentType::SubAssembly(sub_assembly) => {
                        stack.push((sub_assembly, new_transform));
                    }
                }
            }
        }

        (molecules, transforms)
    }

    /// Recursively synchronize the atom data of each molecule to the GPU.
    pub fn synchronize_buffers(&mut self, gpu_resources: &render::GlobalRenderResources) {
        for component in self.components.iter_mut() {
            match &mut component.data {
                ComponentType::Molecule(ref mut molecule) => {
                    molecule.repr.reupload_atoms(gpu_resources);
                }
                ComponentType::SubAssembly(ref mut assembly) => {
                    assembly.synchronize_buffers(gpu_resources);
                }
            }
        }
    }

    // I don't think these have to be seperate
    // pub fn walk_molecules(&self, f: impl Fn(&Molecule)) {
    //     let mut stack: VecDeque<&Component> = self.components.iter().collect();

    //     while let Some(component) = stack.pop_back() {
    //         match &component.data {
    //             ComponentType::Molecule(molecule) => f(molecule),
    //             ComponentType::SubAssembly(assembly) => stack.extend(&assembly.components),
    //         }
    //     }
    // }
    //
    // pub fn walk_transforms(&self, f: impl Fn(Mat4)) {
    //     let mut stack: VecDeque<(&Component, Mat4)> = self
    //         .components
    //         .iter()
    //         .map(|component| (component, component.transform))
    //         .collect();

    //     while let Some((component, acc_transform)) = stack.pop_back() {
    //         match &component.data {
    //             ComponentType::Molecule(_) => {
    //                 f(acc_transform);
    //             }
    //             ComponentType::SubAssembly(sub_assembly) => {
    //                 for sub_component in &sub_assembly.components {
    //                     let new_transform = sub_component.transform * acc_transform;
    //                     stack.push_back((sub_component, new_transform));
    //                 }
    //             }
    //         }
    //     }
    // }

    // Returns a reference to a Vec storing the children that are directly owned by this
    // Assembly. This is NOT a list of every component that the assembly contains, as the
    // directly owned children might be assemblies themselves.
    pub fn direct_children(&self) -> &Vec<Component> {
        &self.components
    }
}
