import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_description_button.dart';
import 'package:flutter_cad/common/select_element_widget.dart';
import 'package:flutter_cad/common/transform_control_widget.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';

/// Editor widget for atom_edit nodes (diff-based atomic structure editing)
class AtomEditEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIAtomEditData? data;
  final StructureDesignerModel model;

  const AtomEditEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<AtomEditEditor> createState() => _AtomEditEditorState();
}

class _AtomEditEditorState extends State<AtomEditEditor> {
  APIAtomEditData? _stagedData;
  int? _replacementAtomicNumber;
  int? _addAtomAtomicNumber;

  bool get _hasDiffChanges {
    final stats = _stagedData?.diffStats;
    if (stats == null) return false;
    return stats.atomsAdded > 0 ||
        stats.atomsDeleted > 0 ||
        stats.atomsModified > 0 ||
        stats.bondsAdded > 0 ||
        stats.bondsDeleted > 0;
  }

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
  void didUpdateWidget(AtomEditEditor oldWidget) {
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
          // Header with title and info button
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Row(
                children: [
                  Text('Atom Edit Tools',
                      style: Theme.of(context).textTheme.titleMedium),
                  const SizedBox(width: 8),
                  const NodeDescriptionButton(nodeTypeName: 'atom_edit'),
                ],
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.medium),
          // Output mode toggle and diff stats
          _buildOutputModeRow(),
          const SizedBox(height: AppSpacing.large),
          // Tool buttons row
          Row(
            children: [
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 4.0),
                child: _buildToolButton(
                  context,
                  APIAtomEditTool.default_,
                  'Default',
                  Icons.pan_tool,
                ),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 4.0),
                child: _buildToolButton(
                  context,
                  APIAtomEditTool.addAtom,
                  'Add Atom',
                  Icons.add_circle_outline,
                ),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 4.0),
                child: _buildToolButton(
                  context,
                  APIAtomEditTool.addBond,
                  'Add Bond',
                  Icons.link,
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.large),
          // Tool-specific UI elements
          _buildToolSpecificUI(),
          const SizedBox(height: AppSpacing.large),
          _buildMinimizeSection(),
        ],
      ),
    );
  }

  Widget _buildOutputModeRow() {
    final stats = _stagedData!.diffStats;
    final hasChanges = stats.atomsAdded > 0 ||
        stats.atomsDeleted > 0 ||
        stats.atomsModified > 0 ||
        stats.bondsAdded > 0 ||
        stats.bondsDeleted > 0;

    return Card(
      elevation: 0,
      margin: EdgeInsets.zero,
      color: Colors.grey[50],
      child: Padding(
        padding: const EdgeInsets.all(AppSpacing.medium),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Output mode toggle
            Row(
              children: [
                Text('View:', style: TextStyle(fontWeight: FontWeight.w500)),
                const SizedBox(width: 8),
                SegmentedButton<bool>(
                  segments: const [
                    ButtonSegment<bool>(
                      value: false,
                      label: Text('Result'),
                    ),
                    ButtonSegment<bool>(
                      value: true,
                      label: Text('Diff'),
                    ),
                  ],
                  selected: {_stagedData!.outputDiff},
                  onSelectionChanged: (Set<bool> selection) {
                    widget.model.toggleAtomEditOutputDiff();
                  },
                  style: ButtonStyle(
                    visualDensity: AppSpacing.compactVerticalDensity,
                  ),
                ),
              ],
            ),
            // Diff mode options (only visible in diff mode)
            if (_stagedData!.outputDiff) ...[
              const SizedBox(height: AppSpacing.small),
              Row(
                children: [
                  SizedBox(
                    height: 24,
                    width: 24,
                    child: Checkbox(
                      value: _stagedData!.showAnchorArrows,
                      onChanged: (_) {
                        widget.model.toggleAtomEditShowAnchorArrows();
                      },
                    ),
                  ),
                  const SizedBox(width: 8),
                  Text('Show anchor arrows'),
                ],
              ),
              const SizedBox(height: AppSpacing.small),
              Row(
                children: [
                  SizedBox(
                    height: 24,
                    width: 24,
                    child: Checkbox(
                      value: _stagedData!.includeBaseBondsInDiff,
                      onChanged: (_) {
                        widget.model.toggleAtomEditIncludeBaseBondsInDiff();
                      },
                    ),
                  ),
                  const SizedBox(width: 8),
                  Text('Include base bonds in the output diff'),
                ],
              ),
            ],
            // Diff statistics
            if (hasChanges) ...[
              const SizedBox(height: AppSpacing.small),
              Text(
                _buildStatsText(stats),
                style: TextStyle(
                  fontSize: 12,
                  color: Colors.grey[600],
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }

  String _buildStatsText(APIDiffStats stats) {
    final parts = <String>[];
    if (stats.atomsAdded > 0) parts.add('+${stats.atomsAdded} atoms');
    if (stats.atomsDeleted > 0) parts.add('-${stats.atomsDeleted} atoms');
    if (stats.atomsModified > 0) parts.add('~${stats.atomsModified} modified');
    if (stats.bondsAdded > 0) parts.add('+${stats.bondsAdded} bonds');
    if (stats.bondsDeleted > 0) parts.add('-${stats.bondsDeleted} bonds');
    return parts.join(', ');
  }

  Widget _buildToolSpecificUI() {
    switch (_stagedData!.activeTool) {
      case APIAtomEditTool.default_:
        return _buildDefaultToolUI();
      case APIAtomEditTool.addAtom:
        return _buildAddAtomToolUI();
      case APIAtomEditTool.addBond:
        return const SizedBox.shrink();
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
            Row(
              children: [
                Text('Default Tool Settings',
                    style: TextStyle(fontWeight: FontWeight.w500)),
                const Spacer(),
                Tooltip(
                  message: 'Show axis gadget on selection',
                  child: IconButton(
                    icon: Icon(
                      Icons.open_with,
                      color: _stagedData!.showGadget
                          ? AppColors.primaryAccent
                          : Colors.grey,
                      size: 20,
                    ),
                    onPressed: () {
                      widget.model.toggleAtomEditShowGadget();
                    },
                    visualDensity: VisualDensity.compact,
                    padding: EdgeInsets.zero,
                    constraints: const BoxConstraints(
                      minWidth: 32,
                      minHeight: 32,
                    ),
                  ),
                ),
              ],
            ),
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
                        widget.model.setAtomEditDefaultData(newValue);
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
                            widget.model.atomEditReplaceSelected(
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
                        widget.model.atomEditDeleteSelected();
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
                  widget.model.atomEditTransformSelected(transform);
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
                  widget.model.setAtomEditAddAtomData(newValue);
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

  Widget _buildMinimizeSection() {
    return Card(
      elevation: 0,
      margin: EdgeInsets.zero,
      color: Colors.grey[50],
      child: Padding(
        padding: const EdgeInsets.all(AppSpacing.medium),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Energy Minimization',
                style: TextStyle(fontWeight: FontWeight.w500)),
            const SizedBox(height: AppSpacing.medium),
            Row(
              children: [
                Expanded(
                  child: SizedBox(
                    height: 48,
                    child: ElevatedButton.icon(
                      onPressed: _hasDiffChanges
                          ? () {
                              widget.model.atomEditMinimize(
                                APIMinimizeFreezeMode.freezeBase,
                              );
                            }
                          : null,
                      icon: const Icon(Icons.lock_outline, size: 18),
                      label: const Text('Minimize\ndiff'),
                      style: AppButtonStyles.primary,
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: SizedBox(
                    height: 48,
                    child: ElevatedButton.icon(
                      onPressed: () {
                        widget.model.atomEditMinimize(
                          APIMinimizeFreezeMode.freeAll,
                        );
                      },
                      icon: const Icon(Icons.lock_open, size: 18),
                      label: const Text('Minimize\nall'),
                      style: AppButtonStyles.primary,
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: SizedBox(
                    height: 48,
                    child: ElevatedButton.icon(
                      onPressed: (_stagedData?.hasSelectedAtoms ?? false)
                          ? () {
                              widget.model.atomEditMinimize(
                                APIMinimizeFreezeMode.freeSelected,
                              );
                            }
                          : null,
                      icon: const Icon(Icons.filter_center_focus, size: 18),
                      label: const Text('Minimize\nselected'),
                      style: AppButtonStyles.primary,
                    ),
                  ),
                ),
              ],
            ),
            if (widget.model.lastMinimizeMessage.isNotEmpty) ...[
              const SizedBox(height: AppSpacing.small),
              Text(
                widget.model.lastMinimizeMessage,
                style: TextStyle(
                  fontSize: 12,
                  color: widget.model.lastMinimizeMessage.startsWith('Error')
                      ? Colors.red[700]
                      : Colors.grey[600],
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _buildToolButton(
    BuildContext context,
    APIAtomEditTool tool,
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
            widget.model.setActiveAtomEditTool(tool);
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
