import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/structure_designer/graph_model.dart';
import 'package:flutter_cad/common/cad_viewport.dart';

class StructureDesignerViewport extends CadViewport {
  final GraphModel graphModel;

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
