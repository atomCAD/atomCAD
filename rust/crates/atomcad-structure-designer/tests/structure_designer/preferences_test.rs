//! Tests for preferences persistence (load/save to config file).

use atomcad_crystolecule::visualization::AtomicStructureVisualization;
use atomcad_structure_designer::preferences::{
    AtomicRenderingMethod, AtomicStructureVisualizationPreferences, BackgroundPreferences,
    GeometryVisualization, GeometryVisualizationPreferences, LayoutAlgorithmPreference,
    LayoutPreferences, MemoryPreferences, MeshSmoothing, NodeDisplayPolicy, NodeDisplayPreferences,
    PrefColor, SimulationPreferences, StructureDesignerPreferences,
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

/// A settings file written before `label_scale` existed must still load, with
/// the new field defaulting — this is what makes the atom-labels preference a
/// no-migration change (`doc/design_atom_labels.md` §Label size). The section is
/// otherwise fully populated, so only the missing field can be under test.
#[test]
fn test_preferences_without_label_scale_defaults_it() {
    let pre_label_json = r#"{
        "atomic_structure_visualization_preferences": {
            "visualization": "SpaceFilling",
            "rendering_method": "TriangleMesh",
            "ball_and_stick_cull_depth": 9.0,
            "space_filling_cull_depth": 4.0,
            "scene_transparency_enabled": true,
            "scene_alpha": 0.25
        }
    }"#;

    let loaded: StructureDesignerPreferences = serde_json::from_str(pre_label_json)
        .expect("A settings file predating label_scale must still load");

    // The new field defaults...
    assert_eq!(
        loaded
            .atomic_structure_visualization_preferences
            .label_scale,
        0.7
    );
    // ...and its absence does not disturb its neighbours.
    assert_eq!(
        loaded
            .atomic_structure_visualization_preferences
            .scene_alpha,
        0.25
    );
    assert_eq!(
        loaded
            .atomic_structure_visualization_preferences
            .visualization,
        AtomicStructureVisualization::SpaceFilling
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
    assert!(
        !prefs
            .atomic_structure_visualization_preferences
            .scene_transparency_enabled
    );
    assert_eq!(
        prefs.atomic_structure_visualization_preferences.scene_alpha,
        0.5
    );
    // Atom label em height, Å (`doc/design_atom_labels.md` §Label size).
    assert_eq!(
        prefs.atomic_structure_visualization_preferences.label_scale,
        0.7
    );

    // Background defaults
    assert_eq!(
        prefs.background_preferences.background_color,
        PrefColor { x: 0, y: 0, z: 0 }
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

    // Simulation defaults
    assert!(prefs.simulation_preferences.use_vdw_cutoff);
    assert_eq!(
        prefs
            .simulation_preferences
            .continuous_minimization_steps_per_frame,
        4
    );
    assert_eq!(
        prefs
            .simulation_preferences
            .continuous_minimization_settle_steps,
        50
    );
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
            show_geometry_shell_for_atomic: false,
            wireframe_active_color: PrefColor {
                x: 10,
                y: 20,
                z: 30,
            },
            wireframe_inactive_color: PrefColor {
                x: 40,
                y: 50,
                z: 60,
            },
            hide_coplanar_wireframe_edges: false,
        },
        node_display_preferences: NodeDisplayPreferences {
            display_policy: NodeDisplayPolicy::PreferFrontier,
        },
        atomic_structure_visualization_preferences: AtomicStructureVisualizationPreferences {
            visualization: AtomicStructureVisualization::SpaceFilling,
            rendering_method: AtomicRenderingMethod::TriangleMesh,
            ball_and_stick_cull_depth: Some(10.0),
            space_filling_cull_depth: None,
            scene_transparency_enabled: true,
            scene_alpha: 0.35,
            label_scale: 1.25,
        },
        background_preferences: BackgroundPreferences {
            background_color: PrefColor {
                x: 255,
                y: 128,
                z: 64,
            },
            show_axes: false,
            show_grid: false,
            grid_size: 100,
            grid_color: PrefColor {
                x: 50,
                y: 50,
                z: 50,
            },
            grid_strong_color: PrefColor {
                x: 100,
                y: 100,
                z: 100,
            },
            show_lattice_axes: false,
            show_lattice_grid: true,
            lattice_grid_color: PrefColor {
                x: 30,
                y: 60,
                z: 60,
            },
            lattice_grid_strong_color: PrefColor {
                x: 80,
                y: 120,
                z: 120,
            },
            drawing_plane_grid_color: PrefColor {
                x: 50,
                y: 50,
                z: 80,
            },
            drawing_plane_grid_strong_color: PrefColor {
                x: 90,
                y: 90,
                z: 130,
            },
            unit_cell_wireframe_color: PrefColor {
                x: 0,
                y: 200,
                z: 200,
            },
        },
        layout_preferences: LayoutPreferences {
            layout_algorithm: LayoutAlgorithmPreference::TopologicalGrid,
            auto_layout_after_edit: false,
        },
        simulation_preferences: SimulationPreferences {
            use_vdw_cutoff: true,
            continuous_minimization_steps_per_frame: 8,
            continuous_minimization_settle_steps: 100,
            continuous_minimization_max_displacement: 0.05,
        },
        memory_preferences: MemoryPreferences {
            csg_mesh_cache_mb: 128,
            csg_sketch_cache_mb: 32,
            invisible_node_cache_mb: 512,
            eval_memo_cache_mb: 2048,
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
        loaded
            .geometry_visualization_preferences
            .wireframe_active_color,
        PrefColor {
            x: 10,
            y: 20,
            z: 30,
        }
    );
    assert_eq!(
        loaded
            .geometry_visualization_preferences
            .wireframe_inactive_color,
        PrefColor {
            x: 40,
            y: 50,
            z: 60,
        }
    );
    assert!(
        !loaded
            .geometry_visualization_preferences
            .hide_coplanar_wireframe_edges
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
    assert!(
        loaded
            .atomic_structure_visualization_preferences
            .scene_transparency_enabled
    );
    assert_eq!(
        loaded
            .atomic_structure_visualization_preferences
            .scene_alpha,
        0.35
    );
    assert_eq!(
        loaded
            .atomic_structure_visualization_preferences
            .label_scale,
        1.25
    );

    assert_eq!(
        loaded.background_preferences.background_color,
        PrefColor {
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
    assert_eq!(
        loaded
            .simulation_preferences
            .continuous_minimization_steps_per_frame,
        8
    );
    assert_eq!(
        loaded
            .simulation_preferences
            .continuous_minimization_settle_steps,
        100
    );
}

/// Test backward compatibility: SimulationPreferences JSON without continuous minimization
/// fields should deserialize with correct defaults.
#[test]
fn test_simulation_preferences_backward_compatibility() {
    // JSON from before continuous minimization was added
    let old_json = r#"{
        "simulation_preferences": {
            "use_vdw_cutoff": true
        }
    }"#;

    let loaded: StructureDesignerPreferences =
        serde_json::from_str(old_json).expect("Failed to deserialize old preferences");

    assert!(loaded.simulation_preferences.use_vdw_cutoff);
    assert_eq!(
        loaded
            .simulation_preferences
            .continuous_minimization_steps_per_frame,
        4
    );
    assert_eq!(
        loaded
            .simulation_preferences
            .continuous_minimization_settle_steps,
        50
    );
}

/// Test roundtrip of continuous minimization fields with non-default values.
#[test]
fn test_continuous_minimization_preferences_roundtrip() {
    let prefs = SimulationPreferences {
        use_vdw_cutoff: false,
        continuous_minimization_steps_per_frame: 10,
        continuous_minimization_settle_steps: 200,
        continuous_minimization_max_displacement: 0.05,
    };

    let json = serde_json::to_string(&prefs).expect("Failed to serialize");
    let loaded: SimulationPreferences = serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(loaded, prefs);
}

// ---------------------------------------------------------------------------
// Memory preferences (`doc/design_eval_memoization.md` D11)
// ---------------------------------------------------------------------------

/// The tolerant-reader contract, which is the one way a preferences change can
/// break existing users: a file written before this phase has no
/// `memory_preferences` section at all and must load with the documented
/// defaults rather than zeroes.
#[test]
fn test_preferences_without_memory_section_defaults_it() {
    let pre_memory_json = r#"{
        "layout_preferences": {
            "layout_algorithm": "Sugiyama",
            "auto_layout_after_edit": true
        }
    }"#;

    let loaded: StructureDesignerPreferences = serde_json::from_str(pre_memory_json)
        .expect("A settings file predating memory_preferences must still load");

    assert_eq!(loaded.memory_preferences.csg_mesh_cache_mb, 200);
    assert_eq!(loaded.memory_preferences.csg_sketch_cache_mb, 56);
    assert_eq!(loaded.memory_preferences.invisible_node_cache_mb, 256);
}

/// A partially-written section must fill only its missing fields — the
/// per-field `#[serde(default)]` contract, not a whole-section fallback.
#[test]
fn test_partial_memory_section_defaults_only_the_missing_fields() {
    let partial_json = r#"{
        "memory_preferences": {
            "csg_mesh_cache_mb": 64
        }
    }"#;

    let loaded: StructureDesignerPreferences =
        serde_json::from_str(partial_json).expect("A partial memory section must load");

    assert_eq!(loaded.memory_preferences.csg_mesh_cache_mb, 64);
    assert_eq!(loaded.memory_preferences.csg_sketch_cache_mb, 56);
    assert_eq!(loaded.memory_preferences.invisible_node_cache_mb, 256);
}

/// Budgets are expressed in megabytes because bytes are the wrong unit for a
/// person; the conversion is the only arithmetic in the path.
#[test]
fn test_megabyte_budgets_convert_to_bytes() {
    assert_eq!(MemoryPreferences::mb_to_bytes(0), 0);
    assert_eq!(MemoryPreferences::mb_to_bytes(1), 1024 * 1024);
    assert_eq!(MemoryPreferences::mb_to_bytes(1024), 1024 * 1024 * 1024);
}

/// Applying a change must not need a restart: `set_preferences` pushes the new
/// budgets straight into the live caches.
#[test]
fn test_memory_preferences_apply_live_without_a_restart() {
    use atomcad_structure_designer::structure_designer::StructureDesigner;

    let mut sd = StructureDesigner::new();

    // Pin the starting point rather than trusting whatever this machine's
    // persisted preferences say (`StructureDesigner::new()` loads the real
    // user file).
    let mut prefs = sd.preferences.clone();
    prefs.memory_preferences = MemoryPreferences {
        csg_mesh_cache_mb: 300,
        csg_sketch_cache_mb: 70,
        invisible_node_cache_mb: 400,
        eval_memo_cache_mb: 2048,
    };
    sd.set_preferences(prefs.clone());

    let stats = sd.network_evaluator.get_csg_cache_stats();
    assert_eq!(stats.mesh_capacity_bytes, 300 * 1024 * 1024);
    assert_eq!(stats.sketch_capacity_bytes, 70 * 1024 * 1024);
    assert_eq!(
        sd.last_generated_structure_designer_scene
            .invisible_node_cache_capacity_bytes(),
        400 * 1024 * 1024
    );

    // ...and lowering them takes effect at once too, which is the direction
    // that matters: `MemoryBoundedLruCache::resize` evicts down to the new
    // limit rather than waiting for the next insert.
    prefs.memory_preferences = MemoryPreferences {
        csg_mesh_cache_mb: 8,
        csg_sketch_cache_mb: 4,
        invisible_node_cache_mb: 16,
        eval_memo_cache_mb: 32,
    };
    sd.set_preferences(prefs);

    let stats = sd.network_evaluator.get_csg_cache_stats();
    assert_eq!(stats.mesh_capacity_bytes, 8 * 1024 * 1024);
    assert_eq!(stats.sketch_capacity_bytes, 4 * 1024 * 1024);
    assert_eq!(
        sd.last_generated_structure_designer_scene
            .invisible_node_cache_capacity_bytes(),
        16 * 1024 * 1024
    );
}

/// The scene — and with it the invisible-node cache — is **rebuilt on every
/// full refresh**, so a plain `StructureDesignerScene::new()` at any of those
/// sites would quietly reset the budget to the built-in default. Without this
/// test the setting appears to work (the dialog shows it, `set_preferences`
/// applies it) and then silently stops working at the next refresh.
#[test]
fn test_invisible_node_cache_budget_survives_a_full_refresh() {
    use atomcad_structure_designer::structure_designer::StructureDesigner;

    let mut sd = StructureDesigner::new();
    sd.add_node_network("Main");
    sd.set_active_node_network_name(Some("Main".to_string()));

    let mut prefs = sd.preferences.clone();
    prefs.memory_preferences.invisible_node_cache_mb = 33;
    sd.set_preferences(prefs);

    sd.mark_full_refresh();
    let changes = sd.get_pending_changes();
    sd.refresh(&changes);

    assert_eq!(
        sd.last_generated_structure_designer_scene
            .invisible_node_cache_capacity_bytes(),
        33 * 1024 * 1024,
        "a full refresh must not reset the configured cache budget"
    );
}

/// `preferences.json` is a plain text file a user can edit, so the apply path
/// clamps rather than trusting it. A `0` budget would build a cache that evicts
/// every entry the moment after it inserts it — still correct, and
/// indistinguishable from a performance bug.
#[test]
fn test_out_of_range_budgets_are_clamped_on_the_apply_path() {
    use atomcad_structure_designer::structure_designer::StructureDesigner;

    assert_eq!(MemoryPreferences::clamped_bytes(0), 1024 * 1024);
    assert_eq!(
        MemoryPreferences::clamped_bytes(u32::MAX),
        16 * 1024 * 1024 * 1024
    );
    // In-range values pass through untouched.
    assert_eq!(
        MemoryPreferences::clamped_bytes(200),
        MemoryPreferences::mb_to_bytes(200)
    );

    let mut sd = StructureDesigner::new();
    let mut prefs = sd.preferences.clone();
    prefs.memory_preferences = MemoryPreferences {
        csg_mesh_cache_mb: 0,
        csg_sketch_cache_mb: 0,
        invisible_node_cache_mb: 0,
        eval_memo_cache_mb: 0,
    };
    sd.set_preferences(prefs);

    let stats = sd.network_evaluator.get_csg_cache_stats();
    assert_eq!(stats.mesh_capacity_bytes, 1024 * 1024);
    assert_eq!(stats.sketch_capacity_bytes, 1024 * 1024);
    assert_eq!(
        sd.last_generated_structure_designer_scene
            .invisible_node_cache_capacity_bytes(),
        1024 * 1024
    );
}
