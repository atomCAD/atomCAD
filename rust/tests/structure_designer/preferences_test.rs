//! Tests for preferences persistence (load/save to config file).

use rust_lib_flutter_cad::api::common_api_types::APIIVec3;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_preferences::{
    AtomicRenderingMethod, AtomicStructureVisualization, AtomicStructureVisualizationPreferences,
    BackgroundPreferences, GeometryVisualization, GeometryVisualizationPreferences,
    LayoutAlgorithmPreference, LayoutPreferences, MeshSmoothing, NodeDisplayPolicy,
    NodeDisplayPreferences, SimulationPreferences, StructureDesignerPreferences,
};

/// Test round-trip serialization: serialize preferences to JSON and deserialize back.
#[test]
fn test_preferences_roundtrip_serialization() {
    let prefs = StructureDesignerPreferences::default();

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&prefs).expect("Failed to serialize preferences");

    // Deserialize back
    let loaded: StructureDesignerPreferences =
        serde_json::from_str(&json).expect("Failed to deserialize preferences");

    // Verify key fields match
    assert_eq!(
        loaded
            .geometry_visualization_preferences
            .geometry_visualization,
        GeometryVisualization::ExplicitMesh
    );
    assert_eq!(
        loaded.node_display_preferences.display_policy,
        NodeDisplayPolicy::Manual
    );
    assert_eq!(
        loaded
            .atomic_structure_visualization_preferences
            .visualization,
        AtomicStructureVisualization::BallAndStick
    );
    assert_eq!(
        loaded.layout_preferences.layout_algorithm,
        LayoutAlgorithmPreference::Sugiyama
    );
}

/// Test forward compatibility: loading JSON with missing fields should use defaults.
#[test]
fn test_preferences_missing_fields_use_defaults() {
    // JSON with only partial data (missing most fields)
    let partial_json = r#"{
        "geometry_visualization_preferences": {
            "geometry_visualization": "SurfaceSplatting"
        }
    }"#;

    let loaded: StructureDesignerPreferences =
        serde_json::from_str(partial_json).expect("Failed to deserialize partial preferences");

    // The specified field should be loaded
    assert_eq!(
        loaded
            .geometry_visualization_preferences
            .geometry_visualization,
        GeometryVisualization::SurfaceSplatting
    );

    // Missing fields should get defaults
    assert!(!loaded.geometry_visualization_preferences.wireframe_geometry);
    assert_eq!(
        loaded
            .geometry_visualization_preferences
            .samples_per_unit_cell,
        1
    );
    assert_eq!(
        loaded.geometry_visualization_preferences.mesh_smoothing,
        MeshSmoothing::SmoothingGroupBased
    );

    // Missing top-level sections should get defaults
    assert_eq!(
        loaded.node_display_preferences.display_policy,
        NodeDisplayPolicy::Manual
    );
    assert_eq!(
        loaded.layout_preferences.layout_algorithm,
        LayoutAlgorithmPreference::Sugiyama
    );
}

/// Test backward compatibility: loading JSON with extra fields should ignore them.
#[test]
fn test_preferences_extra_fields_ignored() {
    let json_with_extra = r#"{
        "geometry_visualization_preferences": {
            "geometry_visualization": "ExplicitMesh",
            "wireframe_geometry": false,
            "samples_per_unit_cell": 1,
            "sharpness_angle_threshold_degree": 29.0,
            "mesh_smoothing": "SmoothingGroupBased",
            "display_camera_target": false,
            "some_future_field": "some_value",
            "another_future_field": 42
        },
        "node_display_preferences": {
            "display_policy": "Manual"
        },
        "atomic_structure_visualization_preferences": {
            "visualization": "BallAndStick",
            "rendering_method": "Impostors",
            "ball_and_stick_cull_depth": 8.0,
            "space_filling_cull_depth": 3.0
        },
        "background_preferences": {
            "background_color": { "x": 0, "y": 0, "z": 0 },
            "show_grid": true,
            "grid_size": 200,
            "grid_color": { "x": 90, "y": 90, "z": 90 },
            "grid_strong_color": { "x": 180, "y": 180, "z": 180 },
            "show_lattice_axes": true,
            "show_lattice_grid": false,
            "lattice_grid_color": { "x": 60, "y": 90, "z": 90 },
            "lattice_grid_strong_color": { "x": 100, "y": 150, "z": 150 },
            "drawing_plane_grid_color": { "x": 70, "y": 70, "z": 100 },
            "drawing_plane_grid_strong_color": { "x": 110, "y": 110, "z": 160 }
        },
        "layout_preferences": {
            "layout_algorithm": "Sugiyama",
            "auto_layout_after_edit": true
        },
        "completely_unknown_section": {
            "data": "ignored"
        }
    }"#;

    // Should parse successfully, ignoring extra fields
    let loaded: StructureDesignerPreferences = serde_json::from_str(json_with_extra)
        .expect("Failed to deserialize preferences with extra fields");

    // Known fields should be loaded correctly
    assert_eq!(
        loaded
            .geometry_visualization_preferences
            .geometry_visualization,
        GeometryVisualization::ExplicitMesh
    );
}

/// Test handling of corrupted/invalid JSON: should fail to parse.
#[test]
fn test_preferences_corrupted_json_fails() {
    let corrupted_json = "{ this is not valid json }";

    let result: Result<StructureDesignerPreferences, _> = serde_json::from_str(corrupted_json);
    assert!(result.is_err(), "Corrupted JSON should fail to parse");
}

/// Test empty JSON object: should use all defaults.
#[test]
fn test_preferences_empty_json_uses_defaults() {
    let empty_json = "{}";

    let loaded: StructureDesignerPreferences =
        serde_json::from_str(empty_json).expect("Failed to deserialize empty preferences");

    // Should be equivalent to default
    let default_prefs = StructureDesignerPreferences::default();

    assert_eq!(
        loaded
            .geometry_visualization_preferences
            .geometry_visualization,
        default_prefs
            .geometry_visualization_preferences
            .geometry_visualization
    );
    assert_eq!(
        loaded.layout_preferences.layout_algorithm,
        default_prefs.layout_preferences.layout_algorithm
    );
}

/// Test that Default trait implementations are consistent with documentation.
#[test]
fn test_default_values_match_documentation() {
    let prefs = StructureDesignerPreferences::default();

    // Geometry visualization defaults
    assert_eq!(
        prefs
            .geometry_visualization_preferences
            .geometry_visualization,
        GeometryVisualization::ExplicitMesh
    );
    assert!(!prefs.geometry_visualization_preferences.wireframe_geometry);
    assert_eq!(
        prefs
            .geometry_visualization_preferences
            .samples_per_unit_cell,
        1
    );
    assert_eq!(
        prefs
            .geometry_visualization_preferences
            .sharpness_angle_threshold_degree,
        29.0
    );
    assert_eq!(
        prefs.geometry_visualization_preferences.mesh_smoothing,
        MeshSmoothing::SmoothingGroupBased
    );
    assert!(
        !prefs
            .geometry_visualization_preferences
            .display_camera_target
    );

    // Node display defaults
    assert_eq!(
        prefs.node_display_preferences.display_policy,
        NodeDisplayPolicy::Manual
    );

    // Atomic visualization defaults
    assert_eq!(
        prefs
            .atomic_structure_visualization_preferences
            .visualization,
        AtomicStructureVisualization::BallAndStick
    );
    assert_eq!(
        prefs
            .atomic_structure_visualization_preferences
            .rendering_method,
        AtomicRenderingMethod::Impostors
    );
    assert_eq!(
        prefs
            .atomic_structure_visualization_preferences
            .ball_and_stick_cull_depth,
        Some(8.0)
    );
    assert_eq!(
        prefs
            .atomic_structure_visualization_preferences
            .space_filling_cull_depth,
        Some(3.0)
    );

    // Background defaults
    assert_eq!(
        prefs.background_preferences.background_color,
        APIIVec3 { x: 0, y: 0, z: 0 }
    );
    assert!(prefs.background_preferences.show_grid);
    assert_eq!(prefs.background_preferences.grid_size, 200);
    assert!(prefs.background_preferences.show_lattice_axes);
    assert!(!prefs.background_preferences.show_lattice_grid);

    // Layout defaults
    assert_eq!(
        prefs.layout_preferences.layout_algorithm,
        LayoutAlgorithmPreference::Sugiyama
    );
    assert!(prefs.layout_preferences.auto_layout_after_edit);
}

/// Test serialization of non-default values.
#[test]
fn test_non_default_values_roundtrip() {
    let prefs = StructureDesignerPreferences {
        geometry_visualization_preferences: GeometryVisualizationPreferences {
            geometry_visualization: GeometryVisualization::SurfaceSplatting,
            wireframe_geometry: true,
            samples_per_unit_cell: 3,
            sharpness_angle_threshold_degree: 45.0,
            mesh_smoothing: MeshSmoothing::Sharp,
            display_camera_target: true,
        },
        node_display_preferences: NodeDisplayPreferences {
            display_policy: NodeDisplayPolicy::PreferFrontier,
        },
        atomic_structure_visualization_preferences: AtomicStructureVisualizationPreferences {
            visualization: AtomicStructureVisualization::SpaceFilling,
            rendering_method: AtomicRenderingMethod::TriangleMesh,
            ball_and_stick_cull_depth: Some(10.0),
            space_filling_cull_depth: None,
        },
        background_preferences: BackgroundPreferences {
            background_color: APIIVec3 {
                x: 255,
                y: 128,
                z: 64,
            },
            show_axes: false,
            show_grid: false,
            grid_size: 100,
            grid_color: APIIVec3 {
                x: 50,
                y: 50,
                z: 50,
            },
            grid_strong_color: APIIVec3 {
                x: 100,
                y: 100,
                z: 100,
            },
            show_lattice_axes: false,
            show_lattice_grid: true,
            lattice_grid_color: APIIVec3 {
                x: 30,
                y: 60,
                z: 60,
            },
            lattice_grid_strong_color: APIIVec3 {
                x: 80,
                y: 120,
                z: 120,
            },
            drawing_plane_grid_color: APIIVec3 {
                x: 50,
                y: 50,
                z: 80,
            },
            drawing_plane_grid_strong_color: APIIVec3 {
                x: 90,
                y: 90,
                z: 130,
            },
        },
        layout_preferences: LayoutPreferences {
            layout_algorithm: LayoutAlgorithmPreference::TopologicalGrid,
            auto_layout_after_edit: false,
        },
        simulation_preferences: SimulationPreferences {
            use_vdw_cutoff: true,
        },
    };

    // Roundtrip
    let json = serde_json::to_string(&prefs).expect("Failed to serialize");
    let loaded: StructureDesignerPreferences =
        serde_json::from_str(&json).expect("Failed to deserialize");

    // Verify all non-default values are preserved
    assert_eq!(
        loaded
            .geometry_visualization_preferences
            .geometry_visualization,
        GeometryVisualization::SurfaceSplatting
    );
    assert!(loaded.geometry_visualization_preferences.wireframe_geometry);
    assert_eq!(
        loaded
            .geometry_visualization_preferences
            .samples_per_unit_cell,
        3
    );
    assert_eq!(
        loaded
            .geometry_visualization_preferences
            .sharpness_angle_threshold_degree,
        45.0
    );
    assert_eq!(
        loaded.geometry_visualization_preferences.mesh_smoothing,
        MeshSmoothing::Sharp
    );
    assert!(
        loaded
            .geometry_visualization_preferences
            .display_camera_target
    );

    assert_eq!(
        loaded.node_display_preferences.display_policy,
        NodeDisplayPolicy::PreferFrontier
    );

    assert_eq!(
        loaded
            .atomic_structure_visualization_preferences
            .visualization,
        AtomicStructureVisualization::SpaceFilling
    );
    assert_eq!(
        loaded
            .atomic_structure_visualization_preferences
            .rendering_method,
        AtomicRenderingMethod::TriangleMesh
    );
    assert_eq!(
        loaded
            .atomic_structure_visualization_preferences
            .ball_and_stick_cull_depth,
        Some(10.0)
    );
    assert_eq!(
        loaded
            .atomic_structure_visualization_preferences
            .space_filling_cull_depth,
        None
    );

    assert_eq!(
        loaded.background_preferences.background_color,
        APIIVec3 {
            x: 255,
            y: 128,
            z: 64
        }
    );
    assert!(!loaded.background_preferences.show_grid);
    assert_eq!(loaded.background_preferences.grid_size, 100);
    assert!(!loaded.background_preferences.show_lattice_axes);
    assert!(loaded.background_preferences.show_lattice_grid);

    assert_eq!(
        loaded.layout_preferences.layout_algorithm,
        LayoutAlgorithmPreference::TopologicalGrid
    );
    assert!(!loaded.layout_preferences.auto_layout_after_edit);

    assert!(loaded.simulation_preferences.use_vdw_cutoff);
}
