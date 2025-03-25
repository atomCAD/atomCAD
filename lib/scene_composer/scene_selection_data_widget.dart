import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/inputs/vec3_input.dart';
import 'package:flutter_cad/scene_composer/scene_composer_model.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';

/// A widget that displays and allows editing of the selected frame transformation.
class SceneSelectionDataWidget extends StatefulWidget {
  final SceneComposerModel model;

  const SceneSelectionDataWidget({
    super.key,
    required this.model,
  });

  @override
  State<SceneSelectionDataWidget> createState() =>
      _SceneSelectionDataWidgetState();
}

class _SceneSelectionDataWidgetState extends State<SceneSelectionDataWidget> {
  SceneComposerView? sceneComposerView;
  APITransform? _stagedTransform;
  String _selectedAxis = 'X';
  TextEditingController _translationValueController =
      TextEditingController(text: '0.0');
  TextEditingController _rotationValueController =
      TextEditingController(text: '0.0');

  @override
  void initState() {
    super.initState();
    _updateStagedTransform();
  }

  void _updateStagedTransform() {
    final transform = widget.model.getSelectedFrameTransform();
    setState(() {
      _stagedTransform = transform;
    });
  }

  @override
  void dispose() {
    _translationValueController.dispose();
    _rotationValueController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: widget.model,
      child: Consumer<SceneComposerModel>(
        builder: (context, model, child) {
          // Only update transform from model, if the model actually changed
          // otherwise we keep the staged transform

          if (sceneComposerView != model.sceneComposerView) {
            _stagedTransform = model.getSelectedFrameTransform();
          }
          sceneComposerView = model.sceneComposerView;

          return Padding(
            padding: const EdgeInsets.all(16.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'Frame Transformation',
                  style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
                ),
                const SizedBox(height: 16),

                // Translation section
                Vec3Input(
                  label: 'Translation',
                  value: _stagedTransform?.translation ??
                      APIVec3(x: 0, y: 0, z: 0),
                  onChanged: (value) {
                    print("onChanged ${value.x} ${value.y} ${value.z}");
                    setState(() {
                      if (_stagedTransform != null) {
                        _stagedTransform = APITransform(
                          translation: value,
                          rotation: _stagedTransform!.rotation,
                        );
                      }
                    });
                  },
                ),

                const SizedBox(height: 16),

                // Rotation section
                Vec3Input(
                  label: 'Rotation',
                  value:
                      _stagedTransform?.rotation ?? APIVec3(x: 0, y: 0, z: 0),
                  onChanged: (value) {
                    setState(() {
                      if (_stagedTransform != null) {
                        _stagedTransform = APITransform(
                          translation: _stagedTransform!.translation,
                          rotation: value,
                        );
                      }
                    });
                  },
                ),

                const SizedBox(height: 16),

                // Apply button
                ElevatedButton(
                  onPressed: _stagedTransform == null
                      ? null
                      : () {
                          model.setSelectedFrameTransform(_stagedTransform!);
                        },
                  child: const Text('Apply Transform'),
                ),

                const SizedBox(height: 24),
                const Divider(),
                const SizedBox(height: 16),

                // Translate along axis section
                Row(
                  children: [
                    const Text('Along:'),
                    const SizedBox(width: 16),
                    DropdownButton<String>(
                      value: _selectedAxis,
                      items: const [
                        DropdownMenuItem(value: 'X', child: Text('X')),
                        DropdownMenuItem(value: 'Y', child: Text('Y')),
                        DropdownMenuItem(value: 'Z', child: Text('Z')),
                      ],
                      onChanged: (value) {
                        if (value != null) {
                          setState(() {
                            _selectedAxis = value;
                          });
                        }
                      },
                    ),
                    const SizedBox(width: 16),
                    Expanded(
                      child: TextField(
                        controller: _translationValueController,
                        decoration: const InputDecoration(
                          border: OutlineInputBorder(),
                          labelText: 'Distance',
                          contentPadding:
                              EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                        ),
                        keyboardType: const TextInputType.numberWithOptions(
                            decimal: true),
                      ),
                    ),
                    const SizedBox(width: 16),
                    ElevatedButton(
                      onPressed: _stagedTransform == null
                          ? null
                          : () {
                              final value = double.tryParse(
                                  _translationValueController.text);
                              if (value != null) {
                                final axisIndex = _selectedAxis == 'X'
                                    ? 0
                                    : _selectedAxis == 'Y'
                                        ? 1
                                        : 2;
                                model.translateAlongLocalAxis(axisIndex, value);
                                // Update staged transform after modification
                                _updateStagedTransform();
                              }
                            },
                      child: const Text('Translate'),
                    ),
                  ],
                ),

                const SizedBox(height: 16),

                // Rotate around axis section
                Row(
                  children: [
                    const Text('Around:'),
                    const SizedBox(width: 16),
                    DropdownButton<String>(
                      value: _selectedAxis,
                      items: const [
                        DropdownMenuItem(value: 'X', child: Text('X')),
                        DropdownMenuItem(value: 'Y', child: Text('Y')),
                        DropdownMenuItem(value: 'Z', child: Text('Z')),
                      ],
                      onChanged: (value) {
                        if (value != null) {
                          setState(() {
                            _selectedAxis = value;
                          });
                        }
                      },
                    ),
                    const SizedBox(width: 16),
                    Expanded(
                      child: TextField(
                        controller: _rotationValueController,
                        decoration: const InputDecoration(
                          border: OutlineInputBorder(),
                          labelText: 'Degrees',
                          contentPadding:
                              EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                        ),
                        keyboardType: const TextInputType.numberWithOptions(
                            decimal: true),
                      ),
                    ),
                    const SizedBox(width: 16),
                    ElevatedButton(
                      onPressed: _stagedTransform == null
                          ? null
                          : () {
                              final value = double.tryParse(
                                  _rotationValueController.text);
                              if (value != null) {
                                final axisIndex = _selectedAxis == 'X'
                                    ? 0
                                    : _selectedAxis == 'Y'
                                        ? 1
                                        : 2;
                                model.rotateAroundLocalAxis(axisIndex, value);
                                // Update staged transform after modification
                                _updateStagedTransform();
                              }
                            },
                      child: const Text('Rotate'),
                    ),
                  ],
                ),
              ],
            ),
          );
        },
      ),
    );
  }
}
