import 'package:flutter/material.dart';
import 'package:flutter_cad/common/color_field_widget.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor for the `isosurface` node.
///
/// Four groups, in the order a user reaches for them: the isolevel, the two
/// phase colors, the opacity, and the colormap domain.
///
/// **The swap button is not a convenience.** An orbital's global sign is
/// arbitrary — the same calculation run twice can hand back `psi` or `-psi` —
/// so exchanging the two colors is how a user matches a published figure. It
/// swaps the colors and nothing else.
///
/// **The colormap controls are disabled unless `color_field` is wired**,
/// because they describe a domain that nothing reads while the surface is
/// painted per sign. They stay visible rather than hidden so the stored values
/// are still inspectable, and so the panel does not change shape when a wire is
/// made.
class IsosurfaceEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIIsosurfaceData? data;
  final StructureDesignerModel model;

  /// Whether the `color_field` input pin currently has a wire. Gates the
  /// colormap group.
  final bool colorFieldConnected;

  const IsosurfaceEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
    required this.colorFieldConnected,
  });

  void _update({
    double? level,
    APIVec3? positiveColor,
    APIVec3? negativeColor,
    double? alpha,
    APIColormap? colormap,
    double? colorMin,
    double? colorMax,
  }) {
    final current = data;
    if (current == null) return;
    model.setIsosurfaceData(
      nodeId,
      APIIsosurfaceData(
        level: level ?? current.level,
        positiveColor: positiveColor ?? current.positiveColor,
        negativeColor: negativeColor ?? current.negativeColor,
        alpha: alpha ?? current.alpha,
        colormap: colormap ?? current.colormap,
        colorMin: colorMin ?? current.colorMin,
        colorMax: colorMax ?? current.colorMax,
      ),
    );
  }

  void _swapColors() {
    final current = data;
    if (current == null) return;
    _update(
      positiveColor: current.negativeColor,
      negativeColor: current.positiveColor,
    );
  }

  @override
  Widget build(BuildContext context) {
    final current = data;
    if (current == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final captionStyle = Theme.of(context).textTheme.bodySmall;
    final headingStyle = Theme.of(context).textTheme.titleSmall;

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const NodeEditorHeader(
              title: 'Isosurface Properties',
              nodeTypeName: 'isosurface',
            ),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Level',
              value: current.level,
              onChanged: (value) => _update(level: value),
            ),
            const SizedBox(height: 4),
            Text(
              'A magnitude: the surface is drawn at +level and at -level, so a '
              'signed field shows both lobes. Must be greater than 0. A level '
              'above anything in the field draws nothing — hover the `field` '
              'output pin on the upstream import_cube node to read the '
              "field's value range and what it holds. Wiring the `level` "
              'input pin overrides this value.',
              style: captionStyle,
            ),
            const SizedBox(height: 16),
            Row(
              children: [
                Text('Phase colors', style: headingStyle),
                const Spacer(),
                Tooltip(
                  message: 'Swap positive and negative colors',
                  child: IconButton(
                    key: const Key('isosurface_swap_colors'),
                    icon: const Icon(Icons.swap_vert),
                    iconSize: 18,
                    visualDensity: VisualDensity.compact,
                    onPressed: _swapColors,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text('Positive (+level)', style: captionStyle),
            const SizedBox(height: 4),
            ColorFieldWidget(
              keyPrefix: 'isosurface_positive_color',
              value: current.positiveColor,
              onChanged: (value) => _update(positiveColor: value),
            ),
            const SizedBox(height: 8),
            Text('Negative (-level)', style: captionStyle),
            const SizedBox(height: 4),
            ColorFieldWidget(
              keyPrefix: 'isosurface_negative_color',
              value: current.negativeColor,
              onChanged: (value) => _update(negativeColor: value),
            ),
            const SizedBox(height: 4),
            Text(
              'An orbital\'s overall sign is arbitrary, so the swap button is '
              'how you match a published figure.',
              style: captionStyle,
            ),
            const SizedBox(height: 16),
            Text('Opacity', style: headingStyle),
            Row(
              children: [
                Expanded(
                  child: Slider(
                    value: current.alpha.clamp(0.0, 1.0),
                    min: 0.0,
                    max: 1.0,
                    divisions: 100,
                    label: current.alpha.toStringAsFixed(2),
                    onChanged: (value) => _update(alpha: value.clamp(0.0, 1.0)),
                  ),
                ),
                SizedBox(
                  width: 80,
                  child: FloatInput(
                    label: '',
                    value: current.alpha,
                    onChanged: (value) => _update(alpha: value.clamp(0.0, 1.0)),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text(
              'Stored and saved with the project, but surfaces currently render '
              'fully opaque — transparent isosurface rendering is not in yet. '
              'Setting it now keeps the file correct for when it arrives.',
              style: captionStyle,
            ),
            const SizedBox(height: 16),
            Text(
              'Colormap',
              style: headingStyle?.copyWith(
                color: colorFieldConnected
                    ? null
                    : Theme.of(context).disabledColor,
              ),
            ),
            const SizedBox(height: 4),
            DropdownButtonFormField<APIColormap>(
              key: const Key('isosurface_colormap'),
              decoration: const InputDecoration(
                border: OutlineInputBorder(),
                isDense: true,
                contentPadding:
                    EdgeInsets.symmetric(horizontal: 8, vertical: 8),
              ),
              value: current.colormap,
              items: const [
                DropdownMenuItem(
                  value: APIColormap.blueWhiteRed,
                  child: Text('Blue - White - Red'),
                ),
              ],
              onChanged: colorFieldConnected
                  ? (value) {
                      if (value != null) _update(colormap: value);
                    }
                  : null,
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                Expanded(
                  child: FloatInput(
                    label: 'Range min',
                    value: current.colorMin,
                    enabled: colorFieldConnected,
                    onChanged: (value) => _update(colorMin: value),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: FloatInput(
                    label: 'Range max',
                    value: current.colorMax,
                    enabled: colorFieldConnected,
                    onChanged: (value) => _update(colorMax: value),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text(
              colorFieldConnected
                  ? 'The colormap domain is never fitted automatically: these '
                      'quantities span orders of magnitude near the nuclei, so '
                      'fitting to the extremes paints the whole surface one '
                      'flat color.'
                  : 'Wire a field into the `color_field` input pin to paint the '
                      'surface by a second quantity — a density colored by its '
                      'electrostatic potential, for instance.',
              style: captionStyle,
            ),
          ],
        ),
      ),
    );
  }
}
