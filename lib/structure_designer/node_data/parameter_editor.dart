import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// Editor widget for parameter nodes
class ParameterEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIParameterData? data;
  final StructureDesignerModel model;

  const ParameterEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ParameterEditor> createState() => ParameterEditorState();
}

class ParameterEditorState extends State<ParameterEditor> {
  // Staged data - only applied when Apply button is pressed
  late BigInt _stagedParamIndex;
  late String _stagedParamName;
  late APIDataType _stagedDataType;
  late bool _stagedMulti;
  late int _stagedSortOrder;

  // Controllers for text inputs
  late TextEditingController _paramNameController;
  late bool _hasChanges;

  @override
  void initState() {
    super.initState();
    _initializeStagedData();
    _paramNameController = TextEditingController(text: _stagedParamName);
    _hasChanges = false;
  }

  @override
  void didUpdateWidget(ParameterEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      _initializeStagedData();
      _paramNameController.text = _stagedParamName;
      _hasChanges = false;
    }
  }

  void _initializeStagedData() {
    if (widget.data != null) {
      _stagedParamIndex = widget.data!.paramIndex;
      _stagedParamName = widget.data!.paramName;
      _stagedDataType = widget.data!.dataType;
      _stagedMulti = widget.data!.multi;
      _stagedSortOrder = widget.data!.sortOrder;
    } else {
      // Default values for new parameter
      _stagedParamIndex = BigInt.zero;
      _stagedParamName = '';
      _stagedDataType = APIDataType.geometry;
      _stagedMulti = false;
      _stagedSortOrder = 0;
    }
  }

  void _markChanged() {
    if (!_hasChanges) {
      setState(() {
        _hasChanges = true;
      });
    }
  }

  void _applyChanges() {
    final newData = APIParameterData(
      paramIndex: _stagedParamIndex,
      paramName: _stagedParamName,
      dataType: _stagedDataType,
      multi: _stagedMulti,
      sortOrder: _stagedSortOrder,
    );
    
    widget.model.setParameterData(widget.nodeId, newData);
    
    setState(() {
      _hasChanges = false;
    });
  }

  void _resetChanges() {
    _initializeStagedData();
    _paramNameController.text = _stagedParamName;
    
    setState(() {
      _hasChanges = false;
    });
  }

  @override
  void dispose() {
    _paramNameController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Parameter Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          
          // Parameter Name
          TextField(
            controller: _paramNameController,
            decoration: const InputDecoration(
              labelText: 'Parameter Name',
              border: OutlineInputBorder(),
            ),
            onChanged: (newValue) {
              setState(() {
                _stagedParamName = newValue;
              });
              _markChanged();
            },
          ),
          const SizedBox(height: 8),
          
          // Data Type Dropdown
          DropdownButtonFormField<APIDataType>(
            value: _stagedDataType,
            decoration: const InputDecoration(
              labelText: 'Data Type',
              border: OutlineInputBorder(),
            ),
            items: APIDataType.values.map((dataType) {
              String displayName;
              switch (dataType) {
                case APIDataType.geometry2D:
                  displayName = 'Geometry 2D';
                  break;
                case APIDataType.geometry:
                  displayName = 'Geometry';
                  break;
                case APIDataType.atomic:
                  displayName = 'Atomic';
                  break;
              }
              return DropdownMenuItem(
                value: dataType,
                child: Text(displayName),
              );
            }).toList(),
            onChanged: (newValue) {
              if (newValue != null) {
                setState(() {
                  _stagedDataType = newValue;
                });
                _markChanged();
              }
            },
          ),
          const SizedBox(height: 8),
          
          // Multi Checkbox
          CheckboxListTile(
            title: const Text('Multi'),
            subtitle: const Text('Accept multiple inputs'),
            value: _stagedMulti,
            onChanged: (newValue) {
              if (newValue != null) {
                setState(() {
                  _stagedMulti = newValue;
                });
                _markChanged();
              }
            },
          ),
          const SizedBox(height: 8),
          
          // Sort Order
          IntInput(
            label: 'Sort Order',
            value: _stagedSortOrder,
            onChanged: (newValue) {
              setState(() {
                _stagedSortOrder = newValue;
              });
              _markChanged();
            },
          ),
          const SizedBox(height: 16),
          
          // Parameter Index (readonly, calculated by Rust)
          TextField(
            controller: TextEditingController(text: _stagedParamIndex.toString()),
            decoration: const InputDecoration(
              labelText: 'Parameter Index (calculated)',
              border: OutlineInputBorder(),
              enabled: false,
            ),
            readOnly: true,
          ),
          const SizedBox(height: 16),
          
          // Action Buttons
          Row(
            children: [
              Expanded(
                child: ElevatedButton(
                  onPressed: _hasChanges ? _applyChanges : null,
                  style: AppButtonStyles.primary,
                  child: const Text('Apply'),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: OutlinedButton(
                  onPressed: _hasChanges ? _resetChanges : null,
                  child: const Text('Reset'),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
