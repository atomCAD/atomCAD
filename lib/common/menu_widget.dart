import 'package:flutter/material.dart';

/// A reusable menu widget for the menu bar
class MenuWidget extends StatelessWidget {
  final String label;
  final List<Widget> menuItems;

  const MenuWidget({
    super.key,
    required this.label,
    required this.menuItems,
  });

  @override
  Widget build(BuildContext context) {
    return MenuAnchor(
      builder: (context, controller, child) {
        return TextButton(
          onPressed: () {
            if (controller.isOpen) {
              controller.close();
            } else {
              controller.open();
            }
          },
          style: TextButton.styleFrom(
            foregroundColor: Colors.black87,
            padding: const EdgeInsets.symmetric(horizontal: 16),
          ),
          child: Text(label),
        );
      },
      menuChildren: menuItems,
    );
  }
}
