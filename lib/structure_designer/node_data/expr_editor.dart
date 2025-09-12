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
  late TextEditingController _expressionController;
  late FocusNode _expressionFocusNode;

  @override
  void initState() {
    super.initState();
    _expressionController = TextEditingController(text: widget.data?.expression ?? '');
    _expressionFocusNode = FocusNode();
    _expressionFocusNode.addListener(() {
      if (!_expressionFocusNode.hasFocus) {
        // When focus is lost, update the expression
        _updateExpressionFromText(_expressionController.text);
      }
    });
  }

  @override
  void didUpdateWidget(ExprEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data?.expression != widget.data?.expression) {
      _expressionController.text = widget.data?.expression ?? '';
    }
  }

  @override
  void dispose() {
    _expressionController.dispose();
    _expressionFocusNode.dispose();
    super.dispose();
  }

  void _updateExprData(List<APIExprParameter> parameters) {
    widget.model.setExprData(
      widget.nodeId,
      APIExprData(
        parameters: parameters,
        expression: widget.data?.expression ?? '',
        error: widget.data?.error,
        outputType: widget.data?.outputType,
      ),
    );
  }

  void _updateExpressionFromText(String expression) {
    widget.model.setExprData(
      widget.nodeId,
      APIExprData(
        parameters: widget.data?.parameters ?? [],
        expression: expression,
        error: widget.data?.error,
        outputType: widget.data?.outputType,
      ),
    );
  }

  String _generateParameterName(int index) {
    // Use x, y, z for first three parameters
    if (index == 0) return 'x';
    if (index == 1) return 'y';
    if (index == 2) return 'z';
    
    // After x, y, z, use w, u, v, s, t, then a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r
    const additionalNames = ['w', 'u', 'v', 's', 't', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r'];
    
    if (index - 3 < additionalNames.length) {
      return additionalNames[index - 3];
    }
    
    // If we run out of single letters, use param pattern
    return 'param${index + 1}';
  }

  void _addParameter() {
    final currentParameters = widget.data?.parameters ?? [];
    final newParameters = List<APIExprParameter>.from(currentParameters)
      ..add(APIExprParameter(
        name: _generateParameterName(currentParameters.length),
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
          
          // Expression text area
          TextFormField(
            controller: _expressionController,
            focusNode: _expressionFocusNode,
            decoration: const InputDecoration(
              labelText: 'Expression',
              border: OutlineInputBorder(),
              contentPadding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              hintText: 'Enter mathematical expression (e.g., x * 2 + sin(y))',
            ),
            maxLines: 3,
            minLines: 1,
            keyboardType: TextInputType.multiline,
            textInputAction: TextInputAction.done,
            onFieldSubmitted: (text) {
              _updateExpressionFromText(text);
            },
          ),
          
          // Error message display
          if (widget.data?.error != null)
            Padding(
              padding: const EdgeInsets.only(top: 8.0),
              child: Container(
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
                  widget.data!.error!,
                  style: TextStyle(
                    color: Theme.of(context).colorScheme.onErrorContainer,
                    fontSize: 12.0,
                  ),
                ),
              ),
            ),
          
          const SizedBox(height: 16),
          
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
            Padding(
              padding: const EdgeInsets.all(16.0),
              child: Center(
                child: Text(
                  'No parameters defined. Click "Add Parameter" to get started.',
                  style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                    color: Theme.of(context).colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
            )
          else
            ...parameters.asMap().entries.map((entry) {
              final index = entry.key;
              final parameter = entry.value;
              
              return Padding(
                padding: const EdgeInsets.only(bottom: 4.0),
                child: Row(
                  children: [
                    // Parameter name input - takes up available space
                    Expanded(
                      flex: 3,
                      child: StringInput(
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
                    ),
                    const SizedBox(width: 6),
                    
                    // Data type dropdown - more constrained
                    Expanded(
                      flex: 3,
                      child: DropdownButtonFormField<APIDataType>(
                        value: parameter.dataType,
                        decoration: const InputDecoration(
                          labelText: 'Type',
                          border: OutlineInputBorder(),
                          contentPadding: EdgeInsets.symmetric(horizontal: 8, vertical: 8),
                          isDense: true,
                        ),
                        isExpanded: true,
                        items: APIDataType.values.map((dataType) {
                          return DropdownMenuItem(
                            value: dataType,
                            child: Text(
                              getApiDataTypeDisplayName(dataType: dataType),
                              overflow: TextOverflow.ellipsis,
                            ),
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
                    ),
                    const SizedBox(width: 4),
                    
                    // Delete button - more compact
                    SizedBox(
                      width: 36,
                      height: 36,
                      child: IconButton(
                        onPressed: () => _removeParameter(index),
                        icon: const Icon(Icons.delete, size: 18),
                        tooltip: 'Delete Parameter',
                        color: Theme.of(context).colorScheme.error,
                        padding: EdgeInsets.zero,
                      ),
                    ),
                  ],
                ),
              );
            }).toList(),
          
          // Output type display
          if (widget.data?.outputType != null)
            Padding(
              padding: const EdgeInsets.only(top: 16.0),
              child: Container(
                width: double.infinity,
                padding: const EdgeInsets.all(12.0),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.primaryContainer,
                  borderRadius: BorderRadius.circular(8.0),
                  border: Border.all(
                    color: Theme.of(context).colorScheme.primary,
                    width: 1.0,
                  ),
                ),
                child: Row(
                  children: [
                    Icon(
                      Icons.output,
                      color: Theme.of(context).colorScheme.onPrimaryContainer,
                      size: 16.0,
                    ),
                    const SizedBox(width: 8.0),
                    Text(
                      'Output Type: ${getApiDataTypeDisplayName(dataType: widget.data!.outputType!)}',
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.onPrimaryContainer,
                        fontSize: 14.0,
                        fontWeight: FontWeight.w500,
                      ),
                    ),
                  ],
                ),
              ),
            ),
        ],
      ),
    );
  }
}
