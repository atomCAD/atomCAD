use crate::common::scene::Scene;
use crate::api::structure_designer::structure_designer_preferences::{StructureDesignerPreferences, AtomicRenderingMethod};
use crate::renderer::mesh::{Mesh, Material};
use crate::renderer::line_mesh::LineMesh;
use crate::renderer::atom_impostor_mesh::AtomImpostorMesh;
use crate::renderer::bond_impostor_mesh::BondImpostorMesh;
use crate::renderer::tessellator::atomic_tessellator;
use crate::renderer::tessellator::surface_point_tessellator;
use crate::renderer::tessellator::poly_mesh_tessellator::{tessellate_poly_mesh, tessellate_poly_mesh_to_line_mesh};
use crate::renderer::tessellator::tessellator::tessellate_cuboid;
use crate::renderer::camera::Camera;
use glam::f32::Vec3;
use glam::f64::{DVec3, DQuat};

/// Tessellates all scene content and returns CPU mesh data for GPU upload.
/// This is the main entry point for scene tessellation, handling both lightweight and full tessellation.
pub fn tessellate_scene_content<'a, S: Scene<'a>>(
    scene: &S,
    camera: &Camera,
    lightweight: bool,
    preferences: &StructureDesignerPreferences
) -> (Mesh, Mesh, LineMesh, Mesh, AtomImpostorMesh, BondImpostorMesh) {
    //let _timer = Timer::new("tessellate_scene_content");

    // ===== 1. TESSELLATE LIGHTWEIGHT CONTENT (always) =====
    let lightweight_mesh = tessellate_lightweight_content(scene, camera, preferences);

    // ===== 2. TESSELLATE NON-LIGHTWEIGHT CONTENT (when !lightweight) =====
    let (main_mesh, wireframe_mesh, selected_clusters_mesh, atom_impostor_mesh, bond_impostor_mesh) = 
        if !lightweight {
            tessellate_non_lightweight_content(scene, preferences)
        } else {
            // Return empty meshes when in lightweight mode
            (Mesh::new(), LineMesh::new(), Mesh::new(), AtomImpostorMesh::new(), BondImpostorMesh::new())
        };

    //println!("tessellate_scene_content took: {:?}", start_time.elapsed());
    
    (lightweight_mesh, main_mesh, wireframe_mesh, selected_clusters_mesh, atom_impostor_mesh, bond_impostor_mesh)
}

/// Tessellates lightweight content that is always rendered (tessellatable objects, camera pivot).
/// This content is rendered on top of everything else and is always updated regardless of lightweight mode.
pub fn tessellate_lightweight_content<'a, S: Scene<'a>>(
    scene: &S,
    camera: &Camera,
    preferences: &StructureDesignerPreferences
) -> Mesh {
        let mut lightweight_mesh = Mesh::new();
        
        // Tessellate extra tessellatable data
        if let Some(tessellatable) = scene.tessellatable() {
            tessellatable.tessellate(&mut lightweight_mesh);
        }
        
        // Tessellate camera pivot point cube if enabled
        if preferences.geometry_visualization_preferences.display_camera_target {
            let red_material = Material::new(
                &Vec3::new(1.0, 0.0, 0.0), // Red color
                0.5, // roughness
                0.0, // metallic
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

        //println!("lightweight tessellated {} vertices and {} indices", 
        //         lightweight_mesh.vertices.len(), lightweight_mesh.indices.len());

        lightweight_mesh
    }

/// Tessellates non-lightweight scene content based on rendering method preferences.
/// This includes atomic structures (triangle mesh or impostors), surface point clouds, and poly meshes.
/// Only executed when not in lightweight mode.
pub fn tessellate_non_lightweight_content<'a, S: Scene<'a>>(
    scene: &S,
    preferences: &StructureDesignerPreferences
) -> (Mesh, LineMesh, Mesh, AtomImpostorMesh, BondImpostorMesh) {
        let mut main_mesh = Mesh::new();
        let mut wireframe_mesh = LineMesh::new();
        let mut selected_clusters_mesh = Mesh::new();
        let mut atom_impostor_mesh = AtomImpostorMesh::new();
        let mut bond_impostor_mesh = BondImpostorMesh::new();

        let atomic_tessellation_params = atomic_tessellator::AtomicTessellatorParams {
            ball_and_stick_sphere_horizontal_divisions: 12,  // Ball-and-stick: lower resolution
            ball_and_stick_sphere_vertical_divisions: 6,     // Ball-and-stick: lower resolution
            space_filling_sphere_horizontal_divisions: 36,   // Space-filling: higher resolution for Van der Waals
            space_filling_sphere_vertical_divisions: 18,     // Space-filling: higher resolution for Van der Waals
            cylinder_divisions: 12,
        };

        // Tessellate atomic structures based on rendering method
        match preferences.atomic_structure_visualization_preferences.rendering_method {
            AtomicRenderingMethod::TriangleMesh => {
                // Traditional triangle mesh tessellation
                for atomic_structure in scene.atomic_structures() {
                    atomic_tessellator::tessellate_atomic_structure(
                        &mut main_mesh, 
                        &mut selected_clusters_mesh, 
                        atomic_structure, 
                        &atomic_tessellation_params, 
                        scene, 
                        &preferences.atomic_structure_visualization_preferences
                    );
                }
                // Note: atom_impostor_mesh and bond_impostor_mesh remain empty (will be cleared in GPU update)
            },
            AtomicRenderingMethod::Impostors => {
                // Impostor tessellation
                for atomic_structure in scene.atomic_structures() {
                    atomic_tessellator::tessellate_atomic_structure_impostors(
                        &mut atom_impostor_mesh, 
                        &mut bond_impostor_mesh, 
                        atomic_structure, 
                        scene, 
                        &preferences.atomic_structure_visualization_preferences
                    );
                }
                // Note: main_mesh and selected_clusters_mesh remain empty for atomic content
            }
        }

        // Tessellate surface point clouds (always to triangle mesh)
        for surface_point_cloud in scene.surface_point_cloud_2ds() {
            surface_point_tessellator::tessellate_surface_point_cloud_2d(&mut main_mesh, surface_point_cloud);
        }

        for surface_point_cloud in scene.surface_point_clouds() {
            surface_point_tessellator::tessellate_surface_point_cloud(&mut main_mesh, surface_point_cloud);
        }

        // Tessellate poly meshes (always to triangle/line mesh)
        for poly_mesh in scene.poly_meshes() {
            if preferences.geometry_visualization_preferences.wireframe_geometry {
                tessellate_poly_mesh_to_line_mesh(
                    &poly_mesh,
                    &mut wireframe_mesh, 
                    preferences.geometry_visualization_preferences.mesh_smoothing.clone(), 
                    Vec3::new(0.0, 0.0, 0.0).to_array(),
                    // normally normal_edge_color should be Vec3::new(0.4, 0.4, 0.4), but we do not show the difference here
                    // as csgrs sometimes creates non-manifold edges (false sharp edges) where it should not.
                    // Fortunatelly csgrs only do this on edges on a plane, so it does not matter for the
                    // solid visualization. 
                    Vec3::new(0.0, 0.0, 0.0).to_array()
                ); 
            } else {
                tessellate_poly_mesh(
                    &poly_mesh,
                    &mut main_mesh, 
                    preferences.geometry_visualization_preferences.mesh_smoothing.clone(), 
                    &Material::new(
                        &Vec3::new(0.0, 1.0, 0.0), 
                        1.0, 
                        0.0
                    ),
                    Some(&Material::new(
                        &Vec3::new(1.0, 0.0, 0.0), 
                        1.0, 
                        0.0
                    )),
                    Some(&Material::new(
                        &Vec3::new(0.0, 0.0, 1.0), 
                        1.0, 
                        0.0
                    )),
                );
            }
        }

        //println!("main buffers tessellated {} vertices and {} indices, {} bytes", 
        //         main_mesh.vertices.len(), main_mesh.indices.len(), main_mesh.memory_usage_bytes());

        (main_mesh, wireframe_mesh, selected_clusters_mesh, atom_impostor_mesh, bond_impostor_mesh)
    }
