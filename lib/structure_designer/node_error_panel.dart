import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A widget that displays error message for the selected node in the Structure Designer.
class NodeErrorPanel extends StatelessWidget {
  final StructureDesignerModel model;

  const NodeErrorPanel({
    super.key,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
          String? errorMessage;
          
          // Get the selected node and its error message, if any
          if (model.nodeNetworkView != null) {
            // Find the selected node
            final selectedNode = model.nodeNetworkView!.nodes.values
                .where((node) => node.selected)
                .firstOrNull;
            
            if (selectedNode != null) {
              errorMessage = selectedNode.error;
            }
          }

          if (errorMessage == null || errorMessage.isEmpty) {
            return const Center(
              child: Text(
                'No errors in selected node',
                style: TextStyle(
                  fontStyle: FontStyle.italic,
                  color: Colors.grey,
                ),
              ),
            );
          }

          return Padding(
            padding: const EdgeInsets.all(8.0),
            child: SingleChildScrollView(
              child: Text(
                errorMessage,
                style: AppTextStyles.regular.copyWith(
                  color: Colors.red,
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}
