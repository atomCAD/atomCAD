import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Upper bound of the opaque-depth slider (Å). A few unit cells is the useful
/// range — a diamond cell is ~3.6 Å, so this spans roughly four of them. The
/// numeric field is unbounded for the rare deeper case.
const double _MAX_OPAQUE_DEPTH_SLIDER = 16.0;

/// Editor widget for the `xray` node — exposes the display `alpha` (0–1) that
/// makes atoms semi-transparent, plus the `opaque_depth` (Å) depth ramp that
/// fades atoms back to opaque below the crystal surface. A slider and a numeric
/// field stay in sync; both write through `model.setXrayData`. When an input
/// pin is wired the wired value wins at eval, so the panel shows the stored
/// value only as an inert fallback.
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
      APIXrayData(alpha: clamped, opaqueDepth: data?.opaqueDepth ?? 0.0),
    );
  }

  void _setOpaqueDepth(double value) {
    // Negative is meaningless; the Rust side treats anything <= 0 as "ramp
    // off", so normalize to exactly 0 for a stable round-trip through the field.
    final clamped = value < 0.0 ? 0.0 : value;
    model.setXrayData(
      nodeId,
      APIXrayData(alpha: data?.alpha ?? 0.5, opaqueDepth: clamped),
    );
  }

  @override
  Widget build(BuildContext context) {
    final alpha = data?.alpha ?? 0.5;
    final opaqueDepth = data?.opaqueDepth ?? 0.0;
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
              opaqueDepth > 0.0
                  ? '0 = fully transparent, 1 = fully opaque. With a depth ramp '
                      'set below, this is the alpha at the crystal surface. '
                      'Wiring the `alpha` input pin overrides this value.'
                  : '0 = fully transparent, 1 = fully opaque (restores the atoms). '
                      'Wiring the `alpha` input pin overrides this value.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 16),
            Text('Opaque depth', style: Theme.of(context).textTheme.titleSmall),
            Row(
              children: [
                Expanded(
                  child: Slider(
                    value: opaqueDepth.clamp(0.0, _MAX_OPAQUE_DEPTH_SLIDER),
                    min: 0.0,
                    max: _MAX_OPAQUE_DEPTH_SLIDER,
                    divisions: 80,
                    label: opaqueDepth <= 0.0
                        ? 'off'
                        : '${opaqueDepth.toStringAsFixed(2)} Å',
                    onChanged: _setOpaqueDepth,
                  ),
                ),
                SizedBox(
                  width: 80,
                  child: FloatInput(
                    label: '',
                    value: opaqueDepth,
                    onChanged: _setOpaqueDepth,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text(
              opaqueDepth > 0.0
                  ? 'Atoms fade from the surface alpha to fully opaque '
                      '${opaqueDepth.toStringAsFixed(2)} Å below the crystal surface — '
                      'a thin see-through skin over a solid core. Set 0 to apply '
                      'the alpha uniformly instead.'
                  : 'Off: the alpha is applied uniformly at every depth. Set a '
                      'depth (Å) to fade atoms back to opaque below the surface, '
                      'which avoids both the transparent fog and the hollow '
                      'interior left by depth culling.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 4),
            Text(
              'The ramp only affects atoms that carry a crystal depth (those '
              'built by a lattice fill). Imported or hand-placed atoms count as '
              'surface atoms and keep the surface alpha. The slider caps at '
              '${_MAX_OPAQUE_DEPTH_SLIDER.toStringAsFixed(0)} Å; type a larger '
              'value in the field if you need one.',
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
