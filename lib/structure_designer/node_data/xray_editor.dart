import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Upper bound of the fade-depth slider (Å). A few unit cells is the useful
/// range — a diamond cell is ~3.6 Å, so this spans roughly four of them. The
/// numeric field is unbounded for the rare deeper case.
const double _MAX_FADE_DEPTH_SLIDER = 16.0;

/// Editor widget for the `xray` node — exposes the display `alpha` (0–1) that
/// makes atoms semi-transparent, plus the `fade_depth` (Å) ramp that fades
/// atoms out to fully transparent with depth below the crystal surface. A
/// slider and a numeric field stay in sync; both write through
/// `model.setXrayData`. When an input pin is wired the wired value wins at
/// eval, so the panel shows the stored value only as an inert fallback.
class XrayEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIXrayData? data;
  final StructureDesignerModel model;

  const XrayEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  void _setAlpha(double value) {
    // Clamp to the valid range before crossing the FFI boundary (the Rust side
    // clamps at eval too, but keep the panel honest).
    final clamped = value.clamp(0.0, 1.0);
    model.setXrayData(
      nodeId,
      APIXrayData(alpha: clamped, fadeDepth: data?.fadeDepth ?? 0.0),
    );
  }

  void _setFadeDepth(double value) {
    // Negative is meaningless; the Rust side treats anything <= 0 as "ramp
    // off", so normalize to exactly 0 for a stable round-trip through the field.
    final clamped = value < 0.0 ? 0.0 : value;
    model.setXrayData(
      nodeId,
      APIXrayData(alpha: data?.alpha ?? 0.5, fadeDepth: clamped),
    );
  }

  @override
  Widget build(BuildContext context) {
    final alpha = data?.alpha ?? 0.5;
    final fadeDepth = data?.fadeDepth ?? 0.0;
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'X-Ray Transparency',
            nodeTypeName: 'xray',
          ),
          const SizedBox(height: 16),
          if (data != null) ...[
            Text('Alpha', style: Theme.of(context).textTheme.titleSmall),
            Row(
              children: [
                Expanded(
                  child: Slider(
                    value: alpha.clamp(0.0, 1.0),
                    min: 0.0,
                    max: 1.0,
                    divisions: 100,
                    label: alpha.toStringAsFixed(2),
                    onChanged: _setAlpha,
                  ),
                ),
                SizedBox(
                  width: 80,
                  child: FloatInput(
                    label: '',
                    value: alpha,
                    onChanged: _setAlpha,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text(
              '0 = fully transparent, 1 = fully opaque (restores the atoms). '
              'With a fade depth set, this is the alpha at the surface. '
              'Wiring the `alpha` input pin overrides this value.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 16),
            Text('Fade depth', style: Theme.of(context).textTheme.titleSmall),
            Row(
              children: [
                Expanded(
                  child: Slider(
                    value: fadeDepth.clamp(0.0, _MAX_FADE_DEPTH_SLIDER),
                    min: 0.0,
                    max: _MAX_FADE_DEPTH_SLIDER,
                    divisions: 80,
                    label: fadeDepth <= 0.0
                        ? 'off'
                        : '${fadeDepth.toStringAsFixed(2)} Å',
                    onChanged: _setFadeDepth,
                  ),
                ),
                SizedBox(
                  width: 80,
                  child: FloatInput(
                    label: '',
                    value: fadeDepth,
                    onChanged: _setFadeDepth,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text(
              'Atoms fade out to fully transparent this far below the crystal '
              'surface, leaving a thin shell instead of a deep fog. 0 = apply '
              'the alpha at every depth. Only lattice-filled atoms carry a depth.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 8),
            Text(
              'Transparency renders in impostor atomic rendering mode only; '
              'in triangle-mesh mode atoms stay opaque.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ],
        ],
      ),
    );
  }
}
