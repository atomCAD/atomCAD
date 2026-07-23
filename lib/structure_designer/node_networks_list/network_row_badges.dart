import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/structure_designer/find_usages_menu.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Shared trailing badges for a network row in the user-types panel — the
/// validation-error badge and the Find Usages count — so the flat list view and
/// the tree view render them identically and can't drift apart.
///
/// Both are small clickable pills anchored via [menuPositionForWidget]:
/// - The **error badge** (this file) opens a picker of the network's errors,
///   each of which navigates to the offending node — the same "count → click →
///   go there" grammar Find Usages gave instance usages, applied to errors.
/// - The **usage count** delegates to [findUsagesOfNetwork] unchanged.

/// Whether a validation error can be navigated to (it is anchored to a node).
bool _isNavigable(APIValidationError e) => e.nodeId != null;

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
  // The full error list on hover — progressive disclosure: glance (badge) →
  // peek (tooltip) → act (click). Each error on its own line.
  final tooltip = errors.map((e) {
    final q = e.bodyQualifier;
    return q == null ? '• ${e.errorText}' : '• ${e.errorText} ($q)';
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
                  '${errors.length}',
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

/// Presents a network's validation [errors] and jumps to the picked one's
/// offending node.
///
/// Mirrors [showNetworkUsagesMenu]'s grammar so the two badges on a row behave
/// the same: a single navigable error jumps immediately (no extra click); a
/// single network-level error with no node to jump to shows its text in a
/// SnackBar; otherwise a picker lists every error — navigable ones jump,
/// network-level ones are shown disabled (their text is still readable).
Future<void> showValidationErrorsMenu({
  required BuildContext context,
  required StructureDesignerModel model,
  required String networkName,
  required List<APIValidationError> errors,
  required RelativeRect position,
}) async {
  if (errors.isEmpty) return;
  final messenger = ScaffoldMessenger.maybeOf(context);

  if (errors.length == 1) {
    final only = errors.first;
    if (_isNavigable(only)) {
      model.jumpToValidationError(networkName, only);
    } else {
      messenger?.showSnackBar(SnackBar(content: Text(only.errorText)));
    }
    return;
  }

  final picked = await showMenu<int>(
    context: context,
    position: position,
    items: <PopupMenuEntry<int>>[
      PopupMenuItem<int>(
        enabled: false,
        child: Text("Errors in '${getSimpleName(networkName)}'"),
      ),
      for (int i = 0; i < errors.length; i++)
        PopupMenuItem<int>(
          value: i,
          // A network-level error has no node to jump to — keep it visible but
          // unselectable so the count matches the list.
          enabled: _isNavigable(errors[i]),
          child: _errorMenuRow(errors[i]),
        ),
    ],
  );
  if (picked == null) return;
  model.jumpToValidationError(networkName, errors[picked]);
}

/// One picker row: a severity icon plus the error text and (for a body error)
/// its `in map1 body` qualifier.
Widget _errorMenuRow(APIValidationError error) {
  final color = error.blocking ? Colors.red.shade600 : Colors.orange.shade700;
  final icon = error.blocking ? Icons.error : Icons.warning_amber_rounded;
  final qualifier = error.bodyQualifier;
  return ConstrainedBox(
    constraints: const BoxConstraints(maxWidth: 420),
    child: Row(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
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
                maxLines: 2,
              ),
              if (qualifier != null)
                Text(
                  qualifier,
                  overflow: TextOverflow.ellipsis,
                  style: AppTextStyles.regular.copyWith(
                    fontSize: 11,
                    color: Colors.grey,
                  ),
                ),
            ],
          ),
        ),
      ],
    ),
  );
}
