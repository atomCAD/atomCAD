import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../src/rust/api/common_api_types.dart';
import 'structure_designer_model.dart';
import 'view_up_axis_dialog.dart';

class CameraControlWidget extends StatelessWidget {
  final StructureDesignerModel model;

  const CameraControlWidget({super.key, required this.model});

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
          return Row(
            children: [
              // Camera canonical view dropdown
              Expanded(
                child: DropdownButton<APICameraCanonicalView>(
                  key: const Key('camera_view_dropdown'),
                  isExpanded: true,
                  value: model.cameraCanonicalView,
                  onChanged: (APICameraCanonicalView? newValue) {
                    if (newValue != null) {
                      model.setCameraCanonicalView(newValue);
                    }
                  },
                  items: APICameraCanonicalView.values
                      .map<DropdownMenuItem<APICameraCanonicalView>>(
                          (APICameraCanonicalView view) {
                    return DropdownMenuItem<APICameraCanonicalView>(
                      value: view,
                      child: Text(
                        _getViewName(view),
                        overflow: TextOverflow.ellipsis,
                      ),
                    );
                  }).toList(),
                ),
              ),
              const SizedBox(width: 8),
              // Perspective view button
              IconButton(
                key: const Key('camera_perspective_button'),
                icon: const Icon(Icons.panorama_horizontal),
                tooltip: 'Perspective View',
                color: model.isOrthographic
                    ? Colors.grey
                    : Theme.of(context).primaryColor,
                onPressed: () {
                  if (model.isOrthographic) {
                    model.setOrthographicMode(false);
                  }
                },
              ),
              // Orthographic view button
              IconButton(
                key: const Key('camera_orthographic_button'),
                icon: const Icon(Icons.border_all_outlined),
                tooltip: 'Orthographic View',
                color: model.isOrthographic
                    ? Theme.of(context).primaryColor
                    : Colors.grey,
                onPressed: () {
                  if (!model.isOrthographic) {
                    model.setOrthographicMode(true);
                  }
                },
              ),
              const SizedBox(width: 8),
              // Navigation up-axis button (issue #349). Highlighted when a
              // non-default axis is active so a rotated turntable is never a
              // mystery. Shaped as the "navigation up-axis" entry so #97's free
              // mode and #391's gizmo slot in without redesign (D7).
              _buildUpAxisButton(context, model),
            ],
          );
        },
      ),
    );
  }

  Widget _buildUpAxisButton(
      BuildContext context, StructureDesignerModel model) {
    final info = model.viewUpInfo;
    final label = info?.label ?? 'Z';
    final isDefault = info?.isDefault ?? true;
    final primary = Theme.of(context).primaryColor;
    return Tooltip(
      message: 'Navigation up-axis (screen-vertical while orbiting)',
      child: TextButton.icon(
        key: const Key('camera_up_axis_button'),
        icon: Icon(
          Icons.vertical_align_top,
          size: 18,
          color: isDefault ? Colors.grey : primary,
        ),
        label: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 90),
          child: Text(
            'Up: $label',
            overflow: TextOverflow.ellipsis,
            softWrap: false,
            style: TextStyle(
              color: isDefault ? null : primary,
              fontWeight: isDefault ? FontWeight.normal : FontWeight.bold,
            ),
          ),
        ),
        onPressed: () => showViewUpAxisDialog(context, model),
      ),
    );
  }

  String _getViewName(APICameraCanonicalView view) {
    switch (view) {
      case APICameraCanonicalView.custom:
        return 'Custom';
      case APICameraCanonicalView.top:
        return 'Top';
      case APICameraCanonicalView.bottom:
        return 'Bottom';
      case APICameraCanonicalView.front:
        return 'Front';
      case APICameraCanonicalView.back:
        return 'Back';
      case APICameraCanonicalView.left:
        return 'Left';
      case APICameraCanonicalView.right:
        return 'Right';
    }
  }
}
