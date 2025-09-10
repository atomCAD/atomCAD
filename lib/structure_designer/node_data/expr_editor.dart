import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for expression nodes
class ExprEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIExprData? data;
  final StructureDesignerModel model;

  const ExprEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ExprEditor> createState() => ExprEditorState();
}

class ExprEditorState extends State<ExprEditor> {
  void _updateExprData(List<APIExprParameter> parameters) {
    widget.model.setExprData(
      widget.nodeId,
      APIExprData(parameters: parameters),
    );
  }

  void _addParameter() {
    final currentParameters = widget.data?.parameters ?? [];
    final newParameters = List<APIExprParameter>.from(currentParameters)
      ..add(APIExprParameter(
        name: 'param${currentParameters.length + 1}',
        dataType: APIDataType.float,
      ));
    _updateExprData(newParameters);
  }

  void _removeParameter(int index) {
    final currentParameters = widget.data?.parameters ?? [];
    if (index >= 0 && index < currentParameters.length) {
      final newParameters = List<APIExprParameter>.from(currentParameters)
        ..removeAt(index);
      _updateExprData(newParameters);
    }
  }

  void _updateParameter(int index, APIExprParameter updatedParameter) {
    final currentParameters = widget.data?.parameters ?? [];
    if (index >= 0 && index < currentParameters.length) {
      final newParameters = List<APIExprParameter>.from(currentParameters);
      newParameters[index] = updatedParameter;
      _updateExprData(newParameters);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final parameters = widget.data!.parameters;

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Expression Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          
          // Parameters section
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text('Parameters', style: Theme.of(context).textTheme.titleSmall),
              ElevatedButton.icon(
                onPressed: _addParameter,
                icon: const Icon(Icons.add),
                label: const Text('Add Parameter'),
                style: ElevatedButton.styleFrom(
                  padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),

          // Parameters list
          if (parameters.isEmpty)
            Card(
              child: Padding(
                padding: const EdgeInsets.all(16.0),
                child: Center(
                  child: Text(
                    'No parameters defined. Click "Add Parameter" to get started.',
                    style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                    ),
                  ),
                ),
              ),
            )
          else
            ...parameters.asMap().entries.map((entry) {
              final index = entry.key;
              final parameter = entry.value;
              
              return Card(
                margin: const EdgeInsets.only(bottom: 8.0),
                child: Padding(
                  padding: const EdgeInsets.all(12.0),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          Text(
                            'Parameter ${index + 1}',
                            style: Theme.of(context).textTheme.titleSmall,
                          ),
                          IconButton(
                            onPressed: () => _removeParameter(index),
                            icon: const Icon(Icons.delete),
                            tooltip: 'Delete Parameter',
                            color: Theme.of(context).colorScheme.error,
                          ),
                        ],
                      ),
                      const SizedBox(height: 8),
                      
                      // Parameter name input
                      StringInput(
                        label: 'Name',
                        value: parameter.name,
                        onChanged: (newName) {
                          _updateParameter(
                            index,
                            APIExprParameter(
                              name: newName,
                              dataType: parameter.dataType,
                            ),
                          );
                        },
                      ),
                      const SizedBox(height: 8),
                      
                      // Data type dropdown
                      DropdownButtonFormField<APIDataType>(
                        value: parameter.dataType,
                        decoration: const InputDecoration(
                          labelText: 'Data Type',
                          border: OutlineInputBorder(),
                        ),
                        items: APIDataType.values.map((dataType) {
                          return DropdownMenuItem(
                            value: dataType,
                            child: Text(getApiDataTypeDisplayName(dataType: dataType)),
                          );
                        }).toList(),
                        onChanged: (newDataType) {
                          if (newDataType != null) {
                            _updateParameter(
                              index,
                              APIExprParameter(
                                name: parameter.name,
                                dataType: newDataType,
                              ),
                            );
                          }
                        },
                      ),
                    ],
                  ),
                ),
              );
            }).toList(),
        ],
      ),
    );
  }
}
