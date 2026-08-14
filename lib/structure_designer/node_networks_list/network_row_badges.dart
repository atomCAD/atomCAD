import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/structure_designer/error_report.dart';
import 'package:flutter_cad/structure_designer/find_usages_menu.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';
import 'package:flutter_cad/structure_designer/root_cause_navigation.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Shared trailing badges for a network row in the user-types panel — the
/// error badge and the Find Usages count — so the flat list view and the tree
/// view render them identically and can't drift apart.
///
/// The error badge consumes the network's **unified error list** (validation
/// + evaluation entries, `doc/design_error_management.md` D1): the color
/// channel encodes severity (red = something doesn't evaluate, amber =
/// advisory) and the icon encodes the source — filled circle / warning
/// triangle for validation, a bolt for evaluation (dimmed/outlined when the
/// entry is a stale snapshot of an inactive network, D6).
///
/// Both are small clickable pills anchored via [menuPositionForWidget]:
/// - The **error badge** (this file) opens a picker of the network's errors,
///   each of which navigates to the offending node — the same "count → click →
///   go there" grammar Find Usages gave instance usages, applied to errors.
/// - The **usage count** delegates to [findUsagesOfNetwork] unchanged.

// The root-cause grouping (`ErrorGroup`, `groupErrorsByRootCause`) and the
// navigability predicate (`isNavigableError`) live in `error_report.dart`, so
// this picker and the copied bug report can never disagree about what counts
// as one problem.

/// How many lines of message text one picker row shows before ellipsizing.
const int _ERROR_ROW_MAX_LINES = 8;

/// What the user picked in the error menu: a row (jump to its node) or a row's
/// "go to root cause" action.
class _ErrorPick {
  const _ErrorPick(this.error, {this.goToRoot = false});
  final APIValidationError error;
  final bool goToRoot;
}

/// The severity color for a set of errors: red when any error blocks the whole
/// network from evaluating, amber when they are all non-blocking warnings.
Color _severityColor(List<APIValidationError> errors) {
  final blocking = errors.any((e) => e.blocking);
  return blocking ? Colors.red.shade600 : Colors.orange.shade700;
}

/// The severity icon, matching [_severityColor]: a filled error mark for a
/// blocking error, a warning triangle for a warning-only network.
IconData _severityIcon(List<APIValidationError> errors) {
  final blocking = errors.any((e) => e.blocking);
  return blocking ? Icons.error : Icons.warning_amber_rounded;
}

/// The validation-error badge: a small colored pill showing the error count and
/// a severity icon (red = blocking, amber = warning-only). Hover shows the full
/// error text; clicking opens the navigable error picker. Returns a zero-size
/// widget when [errors] is empty, so a valid network reserves no width.
///
/// This replaces the old whole-row red border + tooltip: the border was a
/// binary signal that colored the row but couldn't be acted on, whereas the
/// badge carries the count, the severity, and the jump.
Widget buildNetworkErrorBadge({
  required BuildContext context,
  required StructureDesignerModel model,
  required String networkName,
  required List<APIValidationError> errors,
}) {
  if (errors.isEmpty) return const SizedBox.shrink();
  final color = _severityColor(errors);
  final icon = _severityIcon(errors);
  // The badge counts **problems**, not symptoms: one entry per root cause, with
  // its downstream cone collapsed behind it (D7). Otherwise a single failure
  // three nodes deep would read as "3 errors" while the picker showed one row.
  final groups = groupErrorsByRootCause(networkName, errors);
  // The full list on hover — progressive disclosure: glance (badge) → peek
  // (tooltip) → act (click). Each problem on its own line, with the size of its
  // downstream cone noted rather than enumerated. The tooltip is plain text, so
  // the source icon degrades to a ⚡ marker on eval entries (and a stale entry
  // says where it came from).
  //
  // A tooltip cannot be selected (it is an overlay that dismisses on
  // pointer-exit), so the copy affordances live one step further in: the
  // picker's *Copy all* header action and its per-row copy buttons.
  final tooltip = groups.map((group) {
    final e = group.root;
    final buffer = StringBuffer('• ');
    if (e.source == APIErrorSource.evaluation) buffer.write('⚡ ');
    buffer.write(e.errorText);
    final q = e.bodyQualifier;
    if (q != null) buffer.write(' ($q)');
    if (e.stale) buffer.write(' — from last evaluation');
    if (group.derived.isNotEmpty) {
      buffer.write(' (+${group.derived.length} downstream)');
    }
    return buffer.toString();
  }).join('\n');

  return Builder(
    builder: (badgeContext) => Tooltip(
      message: tooltip,
      child: InkWell(
        borderRadius: BorderRadius.circular(8),
        onTap: () => showValidationErrorsMenu(
          context: badgeContext,
          model: model,
          networkName: networkName,
          errors: errors,
          position: menuPositionForWidget(badgeContext),
        ),
        child: Padding(
          // A generous hit area around a small pill (Fitts's law).
          padding: const EdgeInsets.symmetric(horizontal: 2, vertical: 2),
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
            decoration: BoxDecoration(
              color: color,
              borderRadius: BorderRadius.circular(8),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(icon, size: 11, color: Colors.white),
                const SizedBox(width: 2),
                Text(
                  '${groups.length}',
                  style: AppTextStyles.regular.copyWith(
                    fontSize: 11,
                    color: Colors.white,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    ),
  );
}

/// The Find Usages count: a bare number that opens the same usage picker as the
/// context-menu entry. Rendered only when the count is > 0. Shared verbatim by
/// both views (was duplicated in each).
Widget buildNetworkUsageCountBadge({
  required BuildContext context,
  required StructureDesignerModel model,
  required String networkName,
  required bool isActive,
}) {
  final usageCount = model.networkUsageCounts[networkName] ?? 0;
  if (usageCount == 0) return const SizedBox.shrink();
  return Builder(
    builder: (countContext) => Tooltip(
      message: 'Used by $usageCount node${usageCount == 1 ? '' : 's'}',
      child: InkWell(
        onTap: () => findUsagesOfNetwork(
          context: countContext,
          model: model,
          networkName: networkName,
          position: menuPositionForWidget(countContext),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2),
          child: Text(
            '$usageCount',
            style: AppTextStyles.regular.copyWith(
              fontSize: 11,
              color: isActive
                  ? AppColors.selectionForeground.withValues(alpha: 0.8)
                  : Colors.grey,
            ),
          ),
        ),
      ),
    ),
  );
}

/// Presents a network's [errors] (validation + evaluation) and jumps to the
/// picked one's offending node.
///
/// Mirrors [showNetworkUsagesMenu]'s grammar so the two badges on a row behave
/// the same: a single navigable error jumps immediately (no extra click); a
/// single network-level error with no node to jump to shows its text in a
/// SnackBar; otherwise a picker lists every problem — navigable ones jump,
/// network-level ones are shown disabled (their text is still readable).
///
/// The list is **one row per underlying problem** (D7): derived errors — the
/// downstream cone of a root cause — are indented under their root instead of
/// competing with it for attention, and each carries a "go to root cause"
/// action. A root cause that lives in another network is listed here too, with
/// its provenance, because it is the failure this network's errors come from.
///
/// Every row also carries a **copy** button, and the header a **Copy all** that
/// puts the whole network's report on the clipboard (issue #359). Those buttons
/// deliberately do *not* pop the menu route — copying one message is usually
/// followed by copying another, or by jumping to the node.
Future<void> showValidationErrorsMenu({
  required BuildContext context,
  required StructureDesignerModel model,
  required String networkName,
  required List<APIValidationError> errors,
  required RelativeRect position,
}) async {
  if (errors.isEmpty) return;
  final groups = groupErrorsByRootCause(networkName, errors);

  if (groups.length == 1 && groups.first.derived.isEmpty) {
    final only = groups.first.root;
    if (isNavigableError(only)) {
      model.jumpToValidationError(networkName, only);
    } else {
      // Not navigable and not persistent — so the message gets a Copy action
      // rather than a selection it could never hold still for.
      showErrorSnackBar(context, only.errorText);
    }
    return;
  }

  final picked = await showMenu<_ErrorPick>(
    context: context,
    position: position,
    items: <PopupMenuEntry<_ErrorPick>>[
      PopupMenuItem<_ErrorPick>(
        enabled: false,
        // `MainAxisSize.min`, not an `Expanded` title: `showMenu` lays its
        // items out under an `IntrinsicWidth`, and a flex child there is a
        // trap. Every other row in this menu sizes the same way.
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text("Errors in '${getSimpleName(networkName)}'"),
            const SizedBox(width: 8),
            CopyTextButton(
              text: formatNetworkErrorReport(networkName, errors),
              tooltip: 'Copy all problems in this network',
              confirmation: 'Copied ${groups.length} '
                  'problem${groups.length == 1 ? '' : 's'} to clipboard',
              size: 15,
            ),
          ],
        ),
      ),
      for (final group in groups) ...[
        PopupMenuItem<_ErrorPick>(
          value: _ErrorPick(group.root),
          // A network-level error has no node to jump to — keep it visible but
          // unselectable so the count matches the list. Its copy button still
          // works: a nested InkWell is hit-tested even in a disabled item.
          enabled: isNavigableError(group.root),
          child: _errorMenuRow(networkName, group.root),
        ),
        for (final derived in group.derived)
          PopupMenuItem<_ErrorPick>(
            value: _ErrorPick(derived),
            enabled: isNavigableError(derived),
            child: _errorMenuRow(networkName, derived, indented: true),
          ),
      ],
    ],
  );
  if (picked == null) return;
  final root = picked.error.rootCause;
  if (picked.goToRoot && root != null) {
    // The picker is a route: the panel row that anchored it can be gone by the
    // time it closes, and `goToRootCause` needs a live context for its
    // transient surface.
    if (!context.mounted) return;
    goToRootCause(context, model, root);
    return;
  }
  model.jumpToValidationError(networkName, picked.error);
}

/// One picker row: a severity-colored source icon plus the error text and
/// (for a body error) its `in map1 body` qualifier. The icon encodes the
/// source (structural glyph vs bolt), the color the severity; a stale eval
/// entry (inactive network's last-known snapshot) renders dimmed with an
/// outlined bolt and a "from last evaluation" note.
///
/// [indented] marks a *derived* row — one of the root's downstream echoes,
/// nested under it. Such a row also carries a "go to root cause" action, which
/// pops the menu with [_ErrorPick.goToRoot] set so the caller jumps to the
/// failure instead of to this symptom.
///
/// The text wraps to [_ERROR_ROW_MAX_LINES] rather than the 2 it used to: a
/// chained runtime error (`error in a input (from sphere #12): …`) was reliably
/// unreadable here, which is half of what issue #359 was about. The remaining
/// cap keeps one pathological message from producing a menu taller than the
/// screen — the copy button hands over the untruncated text.
Widget _errorMenuRow(String networkName, APIValidationError error,
    {bool indented = false}) {
  final isEval = error.source == APIErrorSource.evaluation;
  final color = error.stale
      ? Colors.grey
      : error.blocking
          ? Colors.red.shade600
          : Colors.orange.shade700;
  final icon = isEval
      ? (error.stale ? Icons.offline_bolt_outlined : Icons.offline_bolt)
      : (error.blocking ? Icons.error : Icons.warning_amber_rounded);
  final qualifier = error.bodyQualifier;
  final subtitle = [
    if (qualifier != null) qualifier,
    if (error.stale) 'from last evaluation',
  ].join(' — ');
  return ConstrainedBox(
    constraints: const BoxConstraints(maxWidth: 420),
    child: Row(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // A derived row sits visually under its root cause.
        if (indented) const SizedBox(width: 18),
        Padding(
          padding: const EdgeInsets.only(top: 2),
          child: Icon(icon, size: 14, color: color),
        ),
        const SizedBox(width: 8),
        Flexible(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                error.errorText,
                overflow: TextOverflow.ellipsis,
                maxLines: _ERROR_ROW_MAX_LINES,
                // A stale entry reads as history, not live state.
                style: error.stale
                    ? AppTextStyles.regular.copyWith(color: Colors.grey)
                    : null,
              ),
              if (subtitle.isNotEmpty)
                Text(
                  subtitle,
                  overflow: TextOverflow.ellipsis,
                  style: AppTextStyles.regular.copyWith(
                    fontSize: 11,
                    color: Colors.grey,
                  ),
                ),
            ],
          ),
        ),
        const SizedBox(width: 8),
        // Copies this one entry, formatted the same way the whole-design
        // report formats it (tags + location + text) — a bare message with no
        // node id is much less useful in a bug report. Unlike the ↑ button
        // below it does *not* pop the route, so the list survives the copy.
        CopyTextButton(
          text: formatErrorEntry(networkName, error, derived: indented),
          tooltip: 'Copy this problem',
        ),
        if (error.rootCause != null) ...[
          const SizedBox(width: 4),
          // Popping the menu route with a `goToRoot` pick keeps the whole
          // decision in the caller's single `picked` branch.
          Builder(
            builder: (rowContext) => Tooltip(
              message: 'Go to root cause',
              child: InkWell(
                borderRadius: BorderRadius.circular(4),
                onTap: () => Navigator.of(rowContext)
                    .pop(_ErrorPick(error, goToRoot: true)),
                child: const Padding(
                  padding: EdgeInsets.all(2),
                  child: Icon(Icons.arrow_upward, size: 14, color: Colors.grey),
                ),
              ),
            ),
          ),
        ],
      ],
    ),
  );
}
