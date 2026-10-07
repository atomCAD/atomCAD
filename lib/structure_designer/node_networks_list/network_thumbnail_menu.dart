import 'package:flutter/material.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Context-menu values for the two thumbnail commands (D6) of a tree-view row.
const String PIN_THUMBNAIL_MENU_VALUE = 'pin_thumbnail';
const String UNPIN_THUMBNAIL_MENU_VALUE = 'unpin_thumbnail';

/// The thumbnail items of a local network's context menu (D9): *Pin current
/// view as thumbnail* (enabled only on the active network, the only one with a
/// scene to render), and *Unpin thumbnail* when the thumbnail is pinned.
/// Linked networks never get these items (D8).
List<PopupMenuEntry<String>> thumbnailMenuItems({
  required bool isActiveNetwork,
  required bool pinned,
}) {
  return [
    PopupMenuItem(
      value: PIN_THUMBNAIL_MENU_VALUE,
      enabled: isActiveNetwork,
      child: const Text('Pin current view as thumbnail'),
    ),
    if (pinned)
      const PopupMenuItem(
        value: UNPIN_THUMBNAIL_MENU_VALUE,
        child: Text('Unpin thumbnail (update automatically)'),
      ),
  ];
}

/// Runs the thumbnail command a context menu returned for [networkName], and
/// reports a refusal (for example, nothing is displayed) as an error snackbar.
void handleThumbnailMenuValue(BuildContext context,
    StructureDesignerModel model, String value, String networkName) {
  final error = value == PIN_THUMBNAIL_MENU_VALUE
      ? model.pinCurrentViewAsThumbnail(networkName)
      : model.unpinNetworkThumbnail(networkName);
  if (error != null && context.mounted) {
    showErrorSnackBar(context, 'Thumbnail: $error');
  }
}
