import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:provider/provider.dart';
import 'structure_designer_model.dart';

/// Bottom-docked Console panel showing entries pushed by `print` nodes.
///
/// The panel is collapsed (zero height) when `model.consolePanelVisible` is
/// false. When visible, it occupies a fixed height at the bottom of the
/// structure-designer view. New entries arrive via `model.printLog`, which is
/// refreshed automatically inside `refreshFromKernel` (drain-on-read).
///
/// See `doc/design_node_execution.md` (Phase 4 — Console panel).
class ConsolePanel extends StatefulWidget {
  const ConsolePanel({super.key});

  @override
  State<ConsolePanel> createState() => _ConsolePanelState();
}

class _ConsolePanelState extends State<ConsolePanel> {
  static const double _panelHeight = 220;
  final ScrollController _scrollController = ScrollController();
  bool _autoScroll = true;
  int _lastSeenLength = 0;

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  void _maybeAutoScroll(int currentLength) {
    if (!_autoScroll) return;
    if (currentLength == _lastSeenLength) return;
    _lastSeenLength = currentLength;
    // Schedule the scroll for after the build commits — list extent is not
    // known until then. `hasClients` guards against a panel that just toggled
    // off (the controller may have detached during the same frame).
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_scrollController.hasClients) return;
      _scrollController.jumpTo(_scrollController.position.maxScrollExtent);
    });
  }

  String _formatTimestamp(int epochMillis) {
    final dt = DateTime.fromMillisecondsSinceEpoch(epochMillis);
    final h = dt.hour.toString().padLeft(2, '0');
    final m = dt.minute.toString().padLeft(2, '0');
    final s = dt.second.toString().padLeft(2, '0');
    return '$h:$m:$s';
  }

  @override
  Widget build(BuildContext context) {
    return Consumer<StructureDesignerModel>(
      builder: (context, model, _) {
        if (!model.consolePanelVisible) {
          return const SizedBox.shrink();
        }
        _maybeAutoScroll(model.printLog.length);
        return Container(
          height: _panelHeight,
          decoration: const BoxDecoration(
            color: Color(0xFF1E1E1E),
            border: Border(top: BorderSide(color: Colors.black54, width: 1)),
          ),
          child: Column(
            children: [
              _buildHeader(context, model),
              Expanded(child: _buildBody(model)),
            ],
          ),
        );
      },
    );
  }

  Widget _buildHeader(BuildContext context, StructureDesignerModel model) {
    return Container(
      height: 28,
      padding: const EdgeInsets.symmetric(horizontal: 8),
      color: const Color(0xFF2A2A2A),
      child: Row(
        children: [
          const Text(
            'Console',
            style: TextStyle(
              color: Colors.white70,
              fontWeight: FontWeight.w600,
              fontSize: 12,
            ),
          ),
          const SizedBox(width: 12),
          Text(
            '${model.printLog.length} entr${model.printLog.length == 1 ? "y" : "ies"}',
            style: const TextStyle(color: Colors.white38, fontSize: 11),
          ),
          const Spacer(),
          // Autoscroll toggle
          InkWell(
            onTap: () => setState(() => _autoScroll = !_autoScroll),
            child: Tooltip(
              message: _autoScroll ? 'Autoscroll: on' : 'Autoscroll: off',
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                child: Icon(
                  _autoScroll
                      ? Icons.vertical_align_bottom
                      : Icons.pause_circle_outline,
                  size: 16,
                  color: _autoScroll ? Colors.white70 : Colors.white38,
                ),
              ),
            ),
          ),
          // Clear button
          InkWell(
            onTap: () => model.clearPrintLog(),
            child: const Tooltip(
              message: 'Clear console',
              child: Padding(
                padding: EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                child:
                    Icon(Icons.delete_outline, size: 16, color: Colors.white70),
              ),
            ),
          ),
          // Close button
          InkWell(
            onTap: () => model.toggleConsolePanel(),
            child: const Tooltip(
              message: 'Hide console',
              child: Padding(
                padding: EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                child: Icon(Icons.close, size: 16, color: Colors.white70),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildBody(StructureDesignerModel model) {
    if (model.printLog.isEmpty) {
      return const Center(
        child: Text(
          'No entries yet. Insert a `print` node to surface text here.',
          style: TextStyle(color: Colors.white38, fontSize: 12),
        ),
      );
    }
    return Scrollbar(
      controller: _scrollController,
      child: ListView.builder(
        controller: _scrollController,
        padding: const EdgeInsets.symmetric(vertical: 4, horizontal: 8),
        itemCount: model.printLog.length,
        itemBuilder: (context, index) => _buildEntry(model.printLog[index]),
      ),
    );
  }

  Widget _buildEntry(APIPrintLogEntry entry) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 1),
      child: DefaultTextStyle.merge(
        style: const TextStyle(
          fontFamily: 'monospace',
          fontSize: 12,
          color: Colors.white70,
          height: 1.3,
        ),
        child: SelectableText.rich(
          TextSpan(children: [
            TextSpan(
              text: '[${_formatTimestamp(entry.timestampMs)}] ',
              style: const TextStyle(color: Colors.white38),
            ),
            if (entry.fromExecute)
              const TextSpan(
                text: '▶ ',
                style: TextStyle(color: Color(0xFFE08000)),
              ),
            TextSpan(
              text: '${entry.networkName} / ${entry.nodeLabel}  ',
              style: const TextStyle(color: Color(0xFF6CA0DC)),
            ),
            TextSpan(text: entry.text),
          ]),
        ),
      ),
    );
  }
}

/// Toolbar toggle button (with new-entries dot) for the Console panel. Drop
/// into any toolbar / menu bar; tapping flips `model.consolePanelVisible`.
class ConsoleToggleButton extends StatelessWidget {
  const ConsoleToggleButton({super.key});

  @override
  Widget build(BuildContext context) {
    return Consumer<StructureDesignerModel>(
      builder: (context, model, _) {
        final hasUnread = model.unreadPrintLogCount > 0;
        return Tooltip(
          message: model.consolePanelVisible
              ? 'Hide Console (Ctrl+`)'
              : 'Show Console (Ctrl+`)',
          child: InkWell(
            onTap: () => model.toggleConsolePanel(),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(
                    Icons.terminal,
                    size: 16,
                    color: model.consolePanelVisible
                        ? Colors.black
                        : Colors.black87,
                  ),
                  const SizedBox(width: 4),
                  const Text('Console', style: TextStyle(fontSize: 12)),
                  if (hasUnread) ...[
                    const SizedBox(width: 4),
                    Container(
                      width: 8,
                      height: 8,
                      decoration: const BoxDecoration(
                        color: Color(0xFFE08000),
                        shape: BoxShape.circle,
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}
