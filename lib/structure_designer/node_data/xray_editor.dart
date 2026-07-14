import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Editor widget for the `xray` node — exposes the display `alpha` (0–1) that
/// makes atoms semi-transparent. A slider and a numeric field stay in sync;
/// both write through `model.setXrayData`. When the `alpha` input pin is wired
/// the wired value wins at eval, so the panel shows the stored value only as
/// an inert fallback.
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
    model.setXrayData(nodeId, APIXrayData(alpha: clamped));
  }

  @override
  Widget build(BuildContext context) {
    final alpha = data?.alpha ?? 0.5;
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
              'Wiring the `alpha` input pin overrides this value.',
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
