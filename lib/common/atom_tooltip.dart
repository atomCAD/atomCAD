import 'package:flutter/widgets.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

class AtomTooltip extends StatelessWidget {
  const AtomTooltip({super.key, required this.info});

  final APIHoveredAtomInfo info;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
      constraints: const BoxConstraints(maxWidth: 220),
      decoration: BoxDecoration(
        color: const Color(0xDD303030),
        borderRadius: BorderRadius.circular(4),
        border: Border.all(color: const Color(0x88FFFFFF), width: 0.5),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          // Line 1: element identity — bold, accent color
          Text(
            '${info.symbol} (${info.elementName})',
            style: const TextStyle(
              color: Color(0xFF4FC3F7),
              fontSize: 13,
              fontWeight: FontWeight.w600,
              decoration: TextDecoration.none,
            ),
          ),
          const SizedBox(height: 2),
          // Line 2: bond count
          Text(
            '${info.bondCount} bond${info.bondCount == 1 ? '' : 's'}',
            style: const TextStyle(
              color: Color(0xCCFFFFFF),
              fontSize: 11,
              fontWeight: FontWeight.normal,
              decoration: TextDecoration.none,
            ),
          ),
          // Line 3: position in Angstroms (3 decimal places)
          Text(
            'Pos: (${info.x.toStringAsFixed(3)}, '
            '${info.y.toStringAsFixed(3)}, '
            '${info.z.toStringAsFixed(3)}) \u00c5',
            style: const TextStyle(
              color: Color(0x99FFFFFF),
              fontSize: 11,
              fontWeight: FontWeight.normal,
              decoration: TextDecoration.none,
            ),
          ),
        ],
      ),
    );
  }
}
