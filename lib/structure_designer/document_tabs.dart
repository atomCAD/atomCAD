/// The document tabs (`doc/design_multiple_documents.md` §3, D10).
///
/// One gesture model, [DocumentTabs], and two thin layouts over it:
///
/// - [DocumentTabList] — vertical, one document per row, docked to the
///   viewport's left edge (the default placement);
/// - [DocumentTabStrip] — horizontal, like a browser's, across the top of
///   the main content area (above the viewport).
///
/// Both offer the same gestures: click to activate, the close button or a
/// middle click to close, drag to reorder, and a tooltip with the full path.
/// Which one is shown is the *Interface* preference
/// (`InterfacePreferences.documentTabPlacement`).
///
/// [DocumentTabs] takes the tab list and its actions as plain data and
/// callbacks and never calls the API, so its rules — the reorder arithmetic,
/// the pointer gate — are unit-tested without the Rust library
/// (`test/document_tabs_test.dart`).
library;

import 'package:flutter/gestures.dart' show kMiddleMouseButton;
import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// The label of a tab: its file name, with a `*` while dirty.
String documentTabLabel(APIDocumentTab tab) =>
    tab.isDirty ? '${tab.displayName}*' : tab.displayName;

/// The tooltip of a tab: the full path, or a note that it was never saved.
String documentTabTooltip(APIDocumentTab tab) =>
    tab.filePath ?? '${tab.displayName} (not saved yet)';

/// The documents the quit dialog lists: exactly the dirty ones, in tab order,
/// by file name (*Untitled* for a design that was never saved).
List<String> dirtyDocumentNames(List<APIDocumentTab> tabs) => [
      for (final tab in tabs)
        if (tab.isDirty) tab.displayName
    ];

/// The gestures of the tab list, independent of how it is laid out.
///
/// **The pointer gate.** A switch during a drag on the canvas or in the
/// viewport is refused by Rust (D4), but a tab click must not even try it
/// (D4: "Flutter disables the tabs during drags as well"). The gate is taken
/// when the pointer goes **down** on a tab ([pointerDown]): by the time the
/// tap is recognised, the tab's own pointer is itself counted as down, so a
/// check at that point could not tell "another drag is in progress" from
/// "this is the click".
class DocumentTabs {
  DocumentTabs({
    required this.tabs,
    required this.onActivate,
    required this.onClose,
    required this.onMove,
    required this.pointerBusy,
  });

  /// The open documents in tab order. Updated in place by the host on every
  /// build: the host keeps **one** [DocumentTabs] for its lifetime, because
  /// the gate armed at pointer down must still be there at the tap even when
  /// the model rebuilt the tabs in between.
  List<APIDocumentTab> tabs;

  /// Activates a parked tab.
  final void Function(APIDocumentTab tab) onActivate;

  /// Closes a tab (the host asks to discard unsaved changes first).
  final void Function(APIDocumentTab tab) onClose;

  /// Moves tab `id` to `index` of the tab order, counted *after* removing it
  /// from its old place (`DocumentSet::move_to`).
  final void Function(BigInt id, int index) onMove;

  /// True while some other pointer is down (a drag in the viewport, on the
  /// canvas, on a divider).
  final bool Function() pointerBusy;

  /// Whether the gesture that started with the last [pointerDown] may act.
  bool _armed = false;

  /// A pointer went down on a tab. [button] is the pointer's button mask: a
  /// middle click closes the tab straight away.
  void pointerDown(APIDocumentTab tab, {int button = 0}) {
    _armed = !pointerBusy();
    if (_armed && button == kMiddleMouseButton) {
      _armed = false;
      onClose(tab);
    }
  }

  /// A tap on a tab (after [pointerDown]). Activating the active tab is a
  /// no-op.
  void tap(APIDocumentTab tab) {
    if (!_armed) return;
    _armed = false;
    if (!tab.isActive) onActivate(tab);
  }

  /// The tab's close button (after [pointerDown]).
  void close(APIDocumentTab tab) {
    if (!_armed) return;
    _armed = false;
    onClose(tab);
  }

  /// A finished drag, in [ReorderableListView.onReorder] terms: [newIndex]
  /// is counted *before* the dragged tab is removed, so dropping it past the
  /// end gives `tabs.length`. Dropping a tab onto its own place moves
  /// nothing. Reordering never changes the active document, so it is not
  /// gated.
  void reorder(int oldIndex, int newIndex) {
    if (oldIndex < 0 || oldIndex >= tabs.length) return;
    final target = (newIndex > oldIndex ? newIndex - 1 : newIndex)
        .clamp(0, tabs.length - 1);
    if (target == oldIndex) return;
    onMove(tabs[oldIndex].id, target);
  }
}

const double _TAB_HEIGHT = 28;
const double _STRIP_TAB_MAX_WIDTH = 220;

/// One tab: label, close button, tooltip, and the gesture plumbing shared by
/// both layouts.
class _DocumentTabTile extends StatelessWidget {
  const _DocumentTabTile({
    required this.tab,
    required this.gestures,
    required this.expandLabel,
  });

  final APIDocumentTab tab;
  final DocumentTabs gestures;

  /// True in the vertical list (the label takes the row's width); false in
  /// the strip (the tab is as wide as its label, up to a maximum).
  final bool expandLabel;

  @override
  Widget build(BuildContext context) {
    final active = tab.isActive;
    final label = Text(
      documentTabLabel(tab),
      overflow: TextOverflow.ellipsis,
      softWrap: false,
      style: AppTextStyles.small.copyWith(
        fontWeight: active ? FontWeight.w600 : FontWeight.normal,
        color: active ? AppColors.textPrimary : AppColors.textSecondary,
      ),
    );
    // No Listener of its own: the tab's Listener below already sees the
    // pointer go down on the button (a second one would close twice on a
    // middle click).
    final closeButton = InkWell(
      key: ValueKey('document_tab_close_${tab.id}'),
      borderRadius: BorderRadius.circular(8),
      onTap: () => gestures.close(tab),
      child: const Padding(
        padding: EdgeInsets.all(2),
        child: Icon(Icons.close, size: 14),
      ),
    );
    return Tooltip(
      message: documentTabTooltip(tab),
      waitDuration: const Duration(milliseconds: 600),
      child: Listener(
        onPointerDown: (e) => gestures.pointerDown(tab, button: e.buttons),
        child: Material(
          color: active ? Colors.white : Colors.grey.shade200,
          child: InkWell(
            key: ValueKey('document_tab_${tab.id}'),
            onTap: () => gestures.tap(tab),
            child: Container(
              height: _TAB_HEIGHT,
              constraints: expandLabel
                  ? null
                  : const BoxConstraints(maxWidth: _STRIP_TAB_MAX_WIDTH),
              padding: const EdgeInsets.only(left: 8, right: 4),
              decoration: BoxDecoration(
                border: Border(
                  left: expandLabel && active
                      ? BorderSide(color: AppColors.primaryAccent!, width: 3)
                      : BorderSide.none,
                  top: !expandLabel && active
                      ? BorderSide(color: AppColors.primaryAccent!, width: 2)
                      : BorderSide.none,
                  right: expandLabel
                      ? BorderSide.none
                      : const BorderSide(color: Colors.black12),
                  bottom: expandLabel
                      ? const BorderSide(color: Colors.black12)
                      : BorderSide.none,
                ),
              ),
              child: Row(
                mainAxisSize: expandLabel ? MainAxisSize.max : MainAxisSize.min,
                children: [
                  if (expandLabel)
                    Expanded(child: label)
                  else
                    Flexible(child: label),
                  const SizedBox(width: 4),
                  closeButton,
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// The vertical placement: a list docked to the viewport's left edge, as tall
/// as the viewport. Its width divider belongs to the host (view state, like
/// the other dividers).
class DocumentTabList extends StatelessWidget {
  const DocumentTabList({super.key, required this.gestures, this.width = 180});

  final DocumentTabs gestures;
  final double width;

  @override
  Widget build(BuildContext context) {
    final tabs = gestures.tabs;
    return Container(
      key: const Key('document_tab_list'),
      width: width,
      color: Colors.grey.shade100,
      child: ReorderableListView.builder(
        buildDefaultDragHandles: false,
        itemCount: tabs.length,
        onReorder: gestures.reorder,
        itemBuilder: (context, i) => ReorderableDelayedDragStartListener(
          key: ValueKey(tabs[i].id),
          index: i,
          child: _DocumentTabTile(
              tab: tabs[i], gestures: gestures, expandLabel: true),
        ),
      ),
    );
  }
}

/// The horizontal placement: a strip of tabs, like a browser's.
class DocumentTabStrip extends StatelessWidget {
  const DocumentTabStrip({super.key, required this.gestures});

  final DocumentTabs gestures;

  @override
  Widget build(BuildContext context) {
    final tabs = gestures.tabs;
    return Container(
      key: const Key('document_tab_strip'),
      height: _TAB_HEIGHT,
      decoration: BoxDecoration(
        color: Colors.grey.shade300,
        border: const Border(bottom: BorderSide(color: Colors.black12)),
      ),
      child: ReorderableListView.builder(
        scrollDirection: Axis.horizontal,
        buildDefaultDragHandles: false,
        itemCount: tabs.length,
        onReorder: gestures.reorder,
        itemBuilder: (context, i) => ReorderableDelayedDragStartListener(
          key: ValueKey(tabs[i].id),
          index: i,
          child: _DocumentTabTile(
              tab: tabs[i], gestures: gestures, expandLabel: false),
        ),
      ),
    );
  }
}
