// Direct tessellation from StructureDesignerScene (no Scene trait needed)

use crate::display::atomic_tessellator;
use crate::display::coordinate_system_tessellator;
use crate::display::poly_mesh_tessellator::{
    tessellate_poly_mesh, tessellate_poly_mesh_to_line_mesh,
};
use crate::display::preferences::{AtomicRenderingMethod, DisplayPreferences};
use crate::display::surface_point_tessellator;
use crate::renderer::atom_impostor_mesh::AtomImpostorMesh;
use crate::renderer::bond_impostor_mesh::BondImpostorMesh;
use crate::renderer::camera::Camera;
use crate::renderer::line_mesh::LineMesh;
use crate::renderer::mesh::{Material, Mesh};
use crate::renderer::tessellator::tessellator::{TessellationOutput, tessellate_cuboid};
use crate::structure_designer::structure_designer_scene::{NodeOutput, StructureDesignerScene};
use glam::f32::Vec3;
use glam::f64::{DQuat, DVec3};

/// Tessellates all scene content using the new node_data HashMap structure
/// This is the main entry point for scene tessellation
pub fn tessellate_scene_content(
    scene: &StructureDesignerScene,
    camera: &Camera,
    lightweight: bool,
    preferences: &DisplayPreferences,
) -> (
    Mesh,
    LineMesh,
    Mesh,
    LineMesh,
    AtomImpostorMesh,
    BondImpostorMesh,
) {
    // ===== 1. TESSELLATE LIGHTWEIGHT CONTENT (always) =====
    let (lightweight_mesh, gadget_line_mesh) =
        tessellate_lightweight_content(scene, camera, preferences);

    // ===== 2. TESSELLATE NON-LIGHTWEIGHT CONTENT (when !lightweight) =====
    let (main_mesh, wireframe_mesh, atom_impostor_mesh, bond_impostor_mesh) = if !lightweight {
        tessellate_non_lightweight_content(scene, preferences)
    } else {
        // Return empty meshes for non-lightweight content
        (
            Mesh::new(),
            LineMesh::new(),
            AtomImpostorMesh::new(),
            BondImpostorMesh::new(),
        )
    };

    (
        lightweight_mesh,
        gadget_line_mesh,
        main_mesh,
        wireframe_mesh,
        atom_impostor_mesh,
        bond_impostor_mesh,
    )
}

/// Tessellates lightweight content (gadget, camera pivot)
fn tessellate_lightweight_content(
    scene: &StructureDesignerScene,
    camera: &Camera,
    preferences: &DisplayPreferences,
) -> (Mesh, LineMesh) {
    let mut output = TessellationOutput::new();

    // Tessellate gadget (tessellatable)
    if let Some(tessellatable) = &scene.tessellatable {
        tessellatable.tessellate(&mut output);
    }

    let mut lightweight_mesh = output.mesh;
    let gadget_line_mesh = output.line_mesh;

    // Tessellate camera pivot point cube if enabled
    if preferences.geometry_visualization.display_camera_target {
        let red_material = Material::new(
            &Vec3::new(1.0, 0.0, 0.0), // Red color
            0.5,                       // roughness
            0.0,                       // metallic
        );
        tessellate_cuboid(
            &mut lightweight_mesh,
            &camera.pivot_point,
            &DVec3::new(0.4, 0.4, 0.4),
            &DQuat::IDENTITY,
            &red_material,
            &red_material,
            &red_material,
        );
    }

    (lightweight_mesh, gadget_line_mesh)
}

/// Tessellates non-lightweight content by iterating over node_data HashMap
fn tessellate_non_lightweight_content(
    scene: &StructureDesignerScene,
    preferences: &DisplayPreferences,
) -> (Mesh, LineMesh, AtomImpostorMesh, BondImpostorMesh) {
    let mut main_mesh = Mesh::new();
    let mut wireframe_mesh = LineMesh::new();
    let mut atom_impostor_mesh = AtomImpostorMesh::new();
    let mut bond_impostor_mesh = BondImpostorMesh::new();

    let atomic_tessellation_params = atomic_tessellator::AtomicTessellatorParams {
        ball_and_stick_sphere_horizontal_divisions: 12,
        ball_and_stick_sphere_vertical_divisions: 6,
        space_filling_sphere_horizontal_divisions: 36,
        space_filling_sphere_vertical_divisions: 18,
        cylinder_divisions: 12,
    };

    // Iterate over all node data and tessellate based on output type
    for node_data in scene.node_data.values() {
        match &node_data.output {
            NodeOutput::Atomic(atomic_structure) => {
                // Tessellate atomic structures based on rendering method
                match preferences.atomic_structure_visualization.rendering_method {
                    AtomicRenderingMethod::TriangleMesh => {
                        atomic_tessellator::tessellate_atomic_structure(
                            &mut main_mesh,
                            atomic_structure,
                            &atomic_tessellation_params,
                            &preferences.atomic_structure_visualization,
                        );
                        // Render guide placement visuals (guide dots + anchor arrows)
                        if let Some(visuals) =
                            &atomic_structure.decorator().guide_placement_visuals
                        {
                            atomic_tessellator::tessellate_guide_placement(
                                &mut main_mesh,
                                visuals,
                                &atomic_tessellation_params,
                            );
                        }
                    }
                    AtomicRenderingMethod::Impostors => {
                        atomic_tessellator::tessellate_atomic_structure_impostors(
                            &mut atom_impostor_mesh,
                            &mut bond_impostor_mesh,
                            atomic_structure,
                            &preferences.atomic_structure_visualization,
                        );
                        // Render guide placement visuals (guide dots + anchor arrows)
                        if let Some(visuals) =
                            &atomic_structure.decorator().guide_placement_visuals
                        {
                            atomic_tessellator::tessellate_guide_placement_impostors(
                                &mut atom_impostor_mesh,
                                &mut bond_impostor_mesh,
                                visuals,
                            );
                        }
                    }
                }
            }

            NodeOutput::SurfacePointCloud(point_cloud) => {
                surface_point_tessellator::tessellate_surface_point_cloud(
                    &mut main_mesh,
                    point_cloud,
                );
            }

            NodeOutput::SurfacePointCloud2D(point_cloud_2d) => {
                surface_point_tessellator::tessellate_surface_point_cloud_2d(
                    &mut main_mesh,
                    point_cloud_2d,
                );
            }

            NodeOutput::PolyMesh(poly_mesh) => {
                if preferences.geometry_visualization.wireframe_geometry {
                    tessellate_poly_mesh_to_line_mesh(
                        poly_mesh,
                        &mut wireframe_mesh,
                        preferences.geometry_visualization.mesh_smoothing.clone(),
                        Vec3::new(1.0, 1.0, 1.0).to_array(),
                        Vec3::new(1.0, 1.0, 1.0).to_array(),
                    );
                } else {
                    tessellate_poly_mesh(
                        poly_mesh,
                        &mut main_mesh,
                        preferences.geometry_visualization.mesh_smoothing.clone(),
                        &Material::new(&Vec3::new(0.0, 1.0, 0.0), 1.0, 0.0),
                        Some(&Material::new(&Vec3::new(1.0, 0.0, 0.0), 1.0, 0.0)),
                        Some(&Material::new(&Vec3::new(0.0, 0.0, 1.0), 1.0, 0.0)),
                    );
                }
            }

            NodeOutput::DrawingPlane(drawing_plane) => {
                coordinate_system_tessellator::tessellate_drawing_plane_grid_and_axes(
                    &mut wireframe_mesh,
                    drawing_plane,
                    &preferences.background,
                );
            }

            NodeOutput::None => {
                // No renderable output for this node
            }
        }
    }

    (
        main_mesh,
        wireframe_mesh,
        atom_impostor_mesh,
        bond_impostor_mesh,
    )
}
