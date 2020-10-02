use lib3dmol::{
    parser::read_pdb,
    structures::{atom::AtomType, GetAtom as _},
};
use periodic_table::Element;
use render::{AtomKind, AtomRepr, Fragment, GlobalGpuResources, Part, World};
use std::path::Path;

// TODO: Better result error type.
pub fn load_from_pdb<P: AsRef<Path>>(
    gpu_resources: &GlobalGpuResources,
    name: &str,
    path: P,
) -> Result<World, String> {
    let path = path.as_ref();
    if !path.exists() {
        return Err("path does not exist".to_string());
    }

    let structure = read_pdb(&*path.to_string_lossy(), name);

    let mut world = World::new();

    let parts: Vec<Part> = structure
        .chains
        .into_iter()
        .map(|chain| {
            let fragments: Vec<_> = chain
                .lst_res
                .iter()
                .map(|residue| {
                    let atoms = residue.get_atom();
                    let atoms = atoms.iter().map(|atom| {
                        let element = atom_type_to_element(&atom.a_type);

                        AtomRepr {
                            pos: atom.coord.into(),
                            kind: AtomKind::new(element),
                        }
                    });

                    Fragment::from_atoms(gpu_resources, atoms)
                })
                .collect();

            Part::from_fragments(&mut world, fragments)
        })
        .collect();

    world.spawn_part_batch(parts).for_each(|_| {});

    Ok(world)
}

fn atom_type_to_element(atom_type: &AtomType) -> Element {
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
