import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';

/// A reusable column header widget for table-like displays
/// Uses the application's common styling from ui_common.dart
class TableColumnHeader extends StatelessWidget {
  final String title;
  final double? width;
  final TextAlign textAlign;

  const TableColumnHeader({
    Key? key,
    required this.title,
    this.width,
    this.textAlign = TextAlign.center,
  }) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return Container(
      width: width,
      padding: EdgeInsets.symmetric(
        vertical: AppSpacing.small, 
        horizontal: AppSpacing.small,
      ),
      decoration: BoxDecoration(
        color: AppColors.sectionHeaderBackground,
        border: const Border(
          bottom: BorderSide(
            color: AppColors.dividerColor,
            width: 1.0,
          ),
        ),
      ),
      child: Text(
        title,
        textAlign: textAlign,
        style: AppTextStyles.small.copyWith(
          color: AppColors.sectionHeaderForeground,
          fontWeight: FontWeight.bold,
        ),
      ),
    );
  }
}
