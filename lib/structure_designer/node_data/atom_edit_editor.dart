import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/atom_edit_api.dart'
    as atom_edit_api;
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
          // Measurement display (tool-independent, shown when 2-4 atoms selected)
          if (_stagedData!.measurement != null) ...[
            const SizedBox(height: AppSpacing.large),
            _buildMeasurementDisplay(_stagedData!.measurement!),
          ],
          if (_stagedData!.activeTool == APIAtomEditTool.default_) ...[
            const SizedBox(height: AppSpacing.large),
            _buildCollapsibleMinimizeSection(),
            if (_stagedData!.hasSelection) ...[
              const SizedBox(height: AppSpacing.small),
              _buildCollapsibleTransformSection(),
            ],
          ],
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
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _buildDefaultToolUI(),
            _buildDefaultToolBondInfo(),
          ],
        );
      case APIAtomEditTool.addAtom:
        return _buildAddAtomToolUI();
      case APIAtomEditTool.addBond:
        return _buildAddBondToolUI();
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
            const SizedBox(height: AppSpacing.medium),
            Row(
              children: [
                Text('Hybridization:', style: TextStyle(fontSize: 13)),
                const SizedBox(width: 8),
                SegmentedButton<APIHybridization>(
                  segments: const [
                    ButtonSegment<APIHybridization>(
                      value: APIHybridization.auto,
                      label: Text('Auto'),
                    ),
                    ButtonSegment<APIHybridization>(
                      value: APIHybridization.sp3,
                      label: Text('sp3'),
                    ),
                    ButtonSegment<APIHybridization>(
                      value: APIHybridization.sp2,
                      label: Text('sp2'),
                    ),
                    ButtonSegment<APIHybridization>(
                      value: APIHybridization.sp1,
                      label: Text('sp1'),
                    ),
                  ],
                  selected: {widget.model.hybridizationOverride},
                  onSelectionChanged: (Set<APIHybridization> selection) {
                    widget.model.hybridizationOverride = selection.first;
                  },
                  style: ButtonStyle(
                    visualDensity: AppSpacing.compactVerticalDensity,
                    textStyle: WidgetStatePropertyAll(
                      TextStyle(fontSize: 12),
                    ),
                    padding: WidgetStatePropertyAll(
                      EdgeInsets.symmetric(horizontal: 4),
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: AppSpacing.medium),
            Row(
              children: [
                Text('Bond mode:', style: TextStyle(fontSize: 13)),
                const SizedBox(width: 8),
                SegmentedButton<APIBondMode>(
                  segments: const [
                    ButtonSegment<APIBondMode>(
                      value: APIBondMode.covalent,
                      label: Text('Covalent'),
                    ),
                    ButtonSegment<APIBondMode>(
                      value: APIBondMode.dative,
                      label: Text('Covalent + Dative'),
                    ),
                  ],
                  selected: {widget.model.bondMode},
                  onSelectionChanged: (Set<APIBondMode> selection) {
                    widget.model.bondMode = selection.first;
                  },
                  style: ButtonStyle(
                    visualDensity: AppSpacing.compactVerticalDensity,
                    textStyle: WidgetStatePropertyAll(
                      TextStyle(fontSize: 12),
                    ),
                    padding: WidgetStatePropertyAll(
                      EdgeInsets.symmetric(horizontal: 4),
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: AppSpacing.medium),
            Row(
              children: [
                Text('Bond length:', style: TextStyle(fontSize: 13)),
                const SizedBox(width: 8),
                SegmentedButton<APIBondLengthMode>(
                  segments: const [
                    ButtonSegment<APIBondLengthMode>(
                      value: APIBondLengthMode.crystal,
                      label: Text('Crystal'),
                    ),
                    ButtonSegment<APIBondLengthMode>(
                      value: APIBondLengthMode.uff,
                      label: Text('UFF'),
                    ),
                  ],
                  selected: {widget.model.bondLengthMode},
                  onSelectionChanged: (Set<APIBondLengthMode> selection) {
                    widget.model.bondLengthMode = selection.first;
                  },
                  style: ButtonStyle(
                    visualDensity: AppSpacing.compactVerticalDensity,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildMeasurementDisplay(APIMeasurement measurement) {
    final String label;
    final String value;
    final IconData icon;

    switch (measurement) {
      case APIMeasurement_Distance(:final distance):
        label = 'Distance';
        value = '${distance.toStringAsFixed(3)} \u00C5';
        icon = Icons.straighten;
      case APIMeasurement_Angle(:final angleDegrees):
        label = 'Angle';
        value = '${angleDegrees.toStringAsFixed(1)}\u00B0';
        icon = Icons.architecture;
      case APIMeasurement_Dihedral(:final angleDegrees):
        label = 'Dihedral';
        value = '${angleDegrees.toStringAsFixed(1)}\u00B0';
        icon = Icons.rotate_right;
    }

    return Card(
      elevation: 0,
      margin: EdgeInsets.zero,
      color: Colors.blue[50],
      child: Padding(
        padding: const EdgeInsets.all(AppSpacing.medium),
        child: Row(
          children: [
            Icon(icon, size: 20, color: Colors.blue[700]),
            const SizedBox(width: 8),
            Text(
              '$label: ',
              style: TextStyle(
                fontWeight: FontWeight.w500,
                color: Colors.blue[900],
                fontSize: 13,
              ),
            ),
            Text(
              value,
              style: TextStyle(
                fontWeight: FontWeight.w600,
                color: Colors.blue[900],
                fontSize: 14,
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildCollapsibleMinimizeSection() {
    return Card(
      elevation: 0,
      margin: EdgeInsets.zero,
      color: Colors.grey[50],
      clipBehavior: Clip.antiAlias,
      child: ExpansionTile(
        title: Text('Energy Minimization',
            style: TextStyle(fontWeight: FontWeight.w500, fontSize: 14)),
        tilePadding:
            const EdgeInsets.symmetric(horizontal: AppSpacing.medium),
        childrenPadding: const EdgeInsets.fromLTRB(
            AppSpacing.medium, 0, AppSpacing.medium, AppSpacing.medium),
        initiallyExpanded: false,
        dense: true,
        children: [
          _buildMinimizeSectionContent(),
        ],
      ),
    );
  }

  Widget _buildCollapsibleTransformSection() {
    return Card(
      elevation: 0,
      margin: EdgeInsets.zero,
      color: Colors.grey[50],
      clipBehavior: Clip.antiAlias,
      child: ExpansionTile(
        title: Text('Transform Selected Atoms',
            style: TextStyle(fontWeight: FontWeight.w500, fontSize: 14)),
        tilePadding:
            const EdgeInsets.symmetric(horizontal: AppSpacing.medium),
        childrenPadding: const EdgeInsets.fromLTRB(
            AppSpacing.medium, 0, AppSpacing.medium, AppSpacing.medium),
        initiallyExpanded: false,
        dense: true,
        children: [
          TransformControlWidget(
            initialTransform: _stagedData!.selectionTransform,
            title: 'Transform',
            onApplyTransform: (APITransform transform) {
              widget.model.atomEditTransformSelected(transform);
            },
          ),
        ],
      ),
    );
  }

  Widget _buildMinimizeSectionContent() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
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
    );
  }

  Widget _buildAddBondToolUI() {
    return Card(
      elevation: 0,
      margin: EdgeInsets.zero,
      color: Colors.grey[50],
      child: Padding(
        padding: const EdgeInsets.all(AppSpacing.medium),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Add Bond Settings',
                style: TextStyle(fontWeight: FontWeight.w500)),
            const SizedBox(height: AppSpacing.medium),
            BondOrderSelector(
              selectedOrder: _stagedData!.bondToolBondOrder,
              onOrderChanged: (int order) {
                atom_edit_api.setAddBondOrder(order: order);
                widget.model.refreshFromKernel();
              },
            ),
            const SizedBox(height: AppSpacing.medium),
            Text(
              'Drag from atom to atom to bond.',
              style: TextStyle(fontSize: 12, color: Colors.grey[600]),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildDefaultToolBondInfo() {
    if (!_stagedData!.hasSelectedBonds) return const SizedBox.shrink();

    final count = _stagedData!.selectedBondCount;
    final order = _stagedData!.selectedBondOrder;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const SizedBox(height: AppSpacing.large),
        Card(
          elevation: 0,
          margin: EdgeInsets.zero,
          color: Colors.grey[50],
          child: Padding(
            padding: const EdgeInsets.all(AppSpacing.medium),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Selected: $count bond${count == 1 ? '' : 's'}',
                  style: TextStyle(fontWeight: FontWeight.w500),
                ),
                if (order != null) ...[
                  const SizedBox(height: AppSpacing.small),
                  Text(
                    'Order: ${BondOrderSelector.labelForOrder(order)}',
                    style: TextStyle(fontSize: 13, color: Colors.grey[700]),
                  ),
                ],
                const SizedBox(height: AppSpacing.medium),
                BondOrderSelector(
                  selectedOrder: order,
                  onOrderChanged: (int newOrder) {
                    atom_edit_api.changeSelectedBondsOrder(newOrder: newOrder);
                    widget.model.refreshFromKernel();
                  },
                ),
              ],
            ),
          ),
        ),
      ],
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

/// Bond order constants matching Rust InlineBond values.
const int BOND_SINGLE = 1;
const int BOND_DOUBLE = 2;
const int BOND_TRIPLE = 3;
const int BOND_QUADRUPLE = 4;
const int BOND_AROMATIC = 5;
const int BOND_DATIVE = 6;
const int BOND_METALLIC = 7;

/// Shared widget for selecting bond order. Used by both AddBond tool panel
/// and Default tool bond info panel. Two rows: common (Single/Double/Triple)
/// and specialized (Quad/Aromatic/Dative/Metallic), acting as a single radio group.
class BondOrderSelector extends StatelessWidget {
  /// Currently selected bond order (1-7), or null for mixed/no-selection state.
  final int? selectedOrder;

  /// Callback when user clicks a bond order button.
  final void Function(int order) onOrderChanged;

  const BondOrderSelector({
    super.key,
    required this.selectedOrder,
    required this.onOrderChanged,
  });

  static String labelForOrder(int order) {
    switch (order) {
      case BOND_SINGLE:
        return 'Single';
      case BOND_DOUBLE:
        return 'Double';
      case BOND_TRIPLE:
        return 'Triple';
      case BOND_QUADRUPLE:
        return 'Quad';
      case BOND_AROMATIC:
        return 'Aromatic';
      case BOND_DATIVE:
        return 'Dative';
      case BOND_METALLIC:
        return 'Metallic';
      default:
        return '?';
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // Common bond orders (row 1)
        SegmentedButton<int>(
          segments: const [
            ButtonSegment<int>(value: BOND_SINGLE, label: Text('Single')),
            ButtonSegment<int>(value: BOND_DOUBLE, label: Text('Double')),
            ButtonSegment<int>(value: BOND_TRIPLE, label: Text('Triple')),
          ],
          selected: selectedOrder != null &&
                  selectedOrder! >= BOND_SINGLE &&
                  selectedOrder! <= BOND_TRIPLE
              ? {selectedOrder!}
              : {},
          emptySelectionAllowed: true,
          onSelectionChanged: (Set<int> selection) {
            if (selection.isNotEmpty) {
              onOrderChanged(selection.first);
            }
          },
          style: ButtonStyle(
            visualDensity: AppSpacing.compactVerticalDensity,
            textStyle: WidgetStatePropertyAll(TextStyle(fontSize: 12)),
            padding:
                WidgetStatePropertyAll(EdgeInsets.symmetric(horizontal: 4)),
          ),
        ),
        const SizedBox(height: AppSpacing.small),
        // Specialized bond orders (row 2)
        SegmentedButton<int>(
          segments: const [
            ButtonSegment<int>(value: BOND_QUADRUPLE, label: Text('Quad')),
            ButtonSegment<int>(value: BOND_AROMATIC, label: Text('Arom')),
            ButtonSegment<int>(value: BOND_DATIVE, label: Text('Dative')),
            ButtonSegment<int>(value: BOND_METALLIC, label: Text('Metal')),
          ],
          selected: selectedOrder != null &&
                  selectedOrder! >= BOND_QUADRUPLE &&
                  selectedOrder! <= BOND_METALLIC
              ? {selectedOrder!}
              : {},
          emptySelectionAllowed: true,
          onSelectionChanged: (Set<int> selection) {
            if (selection.isNotEmpty) {
              onOrderChanged(selection.first);
            }
          },
          style: ButtonStyle(
            visualDensity: AppSpacing.compactVerticalDensity,
            textStyle: WidgetStatePropertyAll(TextStyle(fontSize: 11)),
            padding:
                WidgetStatePropertyAll(EdgeInsets.symmetric(horizontal: 2)),
          ),
        ),
      ],
    );
  }
}
