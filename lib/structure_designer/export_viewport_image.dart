/// *File → Export viewport image* — writes the 3D viewport to a PNG.
///
/// Deliberately small: a size (the viewport's own, or a custom one) and a
/// background (as on screen, or white). Everything else is fixed:
///
/// * **Always anti-aliased.** Rust renders the frame at 2× and box-downsamples
///   it (`Renderer::render_image`); there is no switch, because nobody wants
///   the jagged version.
/// * **At most [_MAX_SIDE_HINT] px a side.** The 2× render has to fit the GPU's
///   texture limit. A larger request — including a viewport on a big monitor —
///   is scaled down with its aspect ratio kept, and the dialog says so before
///   exporting. The rule itself is Rust's (`fit_viewport_image_size`); this file
///   only displays its answer.
/// * **What the viewport shows, minus the gizmo.** The live camera is used as
///   is; a custom aspect keeps the vertical field of view, so a wider image
///   shows more at the sides. The grid, axes, labels and selection rims are
///   kept; the gadget pass (gizmo, guide dots, pivot cube) is left out.
library;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/file_dialog_directory.dart';
import 'package:flutter_cad/src/rust/api/screenshot_api.dart' as screenshot_api;

/// The limit as shown in the dialog text. The enforced value lives in Rust
/// (`IMAGE_EXPORT_MAX_SIZE`).
const int _MAX_SIDE_HINT = 2048;

const List<int> _WHITE_RGB = [255, 255, 255];

enum _SizeMode { viewport, custom }

enum _BackgroundMode { asOnScreen, white }

class _ExportOptions {
  final int width;
  final int height;
  final _BackgroundMode background;

  const _ExportOptions(this.width, this.height, this.background);
}

/// Asks for size and background, then a file name, and writes the image.
/// [baseName] seeds the file name (the active network's name).
Future<void> exportViewportImage(BuildContext context, String? baseName) async {
  final viewport = screenshot_api.getViewportImageSize();
  if (viewport.length < 2 || viewport[0] == 0 || viewport[1] == 0) {
    showErrorSnackBar(context, 'The viewport is not ready yet.');
    return;
  }

  final options = await showDialog<_ExportOptions>(
    context: context,
    barrierDismissible: false,
    builder: (context) => _ExportViewportImageDialog(
      viewportWidth: viewport[0],
      viewportHeight: viewport[1],
    ),
  );
  if (options == null || !context.mounted) return;
  final messenger = ScaffoldMessenger.maybeOf(context);

  final outputFile = await FilePicker.platform.saveFile(
    dialogTitle: 'Export viewport image',
    fileName: '${_fileNameFor(baseName)}.png',
    type: FileType.custom,
    allowedExtensions: ['png'],
    initialDirectory: initialDirectoryFor(APIFileDialogPurpose.viewportImage),
  );
  if (outputFile == null) return;

  final path = outputFile.contains('.') ? outputFile : '$outputFile.png';
  rememberPickedFile(APIFileDialogPurpose.viewportImage, path);

  final result = screenshot_api.exportViewportImage(
    outputPath: path,
    width: options.width,
    height: options.height,
    backgroundRgb: options.background == _BackgroundMode.white
        ? Uint8List.fromList(_WHITE_RGB)
        : null,
  );
  if (messenger == null) return;
  if (!result.success) {
    showErrorSnackBarOn(
        messenger, result.errorMessage ?? 'Exporting the image failed.');
    return;
  }
  messenger
    ..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(
      content: Text('Exported ${result.width}×${result.height} px '
          'to ${_baseName(path)}'),
      duration: const Duration(seconds: 4),
    ));
}

/// Turns a (possibly namespaced) network name into a filename stem.
String _fileNameFor(String? name) {
  final cleaned = (name ?? '').replaceAll(RegExp(r'[^A-Za-z0-9_.-]+'), '_');
  final trimmed = cleaned.replaceAll(RegExp(r'^_+|_+$'), '');
  return trimmed.isEmpty ? 'viewport' : trimmed;
}

String _baseName(String path) {
  final parts =
      path.split(RegExp(r'[\\/]')).where((p) => p.isNotEmpty).toList();
  return parts.isEmpty ? path : parts.last;
}

class _ExportViewportImageDialog extends StatefulWidget {
  final int viewportWidth;
  final int viewportHeight;

  const _ExportViewportImageDialog({
    required this.viewportWidth,
    required this.viewportHeight,
  });

  @override
  State<_ExportViewportImageDialog> createState() =>
      _ExportViewportImageDialogState();
}

class _ExportViewportImageDialogState
    extends State<_ExportViewportImageDialog> {
  _SizeMode _sizeMode = _SizeMode.viewport;
  _BackgroundMode _background = _BackgroundMode.asOnScreen;

  /// The custom size as typed; `null` while a field holds something unusable,
  /// which disables Export rather than exporting a size nobody asked for.
  int? _customWidth;
  int? _customHeight;

  late final TextEditingController _widthController =
      TextEditingController(text: '${widget.viewportWidth}');
  late final TextEditingController _heightController =
      TextEditingController(text: '${widget.viewportHeight}');

  @override
  void initState() {
    super.initState();
    _customWidth = widget.viewportWidth;
    _customHeight = widget.viewportHeight;
  }

  @override
  void dispose() {
    _widthController.dispose();
    _heightController.dispose();
    super.dispose();
  }

  static int? _parseSide(String text) {
    final value = int.tryParse(text.trim());
    return (value == null || value < 1) ? null : value;
  }

  /// The requested size, or `null` when a custom field is invalid.
  (int, int)? get _requested => _sizeMode == _SizeMode.viewport
      ? (widget.viewportWidth, widget.viewportHeight)
      : (_customWidth != null && _customHeight != null)
          ? (_customWidth!, _customHeight!)
          : null;

  @override
  Widget build(BuildContext context) {
    final requested = _requested;
    final fitted = requested == null
        ? null
        : screenshot_api.fitViewportImageSize(
            width: requested.$1, height: requested.$2);
    final custom = _sizeMode == _SizeMode.custom;

    return DraggableDialog(
      width: 420,
      dismissible: true,
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            DefaultTextStyle(
              style: Theme.of(context).textTheme.headlineSmall!,
              child: const Text('Export viewport image'),
            ),
            const SizedBox(height: 16),
            _row(
              'Size',
              DropdownButton<_SizeMode>(
                key: const Key('export_viewport_size_dropdown'),
                value: _sizeMode,
                isExpanded: true,
                onChanged: (value) {
                  if (value != null) setState(() => _sizeMode = value);
                },
                items: [
                  DropdownMenuItem(
                    value: _SizeMode.viewport,
                    child: Text('Viewport size (${widget.viewportWidth} × '
                        '${widget.viewportHeight})'),
                  ),
                  const DropdownMenuItem(
                    value: _SizeMode.custom,
                    child: Text('Custom'),
                  ),
                ],
              ),
            ),
            if (custom) ...[
              const SizedBox(height: 8),
              _row(
                '',
                Row(
                  children: [
                    Expanded(
                      child: _sideField(
                        key: const Key('export_viewport_width_field'),
                        controller: _widthController,
                        label: 'Width',
                        invalid: _customWidth == null,
                        onChanged: (text) =>
                            setState(() => _customWidth = _parseSide(text)),
                      ),
                    ),
                    const Padding(
                      padding: EdgeInsets.symmetric(horizontal: 8),
                      child: Text('×'),
                    ),
                    Expanded(
                      child: _sideField(
                        key: const Key('export_viewport_height_field'),
                        controller: _heightController,
                        label: 'Height',
                        invalid: _customHeight == null,
                        onChanged: (text) =>
                            setState(() => _customHeight = _parseSide(text)),
                      ),
                    ),
                  ],
                ),
              ),
            ],
            const SizedBox(height: 8),
            _row(
              'Background',
              DropdownButton<_BackgroundMode>(
                key: const Key('export_viewport_background_dropdown'),
                value: _background,
                isExpanded: true,
                onChanged: (value) {
                  if (value != null) setState(() => _background = value);
                },
                items: const [
                  DropdownMenuItem(
                    value: _BackgroundMode.asOnScreen,
                    child: Text('As on screen'),
                  ),
                  DropdownMenuItem(
                    value: _BackgroundMode.white,
                    child: Text('White'),
                  ),
                ],
              ),
            ),
            const SizedBox(height: 16),
            Text(
              fitted == null
                  ? 'Enter a width and a height of at least 1 px.'
                  : fitted.reduced
                      ? 'Image: ${fitted.width}×${fitted.height} px — reduced '
                          'to fit the $_MAX_SIDE_HINT px limit.'
                      : 'Image: ${fitted.width}×${fitted.height} px',
              style: Theme.of(context).textTheme.bodyMedium,
            ),
            const SizedBox(height: 4),
            Text(
              'Anti-aliased. The gizmo is left out.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 24),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  key: const Key('export_viewport_confirm_button'),
                  onPressed: fitted == null
                      ? null
                      : () => Navigator.of(context).pop(_ExportOptions(
                          fitted.width, fitted.height, _background)),
                  child: const Text('Export...'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _sideField({
    required Key key,
    required TextEditingController controller,
    required String label,
    required bool invalid,
    required ValueChanged<String> onChanged,
  }) =>
      TextField(
        key: key,
        controller: controller,
        keyboardType: TextInputType.number,
        inputFormatters: [FilteringTextInputFormatter.digitsOnly],
        onChanged: onChanged,
        decoration: InputDecoration(
          isDense: true,
          labelText: label,
          suffixText: 'px',
          errorText: invalid ? 'at least 1' : null,
        ),
      );

  Widget _row(String label, Widget field) => Row(
        children: [
          SizedBox(width: 90, child: Text(label)),
          Expanded(child: field),
        ],
      );
}
