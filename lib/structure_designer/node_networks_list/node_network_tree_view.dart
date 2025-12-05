import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Tree view widget for node networks with hierarchical namespace display.
/// (To be implemented with flutter_fancy_tree_view)
class NodeNetworkTreeView extends StatelessWidget {
  final StructureDesignerModel model;

  const NodeNetworkTreeView({super.key, required this.model});

  @override
  Widget build(BuildContext context) {
    return const Center(
      child: Text(
        'Tree view - Coming soon',
        style: TextStyle(fontSize: 16, color: Colors.grey),
      ),
    );
  }
}
