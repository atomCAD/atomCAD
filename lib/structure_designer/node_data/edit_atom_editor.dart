import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer_api.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for edit_atom nodes
class EditAtomEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIEditAtomData? data;
  final StructureDesignerModel model;

  const EditAtomEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<EditAtomEditor> createState() => _EditAtomEditorState();
}

class _EditAtomEditorState extends State<EditAtomEditor> {
  APIEditAtomData? _stagedData;

  @override
  void initState() {
    super.initState();
    setState(() {
      _stagedData = widget.data;
    });
  }

  @override
  void didUpdateWidget(EditAtomEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      setState(() {
        _stagedData = widget.data;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_stagedData == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Edit Atom Tools',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 16),
          Row(
            children: [
              _buildToolButton(
                context, 
                APIEditAtomTool.default_, 
                'Default',
                Icons.pan_tool,
              ),
              const SizedBox(width: 8),
              _buildToolButton(
                context, 
                APIEditAtomTool.addAtom, 
                'Add Atom',
                Icons.add_circle_outline,
              ),
              const SizedBox(width: 8),
              _buildToolButton(
                context, 
                APIEditAtomTool.addBond, 
                'Add Bond',
                Icons.connecting_airports,
              ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _buildToolButton(
    BuildContext context,
    APIEditAtomTool tool,
    String tooltip,
    IconData iconData,
  ) {
    final isActive = _stagedData!.activeTool == tool;

    return Tooltip(
      message: tooltip,
      child: Material(
        color: isActive ? AppColors.primaryAccent : Colors.transparent,
        borderRadius: BorderRadius.circular(4.0),
        child: InkWell(
          borderRadius: BorderRadius.circular(4.0),
          onTap: () {
            // Set active tool when clicked
            widget.model.setActiveEditAtomTool(tool);
          },
          child: Container(
            padding: const EdgeInsets.all(8.0),
            child: Icon(
              iconData,
              color: isActive 
                  ? AppColors.textOnDark 
                  : AppColors.textPrimary,
              size: 24.0,
            ),
          ),
        ),
      ),
    );
  }
}
