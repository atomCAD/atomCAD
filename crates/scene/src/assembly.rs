// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use molecule::MoleculeEditor;
use render::{AtomBuffer, BondBuffer};
use ultraviolet::Mat4;

enum ComponentType {
    Molecule(Box<MoleculeEditor>),
    SubAssembly(Assembly),
}

pub struct Component {
    transform: Mat4,
    data: ComponentType,
}

impl Component {
    pub fn from_molecule(molecule: MoleculeEditor, transform: Mat4) -> Self {
        Self {
            transform,
            data: ComponentType::Molecule(Box::new(molecule)),
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

    pub fn walk_mut(&mut self, mut f: impl FnMut(&mut MoleculeEditor, Mat4)) {
        let mut stack: Vec<(&mut Assembly, Mat4)> = vec![(self, Mat4::default())];

        while let Some((assembly, acc_transform)) = stack.pop() {
            for component in &mut assembly.components {
                let new_transform = component.transform * acc_transform;
                match &mut component.data {
                    ComponentType::Molecule(ref mut molecule) => {
                        f(molecule, new_transform);
                    }
                    ComponentType::SubAssembly(sub_assembly) => {
                        stack.push((sub_assembly, new_transform));
                    }
                }
            }
        }
    }

    pub fn collect_rendering_primitives(
        &self,
    ) -> (Vec<&AtomBuffer>, Vec<Option<&BondBuffer>>, Vec<Mat4>) {
        // The number of direct children of the world is an estimate of the
        // lower bound of the number of molecules. It is only possible for this to
        // overestimate if a child assembly contains zero children (which is unusual).
        let mut transforms = Vec::<Mat4>::with_capacity(self.components.len());
        let mut atom_buffers = Vec::<&AtomBuffer>::with_capacity(self.components.len());
        let mut bond_buffers = Vec::<Option<&BondBuffer>>::with_capacity(self.components.len());

        // DFS
        let mut stack: Vec<(&Assembly, Mat4)> = vec![(self, Mat4::default())];

        while let Some((assembly, acc_transform)) = stack.pop() {
            for component in &assembly.components {
                let new_transform = component.transform * acc_transform;
                match &component.data {
                    ComponentType::Molecule(editor) => {
                        if let Some(atom_buf) = editor.repr.atoms() {
                            atom_buffers.push(atom_buf);
                            bond_buffers.push(editor.repr.bonds());
                            transforms.push(new_transform);
                        }
                    }
                    ComponentType::SubAssembly(sub_assembly) => {
                        stack.push((sub_assembly, new_transform));
                    }
                }
            }
        }

        (atom_buffers, bond_buffers, transforms)
    }

    /// Recursively synchronize the atom data of each molecule to the GPU.
    pub fn synchronize_buffers(&mut self, gpu_resources: &render::GlobalRenderResources) {
        for component in self.components.iter_mut() {
            match &mut component.data {
                ComponentType::Molecule(ref mut editor) => {
                    editor.repr.synchronize_buffers(gpu_resources);
                }
                ComponentType::SubAssembly(ref mut assembly) => {
                    assembly.synchronize_buffers(gpu_resources);
                }
            }
        }
    }

    // Returns a reference to a Vec storing the children that are directly owned by this
    // Assembly. This is NOT a list of every component that the assembly contains, as the
    // directly owned children might be assemblies themselves.
    pub fn direct_children(&self) -> &Vec<Component> {
        &self.components
    }
}
