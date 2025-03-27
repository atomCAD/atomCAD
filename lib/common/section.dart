import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';

class Section extends StatelessWidget {
  final String title;
  final Widget content;
  final bool expand;
  final bool addBottomPadding;

  const Section({
    required this.title,
    required this.content,
    this.expand = false,
    this.addBottomPadding = false,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    // Header with background color
    final titleWidget = Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(
        horizontal: AppSpacing.medium, 
        vertical: AppSpacing.small,
      ),
      decoration: BoxDecoration(
        color: AppColors.sectionHeaderBackground,
        border: Border(
          bottom: BorderSide(
            color: AppColors.dividerColor,
            width: 1.0,
          ),
        ),
      ),
      child: Text(
        title.toUpperCase(),
        style: TextStyle(
          fontSize: 12,
          color: AppColors.sectionHeaderForeground,
          fontWeight: FontWeight.w500,
          letterSpacing: 0.5,
        ),
      ),
    );
    
    final contentPadding = SizedBox(height: AppSpacing.small);
    // Smaller bottom spacing or none if disabled
    final bottomSpacing = addBottomPadding 
        ? SizedBox(height: AppSpacing.small)
        : SizedBox.shrink();
    
    // If expand is true, wrap content in Flexible
    final contentWidget = expand 
        ? Flexible(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                contentPadding,
                Expanded(child: content),
              ],
            ),
          )
        : Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              contentPadding,
              content,
              bottomSpacing,
            ],
          );
    
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        titleWidget,
        contentWidget,
      ],
    );
  }
}
