import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/network_text_editor.dart';

/// Tab container that switches between the visual Graph editor and the Text editor.
class NetworkEditorTabs extends StatefulWidget {
  final StructureDesignerModel graphModel;
  final GlobalKey nodeNetworkKey;

  const NetworkEditorTabs({
    super.key,
    required this.graphModel,
    required this.nodeNetworkKey,
  });

  @override
  State<NetworkEditorTabs> createState() => _NetworkEditorTabsState();
}

class _NetworkEditorTabsState extends State<NetworkEditorTabs>
    with SingleTickerProviderStateMixin {
  late TabController _tabController;
  final GlobalKey<NetworkTextEditorState> _textEditorKey =
      GlobalKey<NetworkTextEditorState>();
  int _currentIndex = 0;
  bool _handlingDirtySwitch = false;

  @override
  void initState() {
    super.initState();
    _tabController = TabController(length: 2, vsync: this);
    _tabController.addListener(_onTabChanged);
  }

  @override
  void dispose() {
    _tabController.removeListener(_onTabChanged);
    _tabController.dispose();
    super.dispose();
  }

  void _onTabChanged() {
    // Skip animation-in-progress notifications; only act on final value
    if (_tabController.indexIsChanging) return;
    if (_handlingDirtySwitch) return;

    final newIndex = _tabController.index;
    if (newIndex == _currentIndex) return;

    final leavingTextTab = _currentIndex == 1;
    final enteringTextTab = newIndex == 1;

    if (leavingTextTab) {
      final textEditor = _textEditorKey.currentState;
      if (textEditor != null && textEditor.isDirty) {
        // Snap back to text tab and show confirmation
        _handlingDirtySwitch = true;
        _tabController.index = 1;
        _handlingDirtySwitch = false;
        _showUnsavedChangesDialog();
        return;
      }
    }

    setState(() {
      _currentIndex = newIndex;
    });

    if (enteringTextTab) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _textEditorKey.currentState?.loadFromNetwork();
      });
    }
  }

  Future<void> _showUnsavedChangesDialog() async {
    final result = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Unsaved Text Changes'),
        content: const Text(
          'You have unapplied changes in the text editor. What would you like to do?',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop('cancel'),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.of(context).pop('discard'),
            child: const Text('Discard'),
          ),
          TextButton(
            onPressed: () => Navigator.of(context).pop('apply'),
            child: const Text('Apply'),
          ),
        ],
      ),
    );

    if (!mounted) return;

    if (result == 'apply') {
      _textEditorKey.currentState?.applyChanges();
      _switchToTab(0);
    } else if (result == 'discard') {
      _textEditorKey.currentState?.discardChanges();
      _switchToTab(0);
    }
    // 'cancel' or null: stay on text tab
  }

  void _switchToTab(int index) {
    _handlingDirtySwitch = true;
    _tabController.animateTo(index);
    _handlingDirtySwitch = false;
    setState(() {
      _currentIndex = index;
    });
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        // Compact tab bar
        SizedBox(
          height: 28,
          child: TabBar(
            controller: _tabController,
            tabs: const [
              Tab(
                key: Key('graph_tab'),
                icon: Icon(Icons.schema, size: 14),
                iconMargin: EdgeInsets.zero,
                height: 28,
              ),
              Tab(
                key: Key('text_tab'),
                icon: Icon(Icons.code, size: 14),
                iconMargin: EdgeInsets.zero,
                height: 28,
              ),
            ],
            labelPadding: EdgeInsets.zero,
            indicatorSize: TabBarIndicatorSize.tab,
          ),
        ),
        // Tab content - use IndexedStack to keep both alive
        Expanded(
          child: IndexedStack(
            index: _currentIndex,
            children: [
              // Graph tab
              NodeNetwork(
                key: widget.nodeNetworkKey,
                graphModel: widget.graphModel,
              ),
              // Text tab
              NetworkTextEditor(
                key: _textEditorKey,
                graphModel: widget.graphModel,
              ),
            ],
          ),
        ),
      ],
    );
  }
}
