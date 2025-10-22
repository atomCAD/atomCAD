import 'package:flutter/material.dart';

/// A reusable widget for displaying crystal system information
/// 
/// This widget provides a consistent visual representation of crystal system
/// information across different editors in the application.
class CrystalSystemDisplay extends StatelessWidget {
  final String crystalSystem;
  final String? label;

  const CrystalSystemDisplay({
    super.key,
    required this.crystalSystem,
    this.label,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: Colors.blue.shade50,
        border: Border.all(color: Colors.blue.shade200),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Row(
        children: [
          Icon(
            Icons.grain,
            color: Colors.blue.shade700,
            size: 20,
          ),
          const SizedBox(width: 8),
          Text(
            label ?? 'Crystal System: ',
            style: TextStyle(
              fontWeight: FontWeight.w500,
              color: Colors.blue.shade700,
            ),
          ),
          Text(
            crystalSystem,
            style: TextStyle(
              fontWeight: FontWeight.bold,
              color: Colors.blue.shade800,
            ),
          ),
        ],
      ),
    );
  }
}
