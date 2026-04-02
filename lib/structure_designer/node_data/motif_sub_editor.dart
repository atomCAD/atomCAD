import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/common/parameter_element_override_editor.dart';

/// Editor widget for motif_sub nodes
class MotifSubEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIMotifSubData? data;
  final StructureDesignerModel model;

  const MotifSubEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Motif Substitution',
            nodeTypeName: 'motif_sub',
          ),
          const SizedBox(height: 8),
          ParameterElementOverrideEditor(
            availableParameters: data!.availableParameters,
            currentDefinitionText: data!.parameterElementValueDefinition,
            onChanged: (newText) {
              model.setMotifSubData(
                nodeId,
                APIMotifSubData(
                  parameterElementValueDefinition: newText,
                  error: null,
                  availableParameters: const [],
                ),
              );
            },
          ),
          if (data!.error != null) ...[
            const SizedBox(height: 8),
            Container(
              width: double.infinity,
              padding: const EdgeInsets.all(8.0),
              decoration: BoxDecoration(
                color: Theme.of(context).colorScheme.errorContainer,
                borderRadius: BorderRadius.circular(4.0),
                border: Border.all(
                  color: Theme.of(context).colorScheme.error,
                  width: 1.0,
                ),
              ),
              child: Text(
                data!.error!,
                style: TextStyle(
                  color: Theme.of(context).colorScheme.onErrorContainer,
                  fontSize: 12.0,
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}
