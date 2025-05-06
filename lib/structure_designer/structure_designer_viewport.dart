import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/cad_viewport.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/common/ui_common.dart';

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
      final selectModifier = getSelectModifierFromKeyboard();
      widget.graphModel.selectAtomOrBondByRay(
        ray.start,
        ray.direction,
        selectModifier,
      );
    }
  }

  @override
  void refreshFromKernel() {
    widget.graphModel.refreshFromKernel();
  }
}
