import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/scene_composer/scene_composer_model.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/scene_composer_api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/scene_composer/transform_control_widget.dart';

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
  // Constants for axis values
  static const List<String> _axisOptions = ['X', 'Y', 'Z'];

  SceneComposerView? sceneComposerView;
  APITransform? _stagedTransform;
  String _selectedTranslationAxis = 'X';
  String _selectedRotationAxis = 'X';
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

  // Convert axis name (X, Y, Z) to index (0, 1, 2)
  int _getAxisIndex(String axisName) {
    switch (axisName) {
      case 'X':
        return 0;
      case 'Y':
        return 1;
      case 'Z':
        return 2;
      default:
        return 0; // Default to X axis
    }
  }

  // Get the color for a specific axis
  Color _getAxisColor(String axisName) {
    switch (axisName) {
      case 'X':
        return AppColors.xAxisColor;
      case 'Y':
        return AppColors.yAxisColor;
      case 'Z':
        return AppColors.zAxisColor;
      default:
        return Colors.blueGrey;
    }
  }

  // Common dropdown button builder for axis selection
  DropdownButton<String> _buildAxisDropdown({
    required String currentValue,
    required ValueChanged<String?> onChanged,
  }) {
    return DropdownButton<String>(
      value: currentValue,
      onChanged: onChanged,
      icon: const Icon(
        Icons.arrow_drop_down,
        size: 18,
      ),
      underline: Container(
        height: 1,
        color: _getAxisColor(currentValue),
      ),
      isDense: true,
      padding: const EdgeInsets.fromLTRB(8.0, 0, 0, 0),
      // Custom dropdown menu items
      selectedItemBuilder: (BuildContext context) {
        return _axisOptions.map<Widget>((String value) {
          return Container(
            alignment: Alignment.centerLeft,
            constraints: const BoxConstraints(minWidth: 28),
            child: Text(
              value,
              style: TextStyle(
                fontWeight: FontWeight.bold,
                color: _getAxisColor(value),
              ),
            ),
          );
        }).toList();
      },
      items: _axisOptions.map<DropdownMenuItem<String>>((String value) {
        return DropdownMenuItem<String>(
          value: value,
          child: Container(
            alignment: Alignment.centerLeft,
            padding: const EdgeInsets.symmetric(horizontal: 4.0),
            child: Text(
              value,
              style: TextStyle(
                fontWeight: FontWeight.bold,
                color: _getAxisColor(value),
              ),
            ),
          ),
        );
      }).toList(),
    );
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
            padding: const EdgeInsets.fromLTRB(12.0, 8.0, 8.0, 8.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // Transform control section
                TransformControlWidget(
                  initialTransform: _stagedTransform,
                  onApplyTransform: (transform) {
                    model.setSelectedFrameTransform(transform);
                  },
                ),

                const SizedBox(height: 10),
                const Divider(height: 1),
                const SizedBox(height: 6),

                // Translate along axis section
                Row(
                  children: [
                    Container(
                      width: AppSpacing.labelWidth,
                      alignment: Alignment.centerRight,
                      child: const Text('Along:', style: AppTextStyles.label),
                    ),
                    const SizedBox(width: AppSpacing.small),
                    _buildAxisDropdown(
                      currentValue: _selectedTranslationAxis,
                      onChanged: (String? value) {
                        if (value != null) {
                          setState(() {
                            _selectedTranslationAxis = value;
                          });
                        }
                      },
                    ),
                    const SizedBox(width: AppSpacing.small),
                    Expanded(
                      child: TextField(
                        controller: _translationValueController,
                        style: AppTextStyles.inputField,
                        decoration: AppInputDecorations.standard.copyWith(
                          labelText: 'Distance',
                        ),
                        keyboardType: const TextInputType.numberWithOptions(
                            decimal: true),
                      ),
                    ),
                    const SizedBox(width: AppSpacing.small),
                    SizedBox(
                      height: AppSpacing.buttonHeight,
                      width: AppSpacing.smallButtonWidth,
                      child: ElevatedButton(
                        onPressed: _stagedTransform == null
                            ? null
                            : () {
                                final value = double.tryParse(
                                    _translationValueController.text);
                                if (value != null) {
                                  final axisIndex =
                                      _getAxisIndex(_selectedTranslationAxis);
                                  model.translateAlongLocalAxis(
                                      axisIndex, value);
                                  // Update staged transform after modification
                                  _updateStagedTransform();
                                }
                              },
                        style: AppButtonStyles.primary,
                        child: const Text('Trans.'),
                      ),
                    ),
                  ],
                ),

                const SizedBox(height: 6),

                // Rotate around axis section
                Row(
                  children: [
                    Container(
                      width: AppSpacing.labelWidth,
                      alignment: Alignment.centerRight,
                      child: const Text('Around:', style: AppTextStyles.label),
                    ),
                    const SizedBox(width: AppSpacing.small),
                    _buildAxisDropdown(
                      currentValue: _selectedRotationAxis,
                      onChanged: (String? value) {
                        if (value != null) {
                          setState(() {
                            _selectedRotationAxis = value;
                          });
                        }
                      },
                    ),
                    const SizedBox(width: AppSpacing.small),
                    Expanded(
                      child: TextField(
                        controller: _rotationValueController,
                        style: AppTextStyles.inputField,
                        decoration: AppInputDecorations.standard.copyWith(
                          labelText: 'Angle',
                        ),
                        keyboardType: const TextInputType.numberWithOptions(
                            decimal: true),
                      ),
                    ),
                    const SizedBox(width: AppSpacing.small),
                    SizedBox(
                      height: AppSpacing.buttonHeight,
                      width: AppSpacing.smallButtonWidth,
                      child: ElevatedButton(
                        onPressed: _stagedTransform == null
                            ? null
                            : () {
                                final value = double.tryParse(
                                    _rotationValueController.text);
                                if (value != null) {
                                  final axisIndex =
                                      _getAxisIndex(_selectedRotationAxis);
                                  model.rotateAroundLocalAxis(axisIndex, value);
                                  // Update staged transform after modification
                                  _updateStagedTransform();
                                }
                              },
                        style: AppButtonStyles.primary,
                        child: const Text('Rotate'),
                      ),
                    ),
                  ],
                ),

                const SizedBox(height: 10),
                const Divider(height: 1),
                const SizedBox(height: 2),

                // Frame lock to atoms toggle
                Row(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    const Text('Lock frame to atoms:',
                        style: AppTextStyles.label),
                    const SizedBox(width: 10),
                    IconButton(
                      icon: Icon(
                        model.isFrameLockedToAtoms()
                            ? Icons.lock
                            : Icons.lock_open,
                        color: model.isFrameLockedToAtoms()
                            ? Colors.blue
                            : Colors.grey,
                      ),
                      tooltip: model.isFrameLockedToAtoms()
                          ? 'Frame is locked to atoms'
                          : 'Frame is not locked to atoms',
                      onPressed: _stagedTransform == null
                          ? null
                          : () {
                              // Toggle the frame lock state
                              model.setFrameLockedToAtoms(
                                  !model.isFrameLockedToAtoms());
                            },
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
