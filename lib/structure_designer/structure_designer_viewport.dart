import 'package:flutter_cad/src/rust/api/common_api.dart' as common_api;
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/cad_viewport.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/edit_atom_api.dart'
    as edit_atom_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as structure_designer_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

class StructureDesignerViewport extends CadViewport {
  final StructureDesignerModel graphModel;

  const StructureDesignerViewport({
    super.key,
    required this.graphModel,
  });

  @override
  _StructureDesignerViewportState createState() =>
      _StructureDesignerViewportState();
}

class _StructureDesignerViewportState
    extends CadViewportState<StructureDesignerViewport> {
  @override
  void onDefaultClick(Offset pointerPos) {
    if (widget.graphModel.isEditAtomActive()) {
      final ray = getRayFromPointerPos(pointerPos);
      final activeEditAtomTool = edit_atom_api.getActiveEditAtomTool();

      // Find the selected node
      final selectedNode = widget.graphModel.nodeNetworkView?.nodes.entries
          .where((entry) => entry.value.selected)
          .map((entry) => entry.value)
          .firstOrNull;

      if (activeEditAtomTool == APIEditAtomTool.addAtom) {
        // Get the atomic number from the current edit atom data
        final editAtomData = structure_designer_api.getEditAtomData(
          nodeId: selectedNode?.id ?? BigInt.zero,
        );

        if (editAtomData != null) {
          final camera = common_api.getCamera();
          final cameraTransform = getCameraTransform(camera);
          final planeNormal = cameraTransform!.forward;

          widget.graphModel.addAtomByRay(
            editAtomData.addAtomToolAtomicNumber!,
            planeNormal,
            ray.start,
            ray.direction,
          );
        }
      } else if (activeEditAtomTool == APIEditAtomTool.addBond) {
        // Add bond tool - create bonds between atoms
        widget.graphModel.drawBondByRay(
          ray.start,
          ray.direction,
        );
      } else if (activeEditAtomTool == APIEditAtomTool.default_) {
        // Default tool behavior - select atoms/bonds
        final selectModifier = getSelectModifierFromKeyboard();
        widget.graphModel.selectAtomOrBondByRay(
          ray.start,
          ray.direction,
          selectModifier,
        );
      }
    }
  }

  @override
  void refreshFromKernel() {
    widget.graphModel.refreshFromKernel();
  }
}
