import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../src/rust/api/common_api_types.dart';
import 'structure_designer_model.dart';

class CameraControlWidget extends StatelessWidget {
  final StructureDesignerModel model;

  const CameraControlWidget({Key? key, required this.model}) : super(key: key);

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
                  isExpanded: true,
                  value: model.cameraCanonicalView,
                  onChanged: (APICameraCanonicalView? newValue) {
                    if (newValue != null) {
                      model.setCameraCanonicalView(newValue);
                    }
                  },
                  items: APICameraCanonicalView.values.map<DropdownMenuItem<APICameraCanonicalView>>(
                    (APICameraCanonicalView view) {
                      return DropdownMenuItem<APICameraCanonicalView>(
                        value: view,
                        child: Text(
                          _getViewName(view),
                          overflow: TextOverflow.ellipsis,
                        ),
                      );
                    }
                  ).toList(),
                ),
              ),
              const SizedBox(width: 8),
              // Perspective view button
              IconButton(
                icon: const Icon(Icons.panorama_horizontal),
                tooltip: 'Perspective View',
                color: model.isOrthographic ? Colors.grey : Theme.of(context).primaryColor,
                onPressed: () {
                  if (model.isOrthographic) {
                    model.setOrthographicMode(false);
                  }
                },
              ),
              // Orthographic view button
              IconButton(
                icon: const Icon(Icons.border_all_outlined),
                tooltip: 'Orthographic View',
                color: model.isOrthographic ? Theme.of(context).primaryColor : Colors.grey,
                onPressed: () {
                  if (!model.isOrthographic) {
                    model.setOrthographicMode(true);
                  }
                },
              ),
            ],
          );
        },
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
      default:
        return 'Unknown';
    }
  }
}
