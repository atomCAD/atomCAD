/// *File → Export node network image* — writes the active network to a PNG,
/// however much larger than the canvas viewport it is: the whole network, or
/// (with *Only the selection*) the region its selected nodes span.
///
/// The canvas is a plain Flutter widget tree, so the image is produced by
/// laying that same tree out at the full size of the content in a private
/// render tree and rasterizing it (`lib/common/offscreen_capture.dart`). The
/// user sees no flicker and the live canvas is not disturbed: its pan and zoom
/// are never touched, because the exported view is framed independently from
/// [nodeNetworkContentBounds].
///
/// Two consequences are worth knowing before changing anything here:
///
/// * **Selection and active-node highlights are the one thing dropped** (via
///   `NodeNetworkCanvasSnapshot.hideSelection`). They are editing state a reader
///   of the picture cannot tell apart from meaning, and a region export is
///   framed *by* a selection that would otherwise light up the whole image. It
///   is done at render time rather than by clearing the selection, because
///   clearing it in the kernel also clears the active node, which re-runs the
///   display policy and can re-evaluate the 3D scene — twice, for a picture.
///   Everything else is exported as it looks on screen: error borders,
///   collapsed HOF bodies, per-node styling.
/// * **A comment note's text is clipped by the note**, exactly as on the canvas
///   (the note's static body is a `SingleChildScrollView`). Text scrolled out of
///   view is not in the image either; the note has to be resized to show it.
/// * **A region export crops, it does not filter.** Everything inside the
///   selection's bounding box renders — unselected nodes included, and wires
///   leaving the box are drawn up to the edge. That is what makes the crop look
///   like the canvas instead of like a subset of it.
library;

import 'dart:io';
import 'dart:math' as math;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/file_dialog_directory.dart';
import 'package:flutter_cad/common/offscreen_capture.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network_content.dart';

/// Pixel-ratio choices offered in the export dialog. 1× is the default — a
/// faithful, modestly-sized image; the higher ratios are there for when small
/// node labels have to stay legible after scaling down elsewhere.
const List<double> EXPORT_IMAGE_SCALES = [1.0, 2.0, 3.0];

/// Default whitespace around the content, in pixels of the exported image at
/// 1×. Matches the canvas's own content margin.
const double DEFAULT_EXPORT_IMAGE_MARGIN = NODE_NETWORK_CONTENT_MARGIN;

/// Upper bound on the margin field. Far past anything useful; it exists so a
/// stray keystroke cannot demand a gigapixel of blank paper.
const double MAX_EXPORT_IMAGE_MARGIN = 2000.0;

/// What the user chose in the export dialog.
class _ExportOptions {
  final ZoomLevel zoomLevel;
  final double pixelRatio;

  /// Frame the image around the selected top-level nodes instead of the whole
  /// network. See [nodeNetworkContentBounds].
  final bool selectedOnly;

  /// Whitespace around the content, in pixels of the exported image at 1× (so
  /// the number matches the size the dialog reports). Converted to canvas units
  /// by [_logicalMargin], since that is what the framing works in.
  final double marginPx;

  const _ExportOptions(
      this.zoomLevel, this.pixelRatio, this.selectedOnly, this.marginPx);
}

/// Asks for framing, zoom level, resolution and margin, renders the active
/// network, and saves it.
///
/// The dialog always opens on 100% / 1× rather than following the canvas's
/// current zoom: an exported image is nearly always wanted at full detail,
/// whatever the user happened to be zoomed out to while editing.
Future<void> exportNetworkImage(
  BuildContext context,
  StructureDesignerModel model,
) async {
  final view = model.nodeNetworkView;
  if (view == null || view.nodes.isEmpty) {
    showErrorSnackBar(context, 'There is no node network to export.');
    return;
  }

  final options = await _showExportOptionsDialog(context, view);
  if (options == null || !context.mounted) return;

  final bounds = nodeNetworkContentBounds(
    view: view,
    zoomLevel: options.zoomLevel,
    selectedOnly: options.selectedOnly,
    margin: _logicalMargin(options.marginPx, options.zoomLevel),
  );
  final snapshot = NodeNetworkCanvasSnapshot(
    model: model,
    view: view,
    logicalBounds: bounds,
    zoomLevel: options.zoomLevel,
  );

  // `View.of` before the first await: the capture needs the window's
  // `FlutterView` handle, and this context may be gone by the time the file
  // dialog closes.
  final flutterView = View.of(context);
  final messenger = ScaffoldMessenger.maybeOf(context);

  OffscreenCapture capture;
  try {
    capture = await capturePngOffscreen(
      child: snapshot,
      logicalSize: snapshot.pixelSize,
      view: flutterView,
      pixelRatio: options.pixelRatio,
    );
  } on OffscreenCaptureTooLargeException catch (e) {
    if (context.mounted) {
      await showErrorDialog(
        context: context,
        title: 'Network too large to export',
        message: e.toString(),
      );
    }
    return;
  } catch (e) {
    if (context.mounted) {
      await showErrorDialog(
        context: context,
        title: 'Export failed',
        message: 'Rendering the node network image failed: $e',
      );
    }
    return;
  }

  final outputFile = await FilePicker.platform.saveFile(
    dialogTitle: 'Export node network image',
    fileName: '${_fileNameFor(view.name)}'
        '${options.selectedOnly ? '_selection' : ''}.png',
    type: FileType.custom,
    allowedExtensions: ['png'],
    initialDirectory: initialDirectoryFor(APIFileDialogPurpose.networkImage),
  );
  if (outputFile == null) return;

  final path = outputFile.contains('.') ? outputFile : '$outputFile.png';
  rememberPickedFile(APIFileDialogPurpose.networkImage, path);

  try {
    await File(path).writeAsBytes(capture.pngBytes, flush: true);
  } catch (e) {
    if (messenger != null) {
      showErrorSnackBarOn(messenger, 'Could not write $path: $e');
    }
    return;
  }

  final sizeNote =
      _wasMeaningfullyCapped(capture.pixelRatio, options.pixelRatio)
          ? ' (reduced from ${_formatRatio(options.pixelRatio)}× to fit the '
              '$MAX_OFFSCREEN_CAPTURE_DIMENSION px limit)'
          : '';
  messenger
    ?..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(
      content: Text('Exported ${capture.widthPx}×${capture.heightPx} px '
          'to ${_baseName(path)}$sizeNote'),
      duration: const Duration(seconds: 4),
    ));
}

/// Turns a (possibly namespaced) network name into a filename stem.
String _fileNameFor(String networkName) {
  final cleaned = networkName.replaceAll(RegExp(r'[^A-Za-z0-9_.-]+'), '_');
  final trimmed = cleaned.replaceAll(RegExp(r'^_+|_+$'), '');
  return trimmed.isEmpty ? 'node_network' : trimmed;
}

String _baseName(String path) {
  final parts =
      path.split(RegExp(r'[\\/]')).where((p) => p.isNotEmpty).toList();
  return parts.isEmpty ? path : parts.last;
}

/// Whether the clamped [effective] ratio differs from the [requested] one by
/// enough to be worth telling the user about.
///
/// Content whose width lands exactly on [MAX_OFFSCREEN_CAPTURE_DIMENSION] at the
/// requested ratio computes a limit of 2.9999…× rather than 3× — floating-point
/// division, not a real cap. Reporting that as "capped at 3×" is both wrong and
/// self-contradictory, so anything within half a percent counts as uncapped.
bool _wasMeaningfullyCapped(double effective, double requested) =>
    effective < requested * 0.995;

/// `2` rather than `2.0`, `1.4` rather than `1.4000000000000001`.
String _formatRatio(double ratio) {
  final rounded = (ratio * 10).round() / 10;
  return rounded == rounded.roundToDouble()
      ? rounded.round().toString()
      : rounded.toString();
}

/// The margin in canvas units, which is what the framing works in. The user
/// sets it in pixels of the exported image at 1×, so it has to be divided by the
/// zoom scale — otherwise a zoomed-out export would come out with a
/// proportionally thinner border than the number promised.
double _logicalMargin(double marginPx, ZoomLevel zoomLevel) =>
    marginPx / getZoomScale(zoomLevel);

Future<_ExportOptions?> _showExportOptionsDialog(
  BuildContext context,
  NodeNetworkView view,
) {
  return showDialog<_ExportOptions>(
    context: context,
    barrierDismissible: false,
    builder: (context) => _ExportNetworkImageDialog(view: view),
  );
}

/// The options dialog. Draggable per `lib/AGENTS.md`; it shows the resulting
/// image size live, because "how big will this be" is the only thing the two
/// controls are really for.
class _ExportNetworkImageDialog extends StatefulWidget {
  final NodeNetworkView view;

  const _ExportNetworkImageDialog({required this.view});

  @override
  State<_ExportNetworkImageDialog> createState() =>
      _ExportNetworkImageDialogState();
}

class _ExportNetworkImageDialogState extends State<_ExportNetworkImageDialog> {
  ZoomLevel _zoomLevel = ZoomLevel.normal;
  double _pixelRatio = 1.0;

  /// Margin in pixels of the exported image at 1×. `null` while the field holds
  /// something unusable, which disables Export rather than silently exporting
  /// with a margin the user did not ask for.
  double? _marginPx = DEFAULT_EXPORT_IMAGE_MARGIN;

  late final TextEditingController _marginController =
      TextEditingController(text: _formatRatio(DEFAULT_EXPORT_IMAGE_MARGIN));

  @override
  void dispose() {
    _marginController.dispose();
    super.dispose();
  }

  void _onMarginChanged(String text) {
    final parsed = double.tryParse(text.trim());
    setState(() {
      _marginPx = (parsed == null ||
              parsed.isNaN ||
              parsed < 0 ||
              parsed > MAX_EXPORT_IMAGE_MARGIN)
          ? null
          : parsed;
    });
  }

  /// Frame the image around the selection rather than the whole network. Starts
  /// on when there is a selection to frame — the user who just made one almost
  /// certainly made it *for* this.
  late bool _selectedOnly = _selectedNodeCount > 0;

  late final int _selectedNodeCount = countSelectedTopLevelNodes(widget.view);

  bool get _canExportSelection => _selectedNodeCount > 0;

  /// Logical size of the image at the selected zoom level, before the pixel
  /// ratio is applied. Follows the region checkbox, which is what lets a
  /// selection export at a zoom level the whole network is too large for.
  Size get _logicalSize =>
      nodeNetworkContentBounds(
        view: widget.view,
        zoomLevel: _zoomLevel,
        selectedOnly: _selectedOnly && _canExportSelection,
        margin: _logicalMargin(_marginPx ?? 0, _zoomLevel),
      ).size *
      getZoomScale(_zoomLevel);

  /// The pixel ratio that will actually be used — the capture clamps it to
  /// [MAX_OFFSCREEN_CAPTURE_DIMENSION], so the dialog reports the clamped value
  /// rather than promising a crispness it cannot deliver.
  double get _effectiveRatio {
    final longest = math.max(_logicalSize.width, _logicalSize.height);
    if (longest <= 0) return _pixelRatio;
    return math.min(_pixelRatio, MAX_OFFSCREEN_CAPTURE_DIMENSION / longest);
  }

  bool get _tooLarge => _effectiveRatio < 1.0;

  /// Whether this export will actually be framed on the selection.
  bool get _exportsSelection => _selectedOnly && _canExportSelection;

  /// The region checkbox. Disabled — with the reason spelled out — when there is
  /// nothing selected at the top level, rather than silently doing nothing.
  Widget _selectionCheckbox(BuildContext context) {
    final label = _canExportSelection
        ? 'Only the selection ($_selectedNodeCount '
            '${_selectedNodeCount == 1 ? 'node' : 'nodes'})'
        : 'Only the selection (nothing selected)';
    return InkWell(
      onTap: _canExportSelection
          ? () => setState(() => _selectedOnly = !_selectedOnly)
          : null,
      child: Row(
        children: [
          Checkbox(
            key: const Key('export_image_selection_checkbox'),
            value: _exportsSelection,
            onChanged: _canExportSelection
                ? (value) => setState(() => _selectedOnly = value ?? false)
                : null,
          ),
          Flexible(
            child: Text(
              label,
              style: _canExportSelection
                  ? null
                  : TextStyle(color: Theme.of(context).disabledColor),
            ),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final logical = _logicalSize;
    final ratio = _effectiveRatio;
    final widthPx = (logical.width * ratio).ceil();
    final heightPx = (logical.height * ratio).ceil();

    return _DraggableFrame(
      title: 'Export node network image',
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
              _exportsSelection
                  ? 'Exports the selected part of "${widget.view.name}", '
                      'not just the visible part.'
                  : 'Exports all of "${widget.view.name}", not just the '
                      'visible part.',
              style: Theme.of(context).textTheme.bodySmall),
          const SizedBox(height: 8),
          _selectionCheckbox(context),
          const SizedBox(height: 8),
          _row(
            'Zoom level',
            DropdownButton<ZoomLevel>(
              key: const Key('export_image_zoom_dropdown'),
              value: _zoomLevel,
              isExpanded: true,
              onChanged: (value) {
                if (value != null) setState(() => _zoomLevel = value);
              },
              items: ZoomLevel.values
                  .map((level) => DropdownMenuItem(
                        value: level,
                        child: Text(_zoomLabel(level)),
                      ))
                  .toList(),
            ),
          ),
          const SizedBox(height: 8),
          _row(
            'Resolution',
            DropdownButton<double>(
              key: const Key('export_image_scale_dropdown'),
              value: _pixelRatio,
              isExpanded: true,
              onChanged: (value) {
                if (value != null) setState(() => _pixelRatio = value);
              },
              items: EXPORT_IMAGE_SCALES
                  .map((scale) => DropdownMenuItem(
                        value: scale,
                        child: Text('${_formatRatio(scale)}×'),
                      ))
                  .toList(),
            ),
          ),
          const SizedBox(height: 8),
          _row(
            'Margin',
            TextField(
              key: const Key('export_image_margin_field'),
              controller: _marginController,
              keyboardType:
                  const TextInputType.numberWithOptions(decimal: true),
              onChanged: _onMarginChanged,
              decoration: InputDecoration(
                isDense: true,
                suffixText: 'px',
                helperText: 'blank space around the content, at 1×',
                errorText: _marginPx == null
                    ? '0 – ${MAX_EXPORT_IMAGE_MARGIN.round()}'
                    : null,
              ),
            ),
          ),
          const SizedBox(height: 16),
          if (_tooLarge)
            ErrorBanner(
              message: 'At this zoom level the image would be '
                  '${logical.width.ceil()}×${logical.height.ceil()} px, over the '
                  '$MAX_OFFSCREEN_CAPTURE_DIMENSION px limit. Choose a smaller '
                  'zoom level${_exportsSelection ? '' : ', or select a part of '
                      'the network and tick "Only the selection"'}.',
            )
          else
            Text(
              _wasMeaningfullyCapped(ratio, _pixelRatio)
                  ? 'Image: $widthPx×$heightPx px — reduced from '
                      '${_formatRatio(_pixelRatio)}× to fit the '
                      '$MAX_OFFSCREEN_CAPTURE_DIMENSION px limit.'
                  : 'Image: $widthPx×$heightPx px',
              style: Theme.of(context).textTheme.bodyMedium,
            ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        const SizedBox(width: 8),
        ElevatedButton(
          key: const Key('export_image_confirm_button'),
          onPressed: _tooLarge || _marginPx == null
              ? null
              : () => Navigator.of(context).pop(_ExportOptions(
                  _zoomLevel, _pixelRatio, _exportsSelection, _marginPx!)),
          child: const Text('Export'),
        ),
      ],
    );
  }

  static String _zoomLabel(ZoomLevel level) {
    final percent = (getZoomScale(level) * 100).round();
    switch (level) {
      case ZoomLevel.normal:
        return 'Normal ($percent%)';
      case ZoomLevel.zoomedOutMedium:
        return 'Zoomed out ($percent%)';
      case ZoomLevel.zoomedOutFar:
        return 'Zoomed out far ($percent%)';
    }
  }

  Widget _row(String label, Widget field) => Row(
        children: [
          SizedBox(width: 110, child: Text(label)),
          Expanded(child: field),
        ],
      );
}

/// The dialog chrome, matching `showDraggableAlertDialog`'s layout. Built by
/// hand rather than with that helper because this dialog's content is stateful
/// and its buttons return a value through `Navigator.pop`. Dialogs must be
/// draggable — see `lib/AGENTS.md`.
class _DraggableFrame extends StatelessWidget {
  final String title;
  final Widget content;
  final List<Widget> actions;

  const _DraggableFrame({
    required this.title,
    required this.content,
    required this.actions,
  });

  @override
  Widget build(BuildContext context) {
    return DraggableDialog(
      width: 460,
      dismissible: true,
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            DefaultTextStyle(
              style: Theme.of(context).textTheme.headlineSmall!,
              child: Text(title),
            ),
            const SizedBox(height: 16),
            content,
            const SizedBox(height: 24),
            Row(mainAxisAlignment: MainAxisAlignment.end, children: actions),
          ],
        ),
      ),
    );
  }
}
