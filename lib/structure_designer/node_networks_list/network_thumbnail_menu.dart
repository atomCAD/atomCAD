import 'package:flutter/material.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Context-menu values for the two thumbnail commands (D6), shared by the list
/// and tree views.
const String SET_THUMBNAIL_MENU_VALUE = 'set_thumbnail';
const String RESET_THUMBNAIL_MENU_VALUE = 'reset_thumbnail';

/// The thumbnail items of a local network's context menu (D9): *Set current
/// view as thumbnail* (enabled only on the active network, the only one with a
/// scene to render), and *Reset to automatic thumbnail* when the thumbnail is
/// user-set. Linked networks never get these items (D8).
List<PopupMenuEntry<String>> thumbnailMenuItems({
  required bool isActiveNetwork,
  required bool userSet,
}) {
  return [
    PopupMenuItem(
      value: SET_THUMBNAIL_MENU_VALUE,
      enabled: isActiveNetwork,
      child: const Text('Set current view as thumbnail'),
    ),
    if (userSet)
      const PopupMenuItem(
        value: RESET_THUMBNAIL_MENU_VALUE,
        child: Text('Reset to automatic thumbnail'),
      ),
  ];
}

/// Runs the thumbnail command a context menu returned for [networkName], and
/// reports a refusal (for example, nothing is displayed) as an error snackbar.
void handleThumbnailMenuValue(BuildContext context,
    StructureDesignerModel model, String value, String networkName) {
  final error = value == SET_THUMBNAIL_MENU_VALUE
      ? model.setCurrentViewAsThumbnail(networkName)
      : model.resetNetworkThumbnail(networkName);
  if (error != null && context.mounted) {
    showErrorSnackBar(context, 'Thumbnail: $error');
  }
}
