// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bevy::prelude::*;
use bevy_mod_picking::prelude::*;
use periodic_table::{Element, PeriodicTable};
use petgraph::stable_graph::NodeIndex;
use std::collections::HashMap;

/// The integer bond order (1 indicates a single bond and so on).
type BondOrder = u8;

/// Describes how different parts of a spawned (Bevy ECS) molecule (including
/// unbonded electrons and atoms) are connected using a stable undirected
/// graph.  The molecule must be spawned, because the graph's nodes are bevy
/// `Entity` instances, and all of their data (element, position, etc) is
/// stored as the entity's components.  The edge weights represent integer
/// bond order (1 indicates a single bond and so on).  If a node of the
/// molecule graph is a `BondingSite` Entity, it must have exactly one bond,
/// and that bond must be to an `Atom` Entity.
type MolGraph = petgraph::stable_graph::StableUnGraph<Entity, BondOrder>;

/// Stores a molecule graph as a component so that molecules can be stored in
/// ECS.  This effectively allows us to use the ECS as a molecule workspace.
#[derive(Component)]
pub struct Molecule {
    graph: MolGraph,
}

/// The presence of this component means that an `Entity` models an atom in a
/// molecule.
#[derive(Component, Debug)]
struct Atom {
    #[allow(dead_code)]
    element: Element,
    // Note: Initially the position was included here for serialization
    //       purposes: you should be able to describe an atom's position
    //       without creating an entity with a transform for each atom.
    //       However, this introduces a data consistency problem - the entity
    //       transform and the atom position are conflicting and would need to
    //       be kept synchronous.  For now, we're avoiding the problem by only
    //       storing position inside of the entity transform.  This is bad
    //       long term though, as we might want double precision (not single
    //       float Vec3) relative to the COM (not relative to the world
    //       origin).
    // position: Vec3,
}

/// The presence of this component means that an `Entity` models a bonding
/// site in a molecule.
#[derive(Component, Debug)]
pub struct BondingSite {
    node_index: NodeIndex,
    // Note: See `Atom` to know why this is commented out.
    // position: Vec3
}

/// Stores PbrBundles that are often duplicated, namely for things like atoms
/// and bonding sites. Note that cloning a PbrBundle only clones a `Handle` of
/// the Mesh and Material, so it is very cheap to clone this struct's members
/// when you need ownership.
// TODO: this may be redundant - I think `Assets` serves a very similar purpose
#[derive(Resource)]
pub struct PbrCache {
    ptable: PeriodicTable,
    bonding_site: PbrBundle,
    atoms: HashMap<Element, PbrBundle>,
}

const ATOM_HIGHLIGHTING: Highlight<StandardMaterial> = Highlight {
    hovered: Some(HighlightKind::new_dynamic(|orig| StandardMaterial {
        base_color: orig.base_color * 2.,
        ..orig.to_owned()
    })),
    pressed: Some(HighlightKind::new_dynamic(|orig| StandardMaterial {
        base_color: orig.base_color * 6.,
        ..orig.to_owned()
    })),
    selected: Some(HighlightKind::new_dynamic(|orig| StandardMaterial {
        base_color: orig.base_color * 1.5,
        ..orig.to_owned()
    })),
};

/// The `ClickFlag` indicates that the user clicked on an `Entity`, and that
/// this event is yet to be handled. Remove this flag if your system handled
/// the click.
#[derive(Clone, Component, Default)]
pub struct ClickFlag;

/// Implements a simple molecule builder system that allows users to click
/// bonding sites to create new atoms and bonds.  Supports multiple molecules
/// in the same world, because each `Molecule` is stored in ECS as a separate
/// entity.
pub fn molecule_builder(
    mut commands: Commands,
    mut query: Query<(Entity, &Parent, &Transform, &BondingSite), With<ClickFlag>>,
    mut q_parent: Query<&mut Molecule>,
    pbr_cache: Res<PbrCache>,
) {
    for (entity, parent, transform, clicked_bonding_site) in query.iter_mut() {
        // Retrieve the parent of the clicked particle - i.e. its molecule
        let molecule: &mut Molecule = q_parent.get_mut(parent.get()).unwrap().into_inner();

        let clicked_index = clicked_bonding_site.node_index;

        // Get the atom this bonding site was connected to before removing it
        // (recall that we demand that all bonding sites have exactly one
        // neighbor)
        let bond_target = molecule.graph.neighbors(clicked_index).next().unwrap();
        molecule.graph.remove_node(clicked_index);
        commands.entity(entity).despawn();

        // Place a new atom
        let mut new_atom = pbr_cache.atoms[&Element::Carbon].clone();
        new_atom.transform = *transform;
        let new_atom = commands
            .spawn((
                new_atom,
                Atom {
                    element: Element::Carbon,
                },
                PickableBundle::default(),
                ATOM_HIGHLIGHTING,
            ))
            .id();

        // Add a new binding site
        let carbon = &pbr_cache.ptable.element_reprs[Element::Carbon as usize - 1];
        let mut bonding_site = pbr_cache.bonding_site.clone();
        bonding_site.transform = *transform;
        bonding_site.transform.translation += Vec3::new(carbon.radius + 0.5, 0.0, 0.0);
        let bonding_site = commands
            .spawn((
                bonding_site,
                PickableBundle::default(),
                On::<Pointer<Click>>::target_commands_mut(|_event, target_commands| {
                    target_commands.insert(ClickFlag);
                }),
                ATOM_HIGHLIGHTING,
            ))
            .id();

        // Store the graph indexes needed
        let new_atom_index = molecule.graph.add_node(new_atom);
        let bonding_site_index = molecule.graph.add_node(bonding_site);

        // Add a BondingSite component to the entity so that it can track this
        commands.entity(bonding_site).insert(BondingSite {
            node_index: bonding_site_index,
        });

        // Add a single bond between the atom and new bonding site
        molecule
            .graph
            .add_edge(new_atom_index, bonding_site_index, 1);

        // Add a single bond between the old atom and this atom:
        molecule.graph.add_edge(new_atom_index, bond_target, 1);

        commands
            .entity(parent.get())
            .push_children(&[new_atom, bonding_site]);
    }
}

pub fn init_molecule(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut pbr_cache = PbrCache {
        ptable: PeriodicTable::new(),
        atoms: HashMap::new(),
        bonding_site: PbrBundle {
            mesh: meshes.add(Mesh::from(shape::UVSphere {
                radius: 0.3,
                sectors: 24,
                stacks: 24,
            })),
            material: materials.add(StandardMaterial::from(Color::rgb(0.6, 0.0, 0.0))),
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            ..default()
        },
    };

    let carbon = &pbr_cache.ptable.element_reprs[Element::Carbon as usize - 1];
    pbr_cache.atoms.insert(
        Element::Carbon,
        PbrBundle {
            mesh: meshes.add(Mesh::from(shape::UVSphere {
                radius: carbon.radius,
                sectors: 60,
                stacks: 60,
            })),
            material: materials.add(StandardMaterial::from(Color::rgb(
                carbon.color[0],
                carbon.color[1],
                carbon.color[2],
            ))),
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            ..default()
        },
    );

    // Create an initial carbon atom
    let carbon_pbr = pbr_cache.atoms[&Element::Carbon].clone();
    let initial_carbon = commands
        .spawn((
            carbon_pbr,
            PickableBundle::default(),
            ATOM_HIGHLIGHTING,
            Atom {
                element: Element::Carbon,
            },
        ))
        .id();

    // Create a bonding site
    let mut initial_bonding_site_pbr = pbr_cache.bonding_site.clone();
    initial_bonding_site_pbr.transform.translation = Vec3::new(carbon.radius + 0.5, 0.0, 0.0);
    let initial_bonding_site = commands
        .spawn((
            initial_bonding_site_pbr,
            PickableBundle::default(),
            On::<Pointer<Click>>::target_commands_mut(|_event, target_commands| {
                target_commands.insert(ClickFlag);
            }),
            ATOM_HIGHLIGHTING,
        ))
        .id();

    commands.insert_resource(pbr_cache);

    // Build the test molecule's graph
    let mut molgraph = MolGraph::default();

    // Store the graph indexes needed
    let i1 = molgraph.add_node(initial_carbon);
    let i2 = molgraph.add_node(initial_bonding_site);

    // Add a BondingSite component to the entity so that it can track this
    commands
        .entity(initial_bonding_site)
        .insert(BondingSite { node_index: i2 });

    // Add a single bond between them
    molgraph.add_edge(i1, i2, 1);

    // Create a molecule entity backed by the molecule graph - this
    // will allow us to use the ECS as a molecule database and give us
    // unique identifiers for each molecule
    let mut molecule = commands.spawn((
        Molecule { graph: molgraph },
        // The spatial bundle contains both the Bevy-required scenegraph
        // information including view transform and visibility flags.
        SpatialBundle::default(),
    ));

    // Make the displayed atom gameobjects children of the molecule, allowing
    // the molecule to be recovered when an atom is picked
    molecule.push_children(&[initial_carbon, initial_bonding_site]);
}

// End of File
