import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_description_button.dart';

/// Keys for the Add Node Popup used in integration testing.
class AddNodePopupKeys {
  static const Key dialog = Key('add_node_dialog');
  static const Key filterField = Key('add_node_filter_field');
  static const Key nodeListView = Key('add_node_list_view');
  static const Key descriptionPanel = Key('add_node_description_panel');
  static const Key descriptionTitle = Key('add_node_description_title');
  static const Key descriptionText = Key('add_node_description_text');

  /// Returns a Key for a category header in the node list
  static Key categoryHeader(NodeTypeCategory category) =>
      Key('add_node_category_${category.name}');

  /// Returns a Key for a node type item in the list
  static Key nodeItem(String nodeName) => Key('add_node_item_$nodeName');
}

// Helper function to get category display name
String getCategoryDisplayName(NodeTypeCategory category) {
  switch (category) {
    case NodeTypeCategory.annotation:
      return 'Annotation';
    case NodeTypeCategory.mathAndProgramming:
      return 'Math and Programming';
    case NodeTypeCategory.geometry2D:
      return '2D Geometry';
    case NodeTypeCategory.geometry3D:
      return '3D Geometry';
    case NodeTypeCategory.atomicStructure:
      return 'Atomic Structure';
    case NodeTypeCategory.otherBuiltin:
      return 'Other';
    case NodeTypeCategory.custom:
      return 'Custom';
  }
}

class AddNodePopup extends StatefulWidget {
  final String? filterByCompatibleType;
  final bool? draggingFromOutput;

  /// Whether the node will land inside a zone body (an HOF body or a `closure`
  /// body). When true, node types Rust marks `allowedInZoneBody: false` — today
  /// only `parameter`, which declares an input pin of the enclosing *network*
  /// and is meaningless in a body (issue #417) — are dropped from the list.
  /// Rust refuses those adds regardless; this just keeps a dead entry off the
  /// menu.
  final bool inZoneBody;

  const AddNodePopup({
    super.key,
    this.filterByCompatibleType,
    this.draggingFromOutput,
    this.inZoneBody = false,
  });

  @override
  State<AddNodePopup> createState() => _AddNodePopupState();
}

class _AddNodePopupState extends State<AddNodePopup> {
  final TextEditingController _filterController = TextEditingController();
  List<APINodeCategoryView> _allCategories = [];
  List<APINodeCategoryView> _filteredCategories = [];
  APINodeTypeView? _hoveredNode;

  @override
  void initState() {
    super.initState();
    List<APINodeCategoryView>? categories;
    if (widget.filterByCompatibleType != null) {
      categories = getCompatibleNodeTypes(
        sourceTypeStr: widget.filterByCompatibleType!,
        draggingFromOutput: widget.draggingFromOutput ?? true,
      );
    } else {
      categories = getNodeTypeViews();
    }
    if (categories != null) {
      _allCategories = widget.inZoneBody
          ? _dropNodesDisallowedInZoneBody(categories)
          : categories;
    }
    _filteredCategories = List.from(_allCategories);
    _filterController.addListener(_filterNodes);
  }

  /// Removes node types that may not be placed in a zone body, dropping any
  /// category left empty. The flag is computed in Rust
  /// (`node_type_registry::allowed_in_zone_body`) so the popup and the backend
  /// refusal can't drift apart.
  List<APINodeCategoryView> _dropNodesDisallowedInZoneBody(
      List<APINodeCategoryView> categories) {
    return categories
        .map((category) {
          final nodes =
              category.nodes.where((node) => node.allowedInZoneBody).toList();
          if (nodes.isEmpty) {
            return null;
          }
          return APINodeCategoryView(
            category: category.category,
            nodes: nodes,
          );
        })
        .whereType<APINodeCategoryView>()
        .toList();
  }

  void _filterNodes() {
    setState(() {
      String query = _filterController.text.toLowerCase();
      if (query.isEmpty) {
        // No filter: show all categories with all nodes
        _filteredCategories = List.from(_allCategories);
      } else {
        // Filter: show only categories that have matching nodes
        _filteredCategories = _allCategories
            .map((category) {
              final filteredNodes = category.nodes
                  .where((node) => node.name.toLowerCase().contains(query))
                  .toList();
              if (filteredNodes.isEmpty) {
                return null; // Skip categories with no matching nodes
              }
              return APINodeCategoryView(
                category: category.category,
                nodes: filteredNodes,
              );
            })
            .whereType<APINodeCategoryView>() // Remove nulls
            .toList();
      }
    });
  }

  void _selectNode(APINodeTypeView node) {
    Navigator.of(context)
        .pop(node.name); // Close popup and return the selected node name
  }

  // Build a flat list of items for the ListView: category headers + nodes
  List<dynamic> _buildListItems() {
    List<dynamic> items = [];
    for (var category in _filteredCategories) {
      items.add(category.category); // Add category as header
      items.addAll(category.nodes); // Add all nodes in this category
    }
    return items;
  }

  @override
  Widget build(BuildContext context) {
    return DraggableDialog(
      key: AddNodePopupKeys.dialog,
      backgroundColor: Colors.black,
      width: 560,
      child: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('Add Node',
                style: TextStyle(
                    color: Colors.white,
                    fontSize: 15,
                    fontWeight: FontWeight.bold)),
            SizedBox(height: 10),
            TextField(
              key: AddNodePopupKeys.filterField,
              controller: _filterController,
              decoration: InputDecoration(
                hintText: 'Filter node types...',
                hintStyle: TextStyle(color: Colors.white54),
                filled: true,
                fillColor: Colors.grey[900],
                border:
                    OutlineInputBorder(borderRadius: BorderRadius.circular(8)),
              ),
              style: TextStyle(color: Colors.white),
            ),
            SizedBox(height: 10),
            SizedBox(
              height: 320, // Limit height for scrollability
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  // Left panel: Node list
                  SizedBox(
                    width: 200,
                    child: Container(
                      decoration: BoxDecoration(
                        border: Border.all(color: Colors.grey[800]!),
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: ListView.builder(
                        key: AddNodePopupKeys.nodeListView,
                        itemCount: _buildListItems().length,
                        itemBuilder: (context, index) {
                          final items = _buildListItems();
                          final item = items[index];

                          // Check if item is a category header or a node
                          if (item is NodeTypeCategory) {
                            // Render category header
                            return Container(
                              key: AddNodePopupKeys.categoryHeader(item),
                              padding: EdgeInsets.fromLTRB(8, 12, 8, 4),
                              child: Text(
                                getCategoryDisplayName(item),
                                style: TextStyle(
                                  color: Colors.blue[300],
                                  fontSize: 12,
                                  fontWeight: FontWeight.bold,
                                  letterSpacing: 0.5,
                                ),
                              ),
                            );
                          } else if (item is APINodeTypeView) {
                            // Render node item
                            final nodeView = item;
                            return Builder(
                              builder: (itemContext) {
                                return MouseRegion(
                                  onEnter: (_) => setState(() {
                                    _hoveredNode = nodeView;
                                  }),
                                  onExit: (event) {
                                    // Get the render box to determine local position
                                    final RenderBox? box = itemContext
                                        .findRenderObject() as RenderBox?;
                                    if (box != null) {
                                      final localPosition =
                                          box.globalToLocal(event.position);
                                      final size = box.size;

                                      // Only clear hover if exiting to the left, top, or bottom
                                      // If exiting to the right (toward description panel), keep the hover
                                      if (localPosition.dx < size.width * 0.9) {
                                        setState(() {
                                          _hoveredNode = null;
                                        });
                                      }
                                    }
                                  },
                                  child: ListTile(
                                    key: AddNodePopupKeys.nodeItem(
                                        nodeView.name),
                                    contentPadding: EdgeInsets.symmetric(
                                        vertical: 0, horizontal: 8),
                                    dense: true,
                                    visualDensity: VisualDensity(vertical: -4),
                                    title: Text(nodeView.name,
                                        style: TextStyle(
                                            color: Colors.white,
                                            fontSize: 15,
                                            height: 1.0)),
                                    onTap: () => _selectNode(nodeView),
                                  ),
                                );
                              },
                            );
                          }
                          return SizedBox
                              .shrink(); // Fallback for unknown types
                        },
                      ),
                    ),
                  ),
                  SizedBox(width: 12),
                  // Right panel: Description
                  Expanded(
                    child: Container(
                      key: AddNodePopupKeys.descriptionPanel,
                      padding: EdgeInsets.all(12),
                      decoration: BoxDecoration(
                        color: Colors.grey[900],
                        borderRadius: BorderRadius.circular(4),
                        border: Border.all(color: Colors.grey[800]!),
                      ),
                      child: _hoveredNode != null
                          ? MouseRegion(
                              onExit: (_) => setState(() {
                                _hoveredNode = null;
                              }),
                              child: SelectionArea(
                                child: SingleChildScrollView(
                                  child: Column(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      Text(
                                        key: AddNodePopupKeys.descriptionTitle,
                                        _hoveredNode!.name,
                                        style: TextStyle(
                                          color: Colors.white,
                                          fontSize: 14,
                                          fontWeight: FontWeight.bold,
                                        ),
                                      ),
                                      if (_hoveredNode!
                                          .description.isNotEmpty) ...[
                                        SizedBox(height: 8),
                                        MarkdownBody(
                                          key: AddNodePopupKeys.descriptionText,
                                          data: _hoveredNode!.description,
                                          styleSheet:
                                              buildDescriptionMarkdownStyleSheet(
                                            context,
                                            baseFontSize: 13,
                                          ),
                                        ),
                                      ],
                                    ],
                                  ),
                                ),
                              ),
                            )
                          : Center(
                              child: Text(
                                'Hover over a node type\nto see its description',
                                textAlign: TextAlign.center,
                                style: TextStyle(
                                  color: Colors.white38,
                                  fontSize: 13,
                                ),
                              ),
                            ),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

// Function to show the modal popup
Future<String?> showAddNodePopup(
  BuildContext context, {
  String? filterByCompatibleType,
  bool? draggingFromOutput,
  bool inZoneBody = false,
}) {
  return showDialog<String>(
    context: context,
    barrierDismissible: true, // Close when tapping outside
    builder: (context) => AddNodePopup(
      filterByCompatibleType: filterByCompatibleType,
      draggingFromOutput: draggingFromOutput,
      inZoneBody: inZoneBody,
    ),
  );
}

/// One row of the [ArrangementDialog]: a label (see `arrangementRowLabels`),
/// the type printed beside it, and the pin dot's colour.
class ArrangementRow {
  final String label;
  final String typeLabel;
  final Color color;

  const ArrangementRow({
    required this.label,
    required this.typeLabel,
    required this.color,
  });
}

/// Asks which arrangement to splice with (`doc/design_insert_node_on_wire.md`
/// D3): a flat list of the valid ones, one click to commit. Returns the chosen
/// row's index, or null if cancelled.
Future<int?> showArrangementDialog(
  BuildContext context, {
  required String title,
  required List<ArrangementRow> rows,
}) {
  return showDialog<int>(
    context: context,
    barrierDismissible: false,
    builder: (context) => ArrangementDialog(title: title, rows: rows),
  );
}

/// The arrangement picker. It takes keyboard focus: Up/Down move the
/// highlight, Enter commits it, Esc cancels.
class ArrangementDialog extends StatefulWidget {
  final String title;
  final List<ArrangementRow> rows;

  const ArrangementDialog({
    super.key,
    required this.title,
    required this.rows,
  });

  @override
  State<ArrangementDialog> createState() => _ArrangementDialogState();
}

class _ArrangementDialogState extends State<ArrangementDialog> {
  final FocusNode _focusNode = FocusNode();
  int _highlight = 0;

  @override
  void dispose() {
    _focusNode.dispose();
    super.dispose();
  }

  KeyEventResult _handleKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    final key = event.logicalKey;
    if (key == LogicalKeyboardKey.arrowDown) {
      setState(() => _highlight = (_highlight + 1) % widget.rows.length);
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.arrowUp) {
      setState(() => _highlight =
          (_highlight - 1 + widget.rows.length) % widget.rows.length);
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter) {
      Navigator.of(context).pop(_highlight);
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.escape) {
      Navigator.of(context).pop();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    return Focus(
      focusNode: _focusNode,
      autofocus: true,
      onKeyEvent: _handleKey,
      child: DraggableDialog(
        backgroundColor: const Color(0xFF2D2D2D),
        width: 280,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(
              padding: const EdgeInsets.all(12),
              decoration: const BoxDecoration(
                color: Color(0xFF383838),
                borderRadius: BorderRadius.only(
                  topLeft: Radius.circular(8),
                  topRight: Radius.circular(8),
                ),
              ),
              child: Text(
                widget.title,
                style: const TextStyle(
                  color: Colors.white,
                  fontSize: 14,
                  fontWeight: FontWeight.w500,
                ),
              ),
            ),
            const SizedBox(height: 4),
            Flexible(
              child: ListView.builder(
                shrinkWrap: true,
                itemCount: widget.rows.length,
                itemBuilder: (context, index) => _ArrangementListItem(
                  row: widget.rows[index],
                  highlighted: index == _highlight,
                  onHover: () => setState(() => _highlight = index),
                  onTap: () => Navigator.of(context).pop(index),
                ),
              ),
            ),
            const SizedBox(height: 8),
          ],
        ),
      ),
    );
  }
}

class _ArrangementListItem extends StatelessWidget {
  final ArrangementRow row;
  final bool highlighted;
  final VoidCallback onHover;
  final VoidCallback onTap;

  const _ArrangementListItem({
    required this.row,
    required this.highlighted,
    required this.onHover,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      onEnter: (_) => onHover(),
      child: GestureDetector(
        onTap: onTap,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          color: highlighted ? const Color(0xFF404040) : Colors.transparent,
          child: Row(
            children: [
              Container(
                width: 10,
                height: 10,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: row.color,
                  border: Border.all(color: Colors.white54, width: 1),
                ),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Text(
                  row.label,
                  style: const TextStyle(
                    color: Colors.white,
                    fontSize: 13,
                  ),
                ),
              ),
              Text(
                row.typeLabel,
                style: const TextStyle(
                  color: Colors.white54,
                  fontSize: 11,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
