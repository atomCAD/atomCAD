import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/scene_composer/scene_composer_model.dart';
import 'package:provider/provider.dart';

/// A widget that displays tool selection icons in a horizontal row.
class SceneComposerToolsPanel extends StatelessWidget {
  final SceneComposerModel model;

  const SceneComposerToolsPanel({
    super.key,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Consumer<SceneComposerModel>(
        builder: (context, model, child) {
          // Get tool information from the model
          final sceneComposerView = model.sceneComposerView;

          // If view is null, show a message
          if (sceneComposerView == null) {
            return const Center(child: Text('Loading tools...'));
          }

          // Get available tools and active tool
          final availableTools = sceneComposerView.availableTools;
          final activeTool = sceneComposerView.activeTool;

          return Row(
            mainAxisAlignment: MainAxisAlignment.start,
            children: availableTools.map((tool) {
              // Determine if this tool is active
              final isActive = tool == activeTool;

              // Get icon and tooltip based on tool type
              IconData iconData;
              String tooltip;

              switch (tool) {
                case APISceneComposerTool.default_:
                  iconData = Icons.pan_tool;
                  tooltip = 'Default Tool';
                  break;
                case APISceneComposerTool.align:
                  iconData = Icons.align_horizontal_center;
                  tooltip = 'Align Tool';
                  break;
              }

              // Return a styled button for each tool
              return Padding(
                padding: const EdgeInsets.symmetric(horizontal: 4.0),
                child: Tooltip(
                  message: tooltip,
                  child: Material(
                    color: isActive
                        ? Colors.blue.withOpacity(0.2)
                        : Colors.transparent,
                    borderRadius: BorderRadius.circular(4.0),
                    child: InkWell(
                      borderRadius: BorderRadius.circular(4.0),
                      onTap: () {
                        // Set active tool when clicked
                        model.setActiveTool(tool);
                      },
                      child: Container(
                        padding: const EdgeInsets.all(8.0),
                        decoration: BoxDecoration(
                          border: isActive
                              ? Border.all(color: Colors.blue, width: 2.0)
                              : null,
                          borderRadius: BorderRadius.circular(4.0),
                        ),
                        child: Icon(
                          iconData,
                          color: isActive ? Colors.blue : Colors.black87,
                          size: 24.0,
                        ),
                      ),
                    ),
                  ),
                ),
              );
            }).toList(),
          );
        },
      ),
    );
  }
}
