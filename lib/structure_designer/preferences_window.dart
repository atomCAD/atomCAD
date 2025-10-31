import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

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
        case 1: // Solid (Explicit Mesh)
          _preferences.geometryVisualizationPreferences.geometryVisualization =
              GeometryVisualization.explicitMesh;
          _preferences.geometryVisualizationPreferences.wireframeGeometry =
              false;
          break;
        case 2: // Wireframe (Explicit Mesh)
          _preferences.geometryVisualizationPreferences.geometryVisualization =
              GeometryVisualization.explicitMesh;
          _preferences.geometryVisualizationPreferences.wireframeGeometry =
              true;
          break;
        case 3: // Solid (Dual Contouring)
          _preferences.geometryVisualizationPreferences.geometryVisualization =
              GeometryVisualization.dualContouring;
          _preferences.geometryVisualizationPreferences.wireframeGeometry =
              false;
          break;
        case 4: // Wireframe (Dual Contouring)
          _preferences.geometryVisualizationPreferences.geometryVisualization =
              GeometryVisualization.dualContouring;
          _preferences.geometryVisualizationPreferences.wireframeGeometry =
              true;
          break;
      }
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
    } else if (_preferences
            .geometryVisualizationPreferences.geometryVisualization ==
        GeometryVisualization.dualContouring) {
      return _preferences.geometryVisualizationPreferences.wireframeGeometry
          ? 4
          : 3;
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
                                    child: Text('Solid (explicit mesh)'),
                                  ),
                                  DropdownMenuItem(
                                    value: 2,
                                    child: Text('Wireframe (explicit mesh)'),
                                  ),
                                  DropdownMenuItem(
                                    value: 3,
                                    child: Text('Solid (dual contouring)'),
                                  ),
                                  DropdownMenuItem(
                                    value: 4,
                                    child: Text('Wireframe (dual contouring)'),
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

                          // Ball and stick cull depth setting
                          FloatInput(
                            label: 'Depth culling threshold (Å)',
                            value: _preferences
                                .atomicStructureVisualizationPreferences
                                .ballAndStickCullDepth ?? 0.0,
                            onChanged: (value) {
                              setState(() {
                                _preferences
                                    .atomicStructureVisualizationPreferences
                                    .ballAndStickCullDepth = value > 0.0 ? value : null;
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
