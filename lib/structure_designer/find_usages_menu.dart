import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Shared Find Usages presentation (issue #414, `doc/design_find_usages.md`).
///
/// Both entry points — the node context menu (Phase 2) and the user-types panel
/// (Phase 3) — end in the same 0/1/n branching and the same picker, so that
/// part lives here. What differs stays at the call site: the node-level entry
/// filters out the clicked instance and words its empty case accordingly (D3,
/// D5), while the panel queries the raw set.

/// One picker row: `host network — node label (in map1 body)`. Every part is
/// resolved Rust-side (design D2), including the body qualifier, so nothing is
/// re-derived here.
String networkUsageLabel(APINetworkUsage usage) {
  final base = '${usage.hostNetwork} — ${usage.nodeLabel}';
  final qualifier = usage.bodyQualifier;
  return qualifier == null ? base : '$base ($qualifier)';
}

/// Presents [usages] and jumps to the picked one (D5).
///
/// None → [emptyMessage] in a SnackBar; exactly one → jump immediately (no
/// picker — that third click is the friction the issue is about); several → a
/// picker anchored at [position].
///
/// [screenAnchor] is threaded unchanged into every jump, so going through the
/// picker lands just as continuously as the single-usage case. Callers with no
/// meaningful source position (the panel) omit it and get a viewport-centered
/// landing.
Future<void> showNetworkUsagesMenu({
  required BuildContext context,
  required StructureDesignerModel model,
  required String networkName,
  required List<APINetworkUsage> usages,
  required RelativeRect position,
  required String emptyMessage,
  Offset? screenAnchor,
}) async {
  final messenger = ScaffoldMessenger.maybeOf(context);

  if (usages.isEmpty) {
    messenger?.showSnackBar(SnackBar(content: Text(emptyMessage)));
    return;
  }

  if (usages.length == 1) {
    model.jumpToUsage(usages.first, screenAnchor: screenAnchor);
    return;
  }

  final picked = await showMenu<int>(
    context: context,
    position: position,
    items: <PopupMenuEntry<int>>[
      PopupMenuItem<int>(
        enabled: false,
        child: Text("Usages of '${getSimpleName(networkName)}'"),
      ),
      for (int i = 0; i < usages.length; i++)
        PopupMenuItem<int>(
          value: i,
          // Qualified network names get long; cap the row so a deep namespace
          // can't push the menu off screen.
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 420),
            child: Text(networkUsageLabel(usages[i]),
                overflow: TextOverflow.ellipsis),
          ),
        ),
    ],
  );
  if (picked == null) return;
  model.jumpToUsage(usages[picked], screenAnchor: screenAnchor);
}

/// Network-level Find Usages (Phase 3): the user-types panel entry points.
///
/// No self-exclusion here — there is no originating instance, so the count
/// really is zero when nothing comes back, and the wording says so (D5). The
/// landing is viewport-centered: a panel row is not a node position worth
/// anchoring to.
Future<void> findUsagesOfNetwork({
  required BuildContext context,
  required StructureDesignerModel model,
  required String networkName,
  required RelativeRect position,
}) {
  return showNetworkUsagesMenu(
    context: context,
    model: model,
    networkName: networkName,
    usages: sd_api.getNetworkUsages(networkName: networkName),
    position: position,
    emptyMessage: "'${getSimpleName(networkName)}' is not used by any network",
  );
}

/// Anchors a menu at the widget owning [context] — used for a panel row and
/// for the trailing usage count inside it.
RelativeRect menuPositionForWidget(BuildContext context) {
  final RenderBox box = context.findRenderObject() as RenderBox;
  final Offset offset = box.localToGlobal(Offset.zero);
  final Size size = box.size;
  final Size screenSize = MediaQuery.of(context).size;
  return RelativeRect.fromLTRB(
    offset.dx,
    offset.dy,
    screenSize.width - (offset.dx + size.width),
    screenSize.height - (offset.dy + size.height),
  );
}
