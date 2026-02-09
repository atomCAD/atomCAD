import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Text editor for viewing and editing node networks in text format.
class NetworkTextEditor extends StatefulWidget {
  final StructureDesignerModel graphModel;

  const NetworkTextEditor({
    super.key,
    required this.graphModel,
  });

  @override
  State<NetworkTextEditor> createState() => NetworkTextEditorState();
}

class NetworkTextEditorState extends State<NetworkTextEditor> {
  final TextEditingController _controller = TextEditingController();
  final FocusNode _focusNode = FocusNode();
  final ScrollController _editorScrollController = ScrollController();
  final ScrollController _gutterScrollController = ScrollController();

  bool _isDirty = false;
  APITextEditResult? _lastResult;
  Map<int, APITextError> _errorsByLine = {};
  List<_LineNodeMapping> _lineNodeMappings = [];

  bool get isDirty => _isDirty;

  @override
  void initState() {
    super.initState();
    _controller.addListener(_onTextChanged);
    // Sync scroll between gutter and editor
    _editorScrollController.addListener(() {
      if (_gutterScrollController.hasClients) {
        _gutterScrollController.jumpTo(_editorScrollController.offset);
      }
    });
  }

  @override
  void dispose() {
    _controller.removeListener(_onTextChanged);
    _controller.dispose();
    _focusNode.dispose();
    _editorScrollController.dispose();
    _gutterScrollController.dispose();
    super.dispose();
  }

  void _onTextChanged() {
    if (!_isDirty) {
      setState(() {
        _isDirty = true;
      });
    }
  }

  /// Load text from the current network state.
  void loadFromNetwork() {
    final text = sd_api.serializeActiveNetworkToText();
    _controller.removeListener(_onTextChanged);
    _controller.text = text;
    _controller.addListener(_onTextChanged);
    _buildLineNodeMappings(text);
    setState(() {
      _isDirty = false;
      _lastResult = null;
      _errorsByLine = {};
    });
  }

  /// Apply the current text to the network.
  void applyChanges() {
    final code = _controller.text;
    final result = sd_api.applyTextToActiveNetwork(code: code);
    widget.graphModel.refreshFromKernel();

    final errorMap = <int, APITextError>{};
    for (final error in result.errors) {
      if (error.line > 0) {
        errorMap[error.line] = error;
      }
    }

    setState(() {
      _lastResult = result;
      _errorsByLine = errorMap;
      if (result.success) {
        _isDirty = false;
        // Reload from network to get canonical form
        final text = sd_api.serializeActiveNetworkToText();
        _controller.removeListener(_onTextChanged);
        _controller.text = text;
        _controller.addListener(_onTextChanged);
        _buildLineNodeMappings(text);
      }
    });
  }

  /// Discard changes and reload from network.
  void discardChanges() {
    loadFromNetwork();
  }

  /// Build line→node name mappings from the text.
  void _buildLineNodeMappings(String text) {
    final mappings = <_LineNodeMapping>[];
    final lines = text.split('\n');
    for (int i = 0; i < lines.length; i++) {
      final line = lines[i].trim();
      // Match assignment pattern: name = type { ... }
      final eqIndex = line.indexOf('=');
      if (eqIndex > 0 &&
          !line.startsWith('#') &&
          !line.startsWith('output') &&
          !line.startsWith('delete') &&
          !line.startsWith('description') &&
          !line.startsWith('summary')) {
        final name = line.substring(0, eqIndex).trim();
        if (name.isNotEmpty && !name.contains(' ')) {
          mappings.add(_LineNodeMapping(lineNumber: i + 1, nodeName: name));
        }
      }
    }
    _lineNodeMappings = mappings;
  }

  /// Handle cursor position changes to sync active node.
  void _onCursorChanged() {
    if (!_focusNode.hasFocus) return;
    final selection = _controller.selection;
    if (!selection.isValid) return;

    // Compute current line number (1-indexed)
    final text = _controller.text;
    final offset = selection.baseOffset.clamp(0, text.length);
    int currentLine = 1;
    for (int i = 0; i < offset; i++) {
      if (text[i] == '\n') currentLine++;
    }

    // Find node on this line
    for (final mapping in _lineNodeMappings) {
      if (mapping.lineNumber == currentLine) {
        _selectNodeByName(mapping.nodeName);
        return;
      }
    }
  }

  /// Select a node by its custom name.
  void _selectNodeByName(String nodeName) {
    final view = widget.graphModel.nodeNetworkView;
    if (view == null) return;

    for (final entry in view.nodes.entries) {
      final node = entry.value;
      // Match by subtitle which contains custom_name, or by node type + name pattern
      // The NodeView doesn't directly expose custom_name, but we can look at subtitle
      // Actually, let's use the API to find the node by looking at all nodes
      if (node.subtitle == nodeName || _nodeMatchesName(node, nodeName)) {
        widget.graphModel.setSelectedNode(entry.key);
        return;
      }
    }
  }

  /// Check if a node matches a given custom name.
  bool _nodeMatchesName(NodeView node, String name) {
    // The subtitle field contains the custom_name when it differs from auto-generated
    if (node.subtitle != null && node.subtitle == name) return true;
    // For nodes where custom_name matches the auto-generated pattern,
    // subtitle may be null but the name still matches
    // We check if name follows the pattern: nodeTypeName + number
    final typeName = node.nodeTypeName;
    if (name.startsWith(typeName)) {
      final suffix = name.substring(typeName.length);
      if (suffix.isNotEmpty && int.tryParse(suffix) != null) {
        return true;
      }
    }
    return false;
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      color: const Color(0xFF1E1E1E), // Dark editor background
      child: Column(
        children: [
          // Toolbar
          _buildToolbar(),
          // Editor area
          Expanded(
            child: _buildEditorArea(),
          ),
          // Status bar
          _buildStatusBar(),
        ],
      ),
    );
  }

  Widget _buildToolbar() {
    return Container(
      height: 28,
      padding: const EdgeInsets.symmetric(horizontal: 4),
      color: const Color(0xFF2D2D2D),
      child: Row(
        children: [
          // Apply button
          SizedBox(
            height: 22,
            child: TextButton.icon(
              onPressed: _isDirty ? applyChanges : null,
              icon: const Icon(Icons.check, size: 14),
              label: const Text('Apply', style: TextStyle(fontSize: 11)),
              style: TextButton.styleFrom(
                padding: const EdgeInsets.symmetric(horizontal: 6),
                foregroundColor:
                    _isDirty ? Colors.greenAccent : Colors.grey[600],
              ),
            ),
          ),
          const SizedBox(width: 4),
          // Keyboard shortcut hint
          if (_isDirty)
            Text(
              'Ctrl+Enter',
              style: TextStyle(
                fontSize: 10,
                color: Colors.grey[600],
                fontStyle: FontStyle.italic,
              ),
            ),
          const Spacer(),
          // Modified indicator
          if (_isDirty)
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
              decoration: BoxDecoration(
                color: Colors.orange.withValues(alpha: 0.2),
                borderRadius: BorderRadius.circular(3),
              ),
              child: const Text(
                'Modified',
                style: TextStyle(
                  fontSize: 10,
                  color: Colors.orange,
                ),
              ),
            ),
        ],
      ),
    );
  }

  Widget _buildEditorArea() {
    final lines = _controller.text.split('\n');
    final lineCount = lines.isEmpty ? 1 : lines.length;

    return KeyboardListener(
      focusNode: FocusNode(),
      onKeyEvent: (event) {
        // Ctrl+Enter to apply
        if (event is KeyDownEvent &&
            event.logicalKey == LogicalKeyboardKey.enter &&
            HardwareKeyboard.instance.isControlPressed &&
            _isDirty) {
          applyChanges();
        }
      },
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Line number gutter
          SizedBox(
            width: 44,
            child: ScrollConfiguration(
              behavior:
                  ScrollConfiguration.of(context).copyWith(scrollbars: false),
              child: ListView.builder(
                controller: _gutterScrollController,
                itemCount: lineCount,
                itemExtent: 20.0, // Match line height
                physics: const NeverScrollableScrollPhysics(),
                itemBuilder: (context, index) {
                  final lineNum = index + 1;
                  final hasError = _errorsByLine.containsKey(lineNum);
                  return SizedBox(
                    height: 20,
                    child: Row(
                      children: [
                        // Error indicator
                        SizedBox(
                          width: 14,
                          child: hasError
                              ? Tooltip(
                                  message: _errorsByLine[lineNum]!.message,
                                  child: const Icon(
                                    Icons.error,
                                    size: 12,
                                    color: Colors.redAccent,
                                  ),
                                )
                              : null,
                        ),
                        // Line number
                        Expanded(
                          child: Text(
                            '$lineNum',
                            textAlign: TextAlign.right,
                            style: TextStyle(
                              fontSize: 12,
                              fontFamily: 'monospace',
                              color: hasError
                                  ? Colors.redAccent
                                  : Colors.grey[600],
                              height: 20.0 / 12.0,
                            ),
                          ),
                        ),
                        const SizedBox(width: 4),
                      ],
                    ),
                  );
                },
              ),
            ),
          ),
          // Vertical divider
          Container(
            width: 1,
            color: Colors.grey[700],
          ),
          // Text editor
          Expanded(
            child: TextField(
              controller: _controller,
              focusNode: _focusNode,
              scrollController: _editorScrollController,
              maxLines: null,
              expands: true,
              textAlignVertical: TextAlignVertical.top,
              onTap: _onCursorChanged,
              onChanged: (_) {
                // Rebuild line count for gutter
                setState(() {});
                _onCursorChanged();
              },
              style: const TextStyle(
                fontSize: 12,
                fontFamily: 'monospace',
                color: Color(0xFFD4D4D4),
                height: 20.0 / 12.0,
              ),
              decoration: const InputDecoration(
                border: InputBorder.none,
                contentPadding:
                    EdgeInsets.symmetric(horizontal: 8, vertical: 0),
                isDense: true,
              ),
              cursorColor: Colors.white,
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildStatusBar() {
    String statusText;
    Color statusColor;

    if (_lastResult != null) {
      final r = _lastResult!;
      if (r.success) {
        final parts = <String>[];
        if (r.nodesCreated.isNotEmpty) {
          parts.add('${r.nodesCreated.length} created');
        }
        if (r.nodesUpdated.isNotEmpty) {
          parts.add('${r.nodesUpdated.length} updated');
        }
        if (r.nodesDeleted.isNotEmpty) {
          parts.add('${r.nodesDeleted.length} deleted');
        }
        if (r.warnings.isNotEmpty) {
          statusText = 'Applied with ${r.warnings.length} warning(s)';
          statusColor = Colors.orange;
        } else if (parts.isEmpty) {
          statusText = 'Applied (no changes)';
          statusColor = Colors.grey;
        } else {
          statusText = 'Applied: ${parts.join(', ')}';
          statusColor = Colors.greenAccent;
        }
      } else {
        if (r.errors.isNotEmpty) {
          final first = r.errors.first;
          if (first.line > 0) {
            statusText = 'Line ${first.line}: ${first.message}';
          } else {
            statusText = first.message;
          }
        } else {
          statusText = 'Error';
        }
        statusColor = Colors.redAccent;
      }
    } else if (_isDirty) {
      statusText = 'Modified - press Ctrl+Enter to apply';
      statusColor = Colors.grey;
    } else {
      statusText = 'Ready';
      statusColor = Colors.grey;
    }

    final errorCount = _lastResult != null ? _lastResult!.errors.length : 0;
    final warningCount = _lastResult != null ? _lastResult!.warnings.length : 0;

    return Container(
      height: 22,
      padding: const EdgeInsets.symmetric(horizontal: 8),
      color: const Color(0xFF007ACC),
      child: Row(
        children: [
          if (errorCount > 0) ...[
            const Icon(Icons.error, size: 12, color: Colors.white),
            const SizedBox(width: 2),
            Text(
              '$errorCount',
              style: const TextStyle(fontSize: 11, color: Colors.white),
            ),
            const SizedBox(width: 8),
          ],
          if (warningCount > 0) ...[
            const Icon(Icons.warning, size: 12, color: Colors.yellow),
            const SizedBox(width: 2),
            Text(
              '$warningCount',
              style: const TextStyle(fontSize: 11, color: Colors.white),
            ),
            const SizedBox(width: 8),
          ],
          Expanded(
            child: Text(
              statusText,
              style: TextStyle(
                fontSize: 11,
                color:
                    statusColor == Colors.grey ? Colors.white70 : statusColor,
              ),
              overflow: TextOverflow.ellipsis,
            ),
          ),
        ],
      ),
    );
  }
}

/// Maps a line number to a node name for cursor→selection sync.
class _LineNodeMapping {
  final int lineNumber;
  final String nodeName;

  const _LineNodeMapping({required this.lineNumber, required this.nodeName});
}
