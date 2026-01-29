import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Keys for preferences window widgets, used for integration testing.
class PreferencesKeys {
  static const Key preferencesDialog = Key('preferences_dialog');
  static const Key closeButton = Key('preferences_close_button');

  // Geometry visualization
  static const Key visualizationMethodDropdown =
      Key('pref_visualization_method_dropdown');
  static const Key samplesPerUnitCellInput =
      Key('pref_samples_per_unit_cell_input');
  static const Key sharpnessThresholdInput =
      Key('pref_sharpness_threshold_input');
  static const Key meshRenderingDropdown = Key('pref_mesh_rendering_dropdown');

  // Atomic structure visualization
  static const Key atomicVisualizationDropdown =
      Key('pref_atomic_visualization_dropdown');
  static const Key atomicRenderingMethodDropdown =
      Key('pref_atomic_rendering_method_dropdown');
  static const Key ballAndStickCullDepthInput =
      Key('pref_ball_and_stick_cull_depth_input');
  static const Key spaceFillingCullDepthInput =
      Key('pref_space_filling_cull_depth_input');

  // Other settings
  static const Key displayCameraPivotCheckbox =
      Key('pref_display_camera_pivot_checkbox');

  // Background settings
  static const Key backgroundColorInput = Key('pref_background_color_input');
  static const Key showGridCheckbox = Key('pref_show_grid_checkbox');
  static const Key gridSizeInput = Key('pref_grid_size_input');
  static const Key gridColorInput = Key('pref_grid_color_input');
  static const Key gridStrongColorInput = Key('pref_grid_strong_color_input');
  static const Key showLatticeAxesCheckbox =
      Key('pref_show_lattice_axes_checkbox');
  static const Key showLatticeGridCheckbox =
      Key('pref_show_lattice_grid_checkbox');
  static const Key latticeGridColorInput = Key('pref_lattice_grid_color_input');
  static const Key latticeGridStrongColorInput =
      Key('pref_lattice_grid_strong_color_input');
  static const Key drawingPlaneGridColorInput =
      Key('pref_drawing_plane_grid_color_input');
  static const Key drawingPlaneGridStrongColorInput =
      Key('pref_drawing_plane_grid_strong_color_input');
}

/// A modal preferences window for the structure designer.
class PreferencesWindow extends StatefulWidget {
  final StructureDesignerModel model;

  const PreferencesWindow({
    super.key,
    required this.model,
  });

  @override
  State<PreferencesWindow> createState() => _PreferencesWindowState();
}

class _PreferencesWindowState extends State<PreferencesWindow> {
  // Local copy of preferences that we'll modify
  late StructureDesignerPreferences _preferences;

  @override
  void initState() {
    super.initState();
    _initPreferences();
  }

  void _initPreferences() {
    // Make a copy of the current preferences
    final currentPreferences = widget.model.preferences;
    if (currentPreferences != null) {
      // Clone the existing preferences
      _preferences = currentPreferences.cloneSelf();
    } else {
      // If no preferences exist yet, create default ones
      _preferences = StructureDesignerPreferences();
    }
  }

  // Helper method to update visualization method and wireframe settings together
  void _updateVisualizationMethod(int value) {
    setState(() {
      switch (value) {
        case 0: // Surface splatting
          _preferences.geometryVisualizationPreferences.geometryVisualization =
              GeometryVisualization.surfaceSplatting;
          _preferences.geometryVisualizationPreferences.wireframeGeometry =
              false;
          break;
        case 1: // Solid
          _preferences.geometryVisualizationPreferences.geometryVisualization =
              GeometryVisualization.explicitMesh;
          _preferences.geometryVisualizationPreferences.wireframeGeometry =
              false;
          break;
        case 2: // Wireframe
          _preferences.geometryVisualizationPreferences.geometryVisualization =
              GeometryVisualization.explicitMesh;
          _preferences.geometryVisualizationPreferences.wireframeGeometry =
              true;
          break;
      }
    });
    _applyPreferences();
  }

  // Helper to get the selected atomic structure visualization index
  int _getAtomicVisualizationIndex() {
    return _preferences.atomicStructureVisualizationPreferences.visualization ==
            AtomicStructureVisualization.ballAndStick
        ? 0
        : 1;
  }

  // Helper method to update atomic structure visualization
  void _updateAtomicVisualization(int value) {
    setState(() {
      _preferences.atomicStructureVisualizationPreferences.visualization =
          value == 0
              ? AtomicStructureVisualization.ballAndStick
              : AtomicStructureVisualization.spaceFilling;
    });
    _applyPreferences();
  }

  // Helper to get the selected atomic rendering method index
  int _getAtomicRenderingMethodIndex() {
    return _preferences
                .atomicStructureVisualizationPreferences.renderingMethod ==
            AtomicRenderingMethod.triangleMesh
        ? 0
        : 1;
  }

  // Helper method to update atomic rendering method
  void _updateAtomicRenderingMethod(int value) {
    setState(() {
      _preferences.atomicStructureVisualizationPreferences.renderingMethod =
          value == 0
              ? AtomicRenderingMethod.triangleMesh
              : AtomicRenderingMethod.impostors;
    });
    _applyPreferences();
  }

  // Helper to get the selected visualization method index
  int _getVisualizationMethodIndex() {
    if (_preferences.geometryVisualizationPreferences.geometryVisualization ==
        GeometryVisualization.surfaceSplatting) {
      return 0;
    } else if (_preferences
            .geometryVisualizationPreferences.geometryVisualization ==
        GeometryVisualization.explicitMesh) {
      return _preferences.geometryVisualizationPreferences.wireframeGeometry
          ? 2
          : 1;
    }
    return 0; // Default
  }

  // Apply preferences immediately
  void _applyPreferences() {
    // Clone our local preferences before sending to the model
    final updatedPreferences = _preferences.cloneSelf();
    widget.model.setPreferences(updatedPreferences);
  }

  @override
  Widget build(BuildContext context) {
    return DraggableDialog(
      key: PreferencesKeys.preferencesDialog,
      width: 400,
      height: 500,
      child: Padding(
        padding: const EdgeInsets.all(AppSpacing.medium),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            // Title with close button
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                MouseRegion(
                  cursor: SystemMouseCursors.move,
                  child: Text(
                    'Preferences',
                    style: TextStyle(
                      fontSize: 18,
                      fontWeight: FontWeight.bold,
                      color: AppColors.textPrimary,
                    ),
                  ),
                ),
                IconButton(
                  key: PreferencesKeys.closeButton,
                  icon: const Icon(Icons.close),
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(),
                  onPressed: () => Navigator.of(context).pop(),
                ),
              ],
            ),
            const SizedBox(height: AppSpacing.medium),

            // Scrollable content area
            Expanded(
              child: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    // Geometry Visualization Section
                    Container(
                      width: double.infinity,
                      padding: const EdgeInsets.all(AppSpacing.medium),
                      decoration: BoxDecoration(
                        color: Colors.grey[200],
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'Geometry Visualization',
                            style: TextStyle(
                              fontWeight: FontWeight.bold,
                              color: AppColors.textPrimary,
                            ),
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Visualization method dropdown
                          Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              const Text('Visualization method'),
                              const SizedBox(height: 4),
                              DropdownButtonFormField<int>(
                                key: PreferencesKeys.visualizationMethodDropdown,
                                decoration: const InputDecoration(
                                  border: OutlineInputBorder(),
                                  contentPadding:
                                      AppSpacing.fieldContentPadding,
                                ),
                                value: _getVisualizationMethodIndex(),
                                items: const [
                                  DropdownMenuItem(
                                    value: 0,
                                    child: Text('Surface Splatting'),
                                  ),
                                  DropdownMenuItem(
                                    value: 1,
                                    child: Text('Solid'),
                                  ),
                                  DropdownMenuItem(
                                    value: 2,
                                    child: Text('Wireframe'),
                                  ),
                                ],
                                onChanged: (value) {
                                  if (value != null) {
                                    _updateVisualizationMethod(value);
                                  }
                                },
                              ),
                            ],
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Samples per unit cell
                          IntInput(
                            key: PreferencesKeys.samplesPerUnitCellInput,
                            label: 'Samples per unit cell',
                            value: _preferences.geometryVisualizationPreferences
                                .samplesPerUnitCell,
                            onChanged: (value) {
                              setState(() {
                                _preferences.geometryVisualizationPreferences
                                    .samplesPerUnitCell = value;
                              });
                              _applyPreferences();
                            },
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Sharpness threshold
                          FloatInput(
                            key: PreferencesKeys.sharpnessThresholdInput,
                            label: 'Sharpness threshold (degree)',
                            value: _preferences.geometryVisualizationPreferences
                                .sharpnessAngleThresholdDegree,
                            onChanged: (value) {
                              setState(() {
                                _preferences.geometryVisualizationPreferences
                                    .sharpnessAngleThresholdDegree = value;
                              });
                              _applyPreferences();
                            },
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Mesh rendering
                          Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              const Text('Mesh rendering'),
                              const SizedBox(height: 4),
                              DropdownButtonFormField<MeshSmoothing>(
                                key: PreferencesKeys.meshRenderingDropdown,
                                decoration: const InputDecoration(
                                  border: OutlineInputBorder(),
                                  contentPadding:
                                      AppSpacing.fieldContentPadding,
                                ),
                                value: _preferences
                                    .geometryVisualizationPreferences
                                    .meshSmoothing,
                                items: const [
                                  DropdownMenuItem(
                                    value: MeshSmoothing.smooth,
                                    child: Text('Smooth'),
                                  ),
                                  DropdownMenuItem(
                                    value: MeshSmoothing.sharp,
                                    child: Text('Sharp'),
                                  ),
                                  DropdownMenuItem(
                                    value: MeshSmoothing.smoothingGroupBased,
                                    child: Text('Smart (detect sharp edges)'),
                                  ),
                                ],
                                onChanged: (value) {
                                  if (value != null) {
                                    setState(() {
                                      _preferences
                                          .geometryVisualizationPreferences
                                          .meshSmoothing = value;
                                    });
                                    _applyPreferences();
                                  }
                                },
                              ),
                            ],
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(height: AppSpacing.medium),

                    // Atomic Structure Visualization Section
                    Container(
                      width: double.infinity,
                      padding: const EdgeInsets.all(AppSpacing.medium),
                      decoration: BoxDecoration(
                        color: Colors.grey[200],
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'Atomic Structure Visualization',
                            style: TextStyle(
                              fontWeight: FontWeight.bold,
                              color: AppColors.textPrimary,
                            ),
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Atomic structure visualization method dropdown
                          Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              const Text('Visualization method'),
                              const SizedBox(height: 4),
                              DropdownButtonFormField<int>(
                                key: PreferencesKeys.atomicVisualizationDropdown,
                                decoration: const InputDecoration(
                                  border: OutlineInputBorder(),
                                  contentPadding:
                                      AppSpacing.fieldContentPadding,
                                ),
                                value: _getAtomicVisualizationIndex(),
                                items: const [
                                  DropdownMenuItem(
                                    value: 0,
                                    child: Text('Ball and Stick'),
                                  ),
                                  DropdownMenuItem(
                                    value: 1,
                                    child: Text('Space Filling'),
                                  ),
                                ],
                                onChanged: (value) {
                                  if (value != null) {
                                    _updateAtomicVisualization(value);
                                  }
                                },
                              ),
                            ],
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Rendering method selection
                          Row(
                            children: [
                              const Text('Rendering Method:'),
                              const SizedBox(width: AppSpacing.medium),
                              Expanded(
                                child: DropdownButton<int>(
                                  key: PreferencesKeys
                                      .atomicRenderingMethodDropdown,
                                  value: _getAtomicRenderingMethodIndex(),
                                  items: const [
                                    DropdownMenuItem(
                                      value: 0,
                                      child: Text('Triangle Mesh'),
                                    ),
                                    DropdownMenuItem(
                                      value: 1,
                                      child: Text('Impostors'),
                                    ),
                                  ],
                                  onChanged: (value) {
                                    if (value != null) {
                                      _updateAtomicRenderingMethod(value);
                                    }
                                  },
                                ),
                              ),
                            ],
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Ball and stick specific settings
                          if (_preferences
                                  .atomicStructureVisualizationPreferences
                                  .visualization ==
                              AtomicStructureVisualization.ballAndStick) ...[
                            FloatInput(
                              key: PreferencesKeys.ballAndStickCullDepthInput,
                              label: 'Ball & stick depth culling threshold (Å)',
                              value: _preferences
                                      .atomicStructureVisualizationPreferences
                                      .ballAndStickCullDepth ??
                                  0.0,
                              onChanged: (value) {
                                setState(() {
                                  _preferences
                                      .atomicStructureVisualizationPreferences
                                      .ballAndStickCullDepth = value >
                                          0.0
                                      ? value
                                      : null;
                                });
                                _applyPreferences();
                              },
                            ),
                            const SizedBox(height: 4),
                            Text(
                              'Atoms deeper than this threshold will not be rendered (set to 0 to disable)',
                              style: TextStyle(
                                fontSize: 12,
                                color: Colors.grey[600],
                              ),
                            ),
                          ],

                          // Space filling specific settings
                          if (_preferences
                                  .atomicStructureVisualizationPreferences
                                  .visualization ==
                              AtomicStructureVisualization.spaceFilling) ...[
                            FloatInput(
                              key: PreferencesKeys.spaceFillingCullDepthInput,
                              label:
                                  'Space filling depth culling threshold (Å)',
                              value: _preferences
                                      .atomicStructureVisualizationPreferences
                                      .spaceFillingCullDepth ??
                                  0.0,
                              onChanged: (value) {
                                setState(() {
                                  _preferences
                                      .atomicStructureVisualizationPreferences
                                      .spaceFillingCullDepth = value >
                                          0.0
                                      ? value
                                      : null;
                                });
                                _applyPreferences();
                              },
                            ),
                            const SizedBox(height: 4),
                            Text(
                              'Atoms deeper than this threshold will be culled to improve performance (set to 0 to disable)',
                              style: TextStyle(
                                fontSize: 12,
                                color: Colors.grey[600],
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                    const SizedBox(height: AppSpacing.medium),

                    // Other Settings Section
                    Container(
                      width: double.infinity,
                      padding: const EdgeInsets.all(AppSpacing.medium),
                      decoration: BoxDecoration(
                        color: Colors.grey[200],
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'Other Settings',
                            style: TextStyle(
                              fontWeight: FontWeight.bold,
                              color: AppColors.textPrimary,
                            ),
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Camera target display checkbox
                          Row(
                            children: [
                              Checkbox(
                                key: PreferencesKeys.displayCameraPivotCheckbox,
                                value: _preferences
                                    .geometryVisualizationPreferences
                                    .displayCameraTarget,
                                onChanged: (value) {
                                  if (value != null) {
                                    setState(() {
                                      _preferences
                                          .geometryVisualizationPreferences
                                          .displayCameraTarget = value;
                                    });
                                    _applyPreferences();
                                  }
                                },
                              ),
                              const SizedBox(width: 8),
                              const Text('Display camera pivot point'),
                            ],
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(height: AppSpacing.medium),

                    // Background Preferences Section
                    Container(
                      width: double.infinity,
                      padding: const EdgeInsets.all(AppSpacing.medium),
                      decoration: BoxDecoration(
                        color: Colors.grey[200],
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'Background',
                            style: TextStyle(
                              fontWeight: FontWeight.bold,
                              color: AppColors.textPrimary,
                            ),
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Background color
                          IVec3Input(
                            key: PreferencesKeys.backgroundColorInput,
                            label: 'Background color (RGB)',
                            value: _preferences
                                .backgroundPreferences.backgroundColor,
                            onChanged: (value) {
                              setState(() {
                                _preferences.backgroundPreferences
                                    .backgroundColor = value;
                              });
                              _applyPreferences();
                            },
                            minimumValue: const APIIVec3(x: 0, y: 0, z: 0),
                            maximumValue:
                                const APIIVec3(x: 255, y: 255, z: 255),
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Show grid checkbox
                          Row(
                            children: [
                              Checkbox(
                                key: PreferencesKeys.showGridCheckbox,
                                value:
                                    _preferences.backgroundPreferences.showGrid,
                                onChanged: (value) {
                                  if (value != null) {
                                    setState(() {
                                      _preferences.backgroundPreferences
                                          .showGrid = value;
                                    });
                                    _applyPreferences();
                                  }
                                },
                              ),
                              const SizedBox(width: 8),
                              const Text('Show axes and grids'),
                            ],
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Grid size
                          IntInput(
                            key: PreferencesKeys.gridSizeInput,
                            label: 'Grid size',
                            value: _preferences.backgroundPreferences.gridSize,
                            onChanged: (value) {
                              setState(() {
                                _preferences.backgroundPreferences.gridSize =
                                    value;
                              });
                              _applyPreferences();
                            },
                            minimumValue: 1,
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Grid color
                          IVec3Input(
                            key: PreferencesKeys.gridColorInput,
                            label: 'Grid color (RGB)',
                            value: _preferences.backgroundPreferences.gridColor,
                            onChanged: (value) {
                              setState(() {
                                _preferences.backgroundPreferences.gridColor =
                                    value;
                              });
                              _applyPreferences();
                            },
                            minimumValue: const APIIVec3(x: 0, y: 0, z: 0),
                            maximumValue:
                                const APIIVec3(x: 255, y: 255, z: 255),
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Grid strong color
                          IVec3Input(
                            key: PreferencesKeys.gridStrongColorInput,
                            label: 'Grid strong color (RGB)',
                            value: _preferences
                                .backgroundPreferences.gridStrongColor,
                            onChanged: (value) {
                              setState(() {
                                _preferences.backgroundPreferences
                                    .gridStrongColor = value;
                              });
                              _applyPreferences();
                            },
                            minimumValue: const APIIVec3(x: 0, y: 0, z: 0),
                            maximumValue:
                                const APIIVec3(x: 255, y: 255, z: 255),
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Show lattice axes checkbox
                          Row(
                            children: [
                              Checkbox(
                                key: PreferencesKeys.showLatticeAxesCheckbox,
                                value: _preferences
                                    .backgroundPreferences.showLatticeAxes,
                                onChanged: (value) {
                                  if (value != null) {
                                    setState(() {
                                      _preferences.backgroundPreferences
                                          .showLatticeAxes = value;
                                    });
                                    _applyPreferences();
                                  }
                                },
                              ),
                              const SizedBox(width: 8),
                              const Expanded(
                                child: Text(
                                    'Show lattice axes (dotted lines for non-Cartesian lattices)'),
                              ),
                            ],
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Show lattice grid checkbox
                          Row(
                            children: [
                              Checkbox(
                                key: PreferencesKeys.showLatticeGridCheckbox,
                                value: _preferences
                                    .backgroundPreferences.showLatticeGrid,
                                onChanged: (value) {
                                  if (value != null) {
                                    setState(() {
                                      _preferences.backgroundPreferences
                                          .showLatticeGrid = value;
                                    });
                                    _applyPreferences();
                                  }
                                },
                              ),
                              const SizedBox(width: 8),
                              const Expanded(
                                child: Text(
                                    'Show lattice grid (secondary grid for non-Cartesian lattices)'),
                              ),
                            ],
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Lattice grid color
                          IVec3Input(
                            key: PreferencesKeys.latticeGridColorInput,
                            label: 'Lattice grid color (RGB)',
                            value: _preferences
                                .backgroundPreferences.latticeGridColor,
                            onChanged: (value) {
                              setState(() {
                                _preferences.backgroundPreferences
                                    .latticeGridColor = value;
                              });
                              _applyPreferences();
                            },
                            minimumValue: const APIIVec3(x: 0, y: 0, z: 0),
                            maximumValue:
                                const APIIVec3(x: 255, y: 255, z: 255),
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Lattice grid strong color
                          IVec3Input(
                            key: PreferencesKeys.latticeGridStrongColorInput,
                            label: 'Lattice grid strong color (RGB)',
                            value: _preferences
                                .backgroundPreferences.latticeGridStrongColor,
                            onChanged: (value) {
                              setState(() {
                                _preferences.backgroundPreferences
                                    .latticeGridStrongColor = value;
                              });
                              _applyPreferences();
                            },
                            minimumValue: const APIIVec3(x: 0, y: 0, z: 0),
                            maximumValue:
                                const APIIVec3(x: 255, y: 255, z: 255),
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Drawing plane grid color
                          IVec3Input(
                            key: PreferencesKeys.drawingPlaneGridColorInput,
                            label: 'Drawing plane grid color (RGB)',
                            value: _preferences
                                .backgroundPreferences.drawingPlaneGridColor,
                            onChanged: (value) {
                              setState(() {
                                _preferences.backgroundPreferences
                                    .drawingPlaneGridColor = value;
                              });
                              _applyPreferences();
                            },
                            minimumValue: const APIIVec3(x: 0, y: 0, z: 0),
                            maximumValue:
                                const APIIVec3(x: 255, y: 255, z: 255),
                          ),
                          const SizedBox(height: AppSpacing.medium),

                          // Drawing plane grid strong color
                          IVec3Input(
                            key: PreferencesKeys
                                .drawingPlaneGridStrongColorInput,
                            label: 'Drawing plane grid strong color (RGB)',
                            value: _preferences.backgroundPreferences
                                .drawingPlaneGridStrongColor,
                            onChanged: (value) {
                              setState(() {
                                _preferences.backgroundPreferences
                                    .drawingPlaneGridStrongColor = value;
                              });
                              _applyPreferences();
                            },
                            minimumValue: const APIIVec3(x: 0, y: 0, z: 0),
                            maximumValue:
                                const APIIVec3(x: 255, y: 255, z: 255),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
