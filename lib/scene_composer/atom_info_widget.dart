import 'package:flutter/material.dart';
import 'package:flutter_cad/scene_composer/scene_composer_model.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:provider/provider.dart';

/// Widget that displays information about the selected atom in the scene composer.
class AtomInfoWidget extends StatelessWidget {
  final SceneComposerModel model;

  const AtomInfoWidget({
    Key? key,
    required this.model,
  }) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Consumer<SceneComposerModel>(
        builder: (context, sceneModel, child) {
          final AtomView? atomInfo = sceneModel.atomInfoView;
          
          if (atomInfo == null) {
            return const Center(
              child: Text(
                'No atom selected',
                style: TextStyle(fontStyle: FontStyle.italic),
              ),
            );
          }
          
          return Padding(
            padding: const EdgeInsets.all(8.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _buildInfoRow('ID:', atomInfo.id.toString()),
                _buildInfoRow('Element:', '${atomInfo.symbol} (${atomInfo.elementName})'),
                _buildInfoRow('Atomic Number:', atomInfo.atomicNumber.toString()),
                _buildInfoRow('Covalent radius:', atomInfo.covalentRadius.toString()),
                _buildInfoRow('Cluster:', '${atomInfo.clusterName} (${atomInfo.clusterId})'),
                const SizedBox(height: 8),
                Text(
                  'Position:',
                  style: TextStyle(
                    fontWeight: FontWeight.bold,
                    color: AppColors.textSecondary,
                  ),
                ),
                _buildPositionRow(atomInfo.position),
              ],
            ),
          );
        },
      ),
    );
  }

  Widget _buildInfoRow(String label, String value) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 4.0),
      child: RichText(
        text: TextSpan(
          style: TextStyle(
            color: AppColors.textPrimary,
            fontSize: 14,
          ),
          children: [
            TextSpan(
              text: '$label ',
              style: TextStyle(
                fontWeight: FontWeight.bold,
                color: AppColors.textSecondary,
              ),
            ),
            TextSpan(text: value),
          ],
        ),
      ),
    );
  }

  Widget _buildPositionRow(APIVec3 position) {
    return Padding(
      padding: const EdgeInsets.only(left: 16.0),
      child: Row(
        children: [
          _buildPositionValue('X', position.x),
          const SizedBox(width: 16),
          _buildPositionValue('Y', position.y),
          const SizedBox(width: 16),
          _buildPositionValue('Z', position.z),
        ],
      ),
    );
  }

  Widget _buildPositionValue(String axis, double value) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8.0, vertical: 4.0),
      decoration: BoxDecoration(
        border: Border.all(color: Colors.grey.shade300),
        borderRadius: BorderRadius.circular(4.0),
      ),
      child: Text(
        '$axis: ${value.toStringAsFixed(6)}',
        style: const TextStyle(
          fontFamily: 'monospace',
          fontSize: 12,
        ),
      ),
    );
  }
}
