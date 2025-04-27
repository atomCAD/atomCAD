import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/cad_viewport.dart';

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
  void refreshFromKernel() {
    widget.graphModel.refreshFromKernel();
  }
}
