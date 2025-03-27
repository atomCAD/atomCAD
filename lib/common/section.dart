import 'package:flutter/material.dart';

class Section extends StatelessWidget {
  final String title;
  final Widget content;

  const Section({
    required this.title,
    required this.content,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 8.0),
          child: Text(
            title,
            style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
          ),
        ),
        Divider(),
        content,
        SizedBox(height: 16), // Spacing between sections
      ],
    );
  }
}
