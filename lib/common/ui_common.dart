import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';

/// UI Common styling for the Flutter CAD application
///
/// This file contains shared styling elements, colors, and themes
/// to maintain consistency across the application.

/// App Colors
class AppColors {
  // Primary accent colors
  static final primaryAccent = Colors.blueGrey[700];
  static final primaryAccentLight = Colors.blueGrey[500];
  static final primaryAccentDark = Colors.blueGrey[800];

  // Axis colors
  static const xAxisColor = Colors.deepOrange;
  static const yAxisColor = Colors.green;
  static const zAxisColor = Colors.blue;

  // Text colors
  static final textPrimary = Colors.grey[900];
  static final textSecondary = Colors.grey[700];
  static final textOnDark = Colors.white;

  // Selection colors
  static final selectionBackground = primaryAccent;
  static final selectionForeground = Colors.white;

  // Divider colors
  static const dividerColor = Colors.black12;

  // Background colors
  static final sectionHeaderBackground = Colors.grey[600];
  static final sectionHeaderForeground = Colors.white;
}

/// Text Styles
class AppTextStyles {
  // Regular text
  static const regular = TextStyle(fontSize: 14);
  static const small = TextStyle(fontSize: 13);

  // Widget labels
  static const label = TextStyle(fontSize: 13);
  static const inputField = TextStyle(fontSize: 14);

  // Button text
  static const buttonText = TextStyle(
    fontSize: 13,
    fontWeight: FontWeight.w500,
  );
}

/// Layouts and Spacing
class AppSpacing {
  // General padding
  static const small = 4.0;
  static const medium = 8.0;
  static const large = 16.0;

  // Widget specific padding
  static const fieldContentPadding = EdgeInsets.symmetric(
    horizontal: 6,
    vertical: 2,
  );

  // Visual density modifiers
  static const compactVerticalDensity = VisualDensity(vertical: -2);

  // Standard widget sizes
  static const buttonHeight = 28.0;
  static const smallButtonWidth = 66.0;
  static const labelWidth = 48.0;
}

/// Button Styles
class AppButtonStyles {
  // Primary button style
  static final primary = ElevatedButton.styleFrom(
    backgroundColor: AppColors.primaryAccent,
    foregroundColor: AppColors.textOnDark,
    elevation: 2,
    shadowColor: AppColors.primaryAccentDark,
    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 0),
    textStyle: AppTextStyles.buttonText,
    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(4)),
  );

  // Disabled button style (useful for maintaining consistent height)
  static final disabled = ElevatedButton.styleFrom(
    backgroundColor: Colors.grey[300],
    foregroundColor: Colors.grey[600],
    elevation: 0,
    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 0),
    textStyle: AppTextStyles.buttonText,
    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(4)),
  );
}

/// Input Field Decorations
class AppInputDecorations {
  static const standard = InputDecoration(
    border: OutlineInputBorder(),
    contentPadding: AppSpacing.fieldContentPadding,
    isDense: true,
    labelStyle: AppTextStyles.label,
  );
}

SelectModifier getSelectModifierFromKeyboard() {
  return HardwareKeyboard.instance.isControlPressed
      ? SelectModifier.toggle
      : HardwareKeyboard.instance.isShiftPressed
          ? SelectModifier.expand
          : SelectModifier.replace;
}
