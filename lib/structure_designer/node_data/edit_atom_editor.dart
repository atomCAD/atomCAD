import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_description_button.dart';
import 'package:flutter_cad/common/select_element_widget.dart';
import 'package:flutter_cad/common/transform_control_widget.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';

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
  int? _replacementAtomicNumber;
  int? _addAtomAtomicNumber;

  @override
  void initState() {
    super.initState();
    setState(() {
      _stagedData = widget.data;
      _replacementAtomicNumber = widget.data?.replacementAtomicNumber;
      _addAtomAtomicNumber = widget.data?.addAtomToolAtomicNumber;
    });
  }

  @override
  void didUpdateWidget(EditAtomEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      setState(() {
        _stagedData = widget.data;
        _replacementAtomicNumber = widget.data?.replacementAtomicNumber;
        _addAtomAtomicNumber = widget.data?.addAtomToolAtomicNumber;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_stagedData == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(AppSpacing.medium),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Header with title, info button, and undo/redo buttons
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Row(
                children: [
                  Text('Edit Atom Tools',
                      style: Theme.of(context).textTheme.titleMedium),
                  const SizedBox(width: 8),
                  const NodeDescriptionButton(nodeTypeName: 'edit_atom'),
                ],
              ),
              // Undo/Redo buttons
              Row(
                children: [
                  // Undo button
                  IconButton(
                    icon: const Icon(Icons.undo),
                    onPressed: _stagedData!.canUndo
                        ? () {
                            widget.model.editAtomUndo();
                          }
                        : null,
                    tooltip: 'Undo',
                    color: _stagedData!.canUndo
                        ? AppColors.primaryAccent
                        : Colors.grey,
                  ),
                  // Redo button
                  IconButton(
                    icon: const Icon(Icons.redo),
                    onPressed: _stagedData!.canRedo
                        ? () {
                            widget.model.editAtomRedo();
                          }
                        : null,
                    tooltip: 'Redo',
                    color: _stagedData!.canRedo
                        ? AppColors.primaryAccent
                        : Colors.grey,
                  ),
                ],
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.large),
          // Tool buttons row
          Row(
            children: [
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 4.0),
                child: _buildToolButton(
                  context,
                  APIEditAtomTool.default_,
                  'Default',
                  Icons.pan_tool,
                ),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 4.0),
                child: _buildToolButton(
                  context,
                  APIEditAtomTool.addAtom,
                  'Add Atom',
                  Icons.add_circle_outline,
                ),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 4.0),
                child: _buildToolButton(
                  context,
                  APIEditAtomTool.addBond,
                  'Add Bond',
                  Icons.link,
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.large),
          // Tool-specific UI elements
          _buildToolSpecificUI(),
        ],
      ),
    );
  }

  Widget _buildToolSpecificUI() {
    // Display different UI elements based on the active tool
    switch (_stagedData!.activeTool) {
      case APIEditAtomTool.default_:
        return _buildDefaultToolUI();
      case APIEditAtomTool.addAtom:
        return _buildAddAtomToolUI();
      case APIEditAtomTool.addBond:
        return const SizedBox.shrink(); // No additional UI for Add Bond
    }
  }

  Widget _buildDefaultToolUI() {
    return Card(
      elevation: 0,
      margin: EdgeInsets.zero,
      color: Colors.grey[50],
      child: Padding(
        padding: const EdgeInsets.all(AppSpacing.medium),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Default Tool Settings',
                style: TextStyle(fontWeight: FontWeight.w500)),
            const SizedBox(height: AppSpacing.medium),
            Row(
              children: [
                Expanded(
                  child: SelectElementWidget(
                    value: _replacementAtomicNumber,
                    onChanged: (int? newValue) {
                      setState(() {
                        _replacementAtomicNumber = newValue;
                      });
                      if (newValue != null) {
                        widget.model.setEditAtomDefaultData(newValue);
                      }
                    },
                    label: 'Replace selected atoms with',
                    hint: 'Select an element',
                    required: true,
                  ),
                ),
                const SizedBox(width: 8),
                SizedBox(
                  height: AppSpacing.buttonHeight,
                  child: ElevatedButton(
                    onPressed: (_replacementAtomicNumber == null ||
                            !_stagedData!.hasSelectedAtoms)
                        ? null
                        : () {
                            // Call the replaceSelectedAtoms method with the selected atomic number
                            widget.model.replaceSelectedAtoms(
                                _replacementAtomicNumber!);
                          },
                    style: AppButtonStyles.primary,
                    child: const Text('Replace'),
                  ),
                ),
              ],
            ),
            const SizedBox(height: AppSpacing.medium),
            SizedBox(
              width: double.infinity,
              height: AppSpacing.buttonHeight,
              child: ElevatedButton(
                onPressed: _stagedData!.hasSelection
                    ? () {
                        // Call the deleteSelectedAtomsAndBonds method
                        widget.model.deleteSelectedAtomsAndBonds();
                      }
                    : null,
                style: AppButtonStyles.primary,
                child: const Text('Delete Selected'),
              ),
            ),
            if (_stagedData!.hasSelection) ...[
              const SizedBox(height: AppSpacing.large),
              const Divider(),
              const SizedBox(height: AppSpacing.small),
              Text('Transform Selected Atoms',
                  style: TextStyle(fontWeight: FontWeight.w500)),
              const SizedBox(height: AppSpacing.medium),
              TransformControlWidget(
                initialTransform: _stagedData!.selectionTransform,
                title: 'Transform',
                onApplyTransform: (APITransform transform) {
                  widget.model.transformSelected(transform);
                },
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _buildAddAtomToolUI() {
    return Card(
      elevation: 0,
      margin: EdgeInsets.zero,
      color: Colors.grey[50],
      child: Padding(
        padding: const EdgeInsets.all(AppSpacing.medium),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Add Atom Settings',
                style: TextStyle(fontWeight: FontWeight.w500)),
            const SizedBox(height: AppSpacing.medium),
            SelectElementWidget(
              value: _addAtomAtomicNumber,
              onChanged: (int? newValue) {
                setState(() {
                  _addAtomAtomicNumber = newValue;
                });
                if (newValue != null) {
                  widget.model.setEditAtomAddAtomData(newValue);
                }
              },
              label: 'Element to add:',
              hint: 'Select an element',
              required: true,
            ),
          ],
        ),
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
              color: isActive ? AppColors.textOnDark : AppColors.textPrimary,
              size: 24.0,
            ),
          ),
        ),
      ),
    );
  }
}
