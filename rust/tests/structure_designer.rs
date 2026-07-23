#[path = "structure_designer/structure_designer_test.rs"]
mod structure_designer_test;

#[path = "structure_designer/data_type_test.rs"]
mod data_type_test;

#[path = "structure_designer/optional_type_phase1_test.rs"]
mod optional_type_phase1_test;

#[path = "structure_designer/optional_type_phase2_test.rs"]
mod optional_type_phase2_test;

#[path = "structure_designer/node_dependency_analysis_tests.rs"]
mod node_dependency_analysis_tests;

#[path = "structure_designer/navigation_history_test.rs"]
mod navigation_history_test;

#[path = "structure_designer/kernel_test.rs"]
mod kernel_test;

#[path = "structure_designer/nodes/node_snapshots_test.rs"]
mod node_snapshots_test;

#[path = "structure_designer/structure_node_test.rs"]
mod structure_node_test;

#[path = "structure_designer/network_validator_test.rs"]
mod network_validator_test;

#[path = "structure_designer/node_network_test.rs"]
mod node_network_test;

#[path = "structure_designer/network_evaluator_test.rs"]
mod network_evaluator_test;

#[path = "structure_designer/node_type_registry_test.rs"]
mod node_type_registry_test;

#[path = "structure_designer/comment_node_test.rs"]
mod comment_node_test;

#[path = "structure_designer/text_format_test.rs"]
mod text_format_test;

#[path = "structure_designer/text_properties_test.rs"]
mod text_properties_test;

#[path = "structure_designer/drawing_plane_explicit_axes_test.rs"]
mod drawing_plane_explicit_axes_test;

#[path = "structure_designer/text_format_snapshot_test.rs"]
mod text_format_snapshot_test;

#[path = "structure_designer/node_layout_test.rs"]
mod node_layout_test;

#[path = "structure_designer/evaluate_node_test.rs"]
mod evaluate_node_test;

#[path = "structure_designer/evaluate_arg_desync_regression_test.rs"]
mod evaluate_arg_desync_regression_test;

#[path = "structure_designer/serialization_test.rs"]
mod serialization_test;

#[path = "structure_designer/rename_wire_loss_regression_test.rs"]
mod rename_wire_loss_regression_test;

#[path = "structure_designer/record_field_rename_wire_loss_test.rs"]
mod record_field_rename_wire_loss_test;

#[path = "structure_designer/record_destructure_output_identity_test.rs"]
mod record_destructure_output_identity_test;

#[path = "structure_designer/record_field_identity_undo_test.rs"]
mod record_field_identity_undo_test;

#[path = "structure_designer/layout_topological_grid_test.rs"]
mod layout_topological_grid_test;

#[path = "structure_designer/layout_sugiyama_test.rs"]
mod layout_sugiyama_test;

#[path = "structure_designer/layout_after_edit_test.rs"]
mod layout_after_edit_test;

#[path = "structure_designer/parameter_wire_preservation_test.rs"]
mod parameter_wire_preservation_test;

#[path = "structure_designer/parameter_wire_stability_regression_test.rs"]
mod parameter_wire_stability_regression_test;

#[path = "structure_designer/preferences_test.rs"]
mod preferences_test;

#[path = "structure_designer/selection_factoring_test.rs"]
mod selection_factoring_test;

#[path = "structure_designer/node_inlining_test.rs"]
mod node_inlining_test;

#[path = "structure_designer/promote_to_parameter_test.rs"]
mod promote_to_parameter_test;

#[path = "structure_designer/copy_paste_test.rs"]
mod copy_paste_test;

#[path = "structure_designer/atom_edit_text_format_test.rs"]
mod atom_edit_text_format_test;

#[path = "structure_designer/atom_edit_mutations_test.rs"]
mod atom_edit_mutations_test;

#[path = "structure_designer/atom_edit_measurement_test.rs"]
mod atom_edit_measurement_test;

#[path = "structure_designer/atom_edit_bond_order_test.rs"]
mod atom_edit_bond_order_test;

#[path = "structure_designer/atom_edit_selection_order_test.rs"]
mod atom_edit_selection_order_test;

#[path = "structure_designer/modify_measurement_test.rs"]
mod modify_measurement_test;

#[path = "structure_designer/atom_edit_move_in_diff_test.rs"]
mod atom_edit_move_in_diff_test;

#[path = "structure_designer/apply_diff_node_test.rs"]
mod apply_diff_node_test;

// Shared support for the per-node diff-output roundtrip (issue #295 Phase 2+).
// `structure_equivalence` provides the `≡` used by `diff_test_support`.
#[path = "test_support/structure_equivalence.rs"]
mod structure_equivalence;

#[path = "structure_designer/diff_test_support.rs"]
mod diff_test_support;

#[path = "structure_designer/relax_diff_output_test.rs"]
mod relax_diff_output_test;

#[path = "structure_designer/movement_diff_output_test.rs"]
mod movement_diff_output_test;

#[path = "structure_designer/atom_op_diff_output_test.rs"]
mod atom_op_diff_output_test;

#[path = "structure_designer/atom_composediff_test.rs"]
mod atom_composediff_test;

#[path = "structure_designer/atom_lattice_transform_test.rs"]
mod atom_lattice_transform_test;

#[path = "structure_designer/passivate_node_test.rs"]
mod passivate_node_test;

#[path = "structure_designer/remove_hydrogen_node_test.rs"]
mod remove_hydrogen_node_test;

#[path = "structure_designer/atom_edit_hydrogen_roundtrip_test.rs"]
mod atom_edit_hydrogen_roundtrip_test;

#[path = "structure_designer/raytrace_per_node_test.rs"]
mod raytrace_per_node_test;

#[path = "structure_designer/atom_edit_unchanged_test.rs"]
mod atom_edit_unchanged_test;

#[path = "structure_designer/atom_edit_add_atom_marker_test.rs"]
mod atom_edit_add_atom_marker_test;

#[path = "structure_designer/atom_edit_subnetwork_caching_test.rs"]
mod atom_edit_subnetwork_caching_test;

#[path = "structure_designer/undo_test.rs"]
mod undo_test;

#[path = "structure_designer/reflow_test.rs"]
mod reflow_test;

#[path = "structure_designer/atom_edit_tags_test.rs"]
mod atom_edit_tags_test;

#[path = "structure_designer/atom_edit_undo_test.rs"]
mod atom_edit_undo_test;

#[path = "structure_designer/atom_edit_guideline_test.rs"]
mod atom_edit_guideline_test;

#[path = "structure_designer/atom_edit_guideline_tool_test.rs"]
mod atom_edit_guideline_tool_test;

#[path = "structure_designer/atom_edit_guideline_render_test.rs"]
mod atom_edit_guideline_render_test;

#[path = "structure_designer/atom_edit_guideline_drag_test.rs"]
mod atom_edit_guideline_drag_test;

#[path = "structure_designer/continuous_minimization_test.rs"]
mod continuous_minimization_test;

#[path = "structure_designer/cli_access_rules_test.rs"]
mod cli_access_rules_test;

#[path = "structure_designer/relax_node_atom_limit_test.rs"]
mod relax_node_atom_limit_test;

#[path = "structure_designer/multi_output_unit_test.rs"]
mod multi_output_unit_test;

#[path = "structure_designer/sequence_node_test.rs"]
mod sequence_node_test;

#[path = "structure_designer/array_node_test.rs"]
mod array_node_test;

#[path = "structure_designer/array_at_test.rs"]
mod array_at_test;

#[path = "structure_designer/if_test.rs"]
mod if_test;

#[path = "structure_designer/array_len_test.rs"]
mod array_len_test;

#[path = "structure_designer/array_concat_test.rs"]
mod array_concat_test;

#[path = "structure_designer/array_append_test.rs"]
mod array_append_test;

#[path = "structure_designer/collect_test.rs"]
mod collect_test;

#[path = "structure_designer/filter_test.rs"]
mod filter_test;

#[path = "structure_designer/fold_test.rs"]
mod fold_test;

#[path = "structure_designer/motif_edit_test.rs"]
mod motif_edit_test;

#[path = "structure_designer/unit_cell_wireframe_test.rs"]
mod unit_cell_wireframe_test;

#[path = "structure_designer/import_cif_test.rs"]
mod import_cif_test;

#[path = "structure_designer/infer_bonds_test.rs"]
mod infer_bonds_test;

#[path = "structure_designer/atom_replace_test.rs"]
mod atom_replace_test;

#[path = "structure_designer/atom_replace_region_test.rs"]
mod atom_replace_region_test;

#[path = "structure_designer/region_atom_ops_test.rs"]
mod region_atom_ops_test;

#[path = "structure_designer/freeze_test.rs"]
mod freeze_test;

#[path = "structure_designer/xray_test.rs"]
mod xray_test;

#[path = "structure_designer/tag_test.rs"]
mod tag_test;

#[path = "structure_designer/apply_style_test.rs"]
mod apply_style_test;

#[path = "structure_designer/network_result_test.rs"]
mod network_result_test;

#[path = "structure_designer/crystal_molecule_split_validation_test.rs"]
mod crystal_molecule_split_validation_test;

#[path = "structure_designer/materialize_test.rs"]
mod materialize_test;

#[path = "structure_designer/materialize_regions_test.rs"]
mod materialize_regions_test;

#[path = "structure_designer/phase_transitions_test.rs"]
mod phase_transitions_test;

#[path = "structure_designer/alignment_test.rs"]
mod alignment_test;

#[path = "structure_designer/csg_structure_propagation_test.rs"]
mod csg_structure_propagation_test;

#[path = "structure_designer/cuboid_subdivision_test.rs"]
mod cuboid_subdivision_test;

#[path = "structure_designer/get_structure_test.rs"]
mod get_structure_test;

#[path = "structure_designer/with_structure_test.rs"]
mod with_structure_test;

#[path = "structure_designer/supercell_node_test.rs"]
mod supercell_node_test;

#[path = "structure_designer/matrix_types_test.rs"]
mod matrix_types_test;

#[path = "structure_designer/imat2_nodes_test.rs"]
mod imat2_nodes_test;

#[path = "structure_designer/imat2_types_test.rs"]
mod imat2_types_test;

#[path = "structure_designer/plane_tiling_vectors_test.rs"]
mod plane_tiling_vectors_test;

#[path = "structure_designer/imat3_nodes_test.rs"]
mod imat3_nodes_test;

#[path = "structure_designer/identifier_test.rs"]
mod identifier_test;

#[path = "structure_designer/relaxed_node_names_test.rs"]
mod relaxed_node_names_test;

#[path = "structure_designer/expr_array_literal_test.rs"]
mod expr_array_literal_test;

#[path = "structure_designer/expr_template_literal_test.rs"]
mod expr_template_literal_test;

#[path = "structure_designer/expr_array_index_test.rs"]
mod expr_array_index_test;

#[path = "structure_designer/record_types_phase1_test.rs"]
mod record_types_phase1_test;

#[path = "structure_designer/record_types_phase2_test.rs"]
mod record_types_phase2_test;

#[path = "structure_designer/record_types_phase3_test.rs"]
mod record_types_phase3_test;

#[path = "structure_designer/record_types_phase4_test.rs"]
mod record_types_phase4_test;

#[path = "structure_designer/record_types_phase8_test.rs"]
mod record_types_phase8_test;

#[path = "structure_designer/atom_replace_rules_phase_a_test.rs"]
mod atom_replace_rules_phase_a_test;

#[path = "structure_designer/atom_replace_rules_phase_b_test.rs"]
mod atom_replace_rules_phase_b_test;

#[path = "structure_designer/iterator_walker_test.rs"]
mod iterator_walker_test;

#[path = "structure_designer/function_value_test.rs"]
mod function_value_test;

#[path = "structure_designer/iter_type_test.rs"]
mod iter_type_test;

#[path = "structure_designer/drag_adapter_test.rs"]
mod drag_adapter_test;

#[path = "structure_designer/unit_type_test.rs"]
mod unit_type_test;

#[path = "structure_designer/execute_flag_test.rs"]
mod execute_flag_test;

#[path = "structure_designer/execute_node_test.rs"]
mod execute_node_test;

#[path = "structure_designer/print_node_test.rs"]
mod print_node_test;

#[path = "structure_designer/custom_node_property_panel_test.rs"]
mod custom_node_property_panel_test;

#[path = "structure_designer/record_construct_property_panel_test.rs"]
mod record_construct_property_panel_test;

#[path = "structure_designer/switch_test.rs"]
mod switch_test;

#[path = "structure_designer/zip_with_test.rs"]
mod zip_with_test;

#[path = "structure_designer/zones_test.rs"]
mod zones_test;

#[path = "structure_designer/parameter_in_zone_body_test.rs"]
mod parameter_in_zone_body_test;

#[path = "structure_designer/closures_test.rs"]
mod closures_test;

#[path = "structure_designer/closure_network_conversion_test.rs"]
mod closure_network_conversion_test;

#[path = "structure_designer/scope_dispatch_test.rs"]
mod scope_dispatch_test;

#[path = "structure_designer/hof_collapse_test.rs"]
mod hof_collapse_test;

#[path = "structure_designer/function_pin_test.rs"]
mod function_pin_test;

#[path = "structure_designer/zones_migration_test.rs"]
mod zones_migration_test;

#[path = "structure_designer/debug_load_test.rs"]
mod debug_load_test;

#[path = "structure_designer/currying_test.rs"]
mod currying_test;

#[path = "structure_designer/function_pin_unification_test.rs"]
mod function_pin_unification_test;

#[path = "structure_designer/apply_function_pin_iter_test.rs"]
mod apply_function_pin_iter_test;

#[path = "structure_designer/abstract_output_type_test.rs"]
mod abstract_output_type_test;

#[path = "structure_designer/hierarchical_records_test.rs"]
mod hierarchical_records_test;

#[path = "structure_designer/empty_folders_test.rs"]
mod empty_folders_test;

#[path = "structure_designer/patch_record_test.rs"]
mod patch_record_test;

#[path = "structure_designer/patch_build_test.rs"]
mod patch_build_test;

#[path = "structure_designer/patch_latticefill_test.rs"]
mod patch_latticefill_test;

#[path = "structure_designer/invariants_test.rs"]
mod invariants_test;

#[path = "structure_designer/field_editor_hints_test.rs"]
mod field_editor_hints_test;

#[path = "structure_designer/record_field_identity_test.rs"]
mod record_field_identity_test;

#[path = "structure_designer/unpack_nodes_test.rs"]
mod unpack_nodes_test;

#[path = "structure_designer/extrude_structure_test.rs"]
mod extrude_structure_test;

#[path = "structure_designer/free_geometry_nodes_test.rs"]
mod free_geometry_nodes_test;

#[path = "structure_designer/lattice_covariant_primitives_test.rs"]
mod lattice_covariant_primitives_test;

#[path = "structure_designer/free_rot_test.rs"]
mod free_rot_test;

#[path = "structure_designer/camera_settings_serialization_test.rs"]
mod camera_settings_serialization_test;

#[path = "structure_designer/canvas_viewport_serialization_test.rs"]
mod canvas_viewport_serialization_test;

#[path = "structure_designer/structure_move_gadget_test.rs"]
mod structure_move_gadget_test;

#[path = "structure_designer/structure_move_serde_test.rs"]
mod structure_move_serde_test;

#[path = "structure_designer/refresh_pipeline_test.rs"]
mod refresh_pipeline_test;

#[path = "structure_designer/scene_noderef_keying_test.rs"]
mod scene_noderef_keying_test;

#[path = "structure_designer/zero_ary_closure_display_test.rs"]
mod zero_ary_closure_display_test;

#[path = "structure_designer/zero_ary_closure_display_undo_test.rs"]
mod zero_ary_closure_display_undo_test;

#[path = "structure_designer/find_usages_test.rs"]
mod find_usages_test;
