import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/vec3_input.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A reusable widget that displays and allows editing of transformation data.
class TransformControlWidget extends StatefulWidget {
  /// The initial transform to display and edit
  final APITransform? initialTransform;
  
  /// Callback when the "Apply Transform" button is pressed
  final Function(APITransform) onApplyTransform;
  
  /// Optional title for the widget
  final String title;

  const TransformControlWidget({
    super.key,
    this.initialTransform,
    required this.onApplyTransform,
    this.title = 'Transform',
  });

  @override
  State<TransformControlWidget> createState() => _TransformControlWidgetState();
}

class _TransformControlWidgetState extends State<TransformControlWidget> {
  APITransform? _stagedTransform;

  @override
  void initState() {
    super.initState();
    _stagedTransform = widget.initialTransform;
  }

  @override
  void didUpdateWidget(TransformControlWidget oldWidget) {
    super.didUpdateWidget(oldWidget);
    // Update the transform if it's coming from the parent and has changed
    if (widget.initialTransform != oldWidget.initialTransform) {
      setState(() {
        _stagedTransform = widget.initialTransform;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // Translation section
        Vec3Input(
          label: 'Translation',
          value: _stagedTransform?.translation ?? APIVec3(x: 0, y: 0, z: 0),
          onChanged: (value) {
            setState(() {
              if (_stagedTransform != null) {
                _stagedTransform = APITransform(
                  translation: value,
                  rotation: _stagedTransform!.rotation,
                );
              } else {
                _stagedTransform = APITransform(
                  translation: value,
                  rotation: APIVec3(x: 0, y: 0, z: 0),
                );
              }
            });
          },
          onPasted: () {
            // Automatically apply transform when values are pasted
            if (_stagedTransform != null) {
              widget.onApplyTransform(_stagedTransform!);
            }
          },
        ),

        const SizedBox(height: 6),

        // Rotation section
        Vec3Input(
          label: 'Rotation',
          value: _stagedTransform?.rotation ?? APIVec3(x: 0, y: 0, z: 0),
          onChanged: (value) {
            setState(() {
              if (_stagedTransform != null) {
                _stagedTransform = APITransform(
                  translation: _stagedTransform!.translation,
                  rotation: value,
                );
              } else {
                _stagedTransform = APITransform(
                  translation: APIVec3(x: 0, y: 0, z: 0),
                  rotation: value,
                );
              }
            });
          },
          onPasted: () {
            // Automatically apply transform when values are pasted
            if (_stagedTransform != null) {
              widget.onApplyTransform(_stagedTransform!);
            }
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
                    widget.onApplyTransform(_stagedTransform!);
                  },
            style: AppButtonStyles.primary,
            child: const Text('Apply Transform'),
          ),
        ),
      ],
    );
  }
}
