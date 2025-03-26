import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/scene_composer/scene_composer_model.dart';
import 'package:flutter_cad/inputs/vec3_input.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';

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

  // Custom dropdown button style
  DropdownButton<String> _buildAxisDropdown() {
    return DropdownButton<String>(
      value: _selectedAxis,
      onChanged: (String? value) {
        if (value != null) {
          setState(() {
            _selectedAxis = value;
          });
        }
      },
      icon: const Icon(
        Icons.arrow_drop_down,
        size: 18,
      ),
      underline: Container(
        height: 1,
        color: _selectedAxis == 'X'
            ? AppColors.xAxisColor
            : _selectedAxis == 'Y'
                ? AppColors.yAxisColor
                : AppColors.zAxisColor,
      ),
      isDense: true,
      padding: const EdgeInsets.fromLTRB(8.0, 0, 0, 0),
      // Custom dropdown menu items
      selectedItemBuilder: (BuildContext context) {
        return ['X', 'Y', 'Z'].map<Widget>((String value) {
          return Container(
            alignment: Alignment.centerLeft,
            constraints: const BoxConstraints(minWidth: 28),
            child: Text(
              value,
              style: TextStyle(
                fontWeight: FontWeight.bold,
                color: value == 'X'
                    ? AppColors.xAxisColor
                    : value == 'Y'
                        ? AppColors.yAxisColor
                        : AppColors.zAxisColor,
              ),
            ),
          );
        }).toList();
      },
      items: ['X', 'Y', 'Z'].map<DropdownMenuItem<String>>((String value) {
        Color textColor;
        switch (value) {
          case 'X':
            textColor = AppColors.xAxisColor;
            break;
          case 'Y':
            textColor = AppColors.yAxisColor;
            break;
          case 'Z':
            textColor = AppColors.zAxisColor;
            break;
          default:
            textColor = Colors.blueGrey;
        }

        return DropdownMenuItem<String>(
          value: value,
          child: Container(
            alignment: Alignment.centerLeft,
            padding: const EdgeInsets.symmetric(horizontal: 4.0),
            child: Text(
              value,
              style: TextStyle(
                fontWeight: FontWeight.bold,
                color: textColor,
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

                const SizedBox(height: 6),

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

                const SizedBox(height: 6),

                // Apply button
                SizedBox(
                  width: double.infinity,
                  height: 32,
                  child: ElevatedButton(
                    onPressed: _stagedTransform == null
                        ? null
                        : () {
                            model.setSelectedFrameTransform(_stagedTransform!);
                          },
                    style: AppButtonStyles.primary,
                    child: const Text('Apply Transform'),
                  ),
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
                    _buildAxisDropdown(),
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
                                  final axisIndex = _selectedAxis == 'X'
                                      ? 0
                                      : _selectedAxis == 'Y'
                                          ? 1
                                          : 2;
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
                    _buildAxisDropdown(),
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
                        style: AppButtonStyles.primary,
                        child: const Text('Rotate'),
                      ),
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
