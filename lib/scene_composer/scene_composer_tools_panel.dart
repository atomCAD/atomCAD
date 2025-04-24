import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/scene_composer_api_types.dart';
import 'package:flutter_cad/scene_composer/scene_composer_model.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/common/ui_common.dart';

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
                case APISceneComposerTool.atomInfo:
                  iconData = Icons.info_outline;
                  tooltip = 'Atom Info Tool';
                  break;
                case APISceneComposerTool.distance:
                  iconData = Icons.straighten;
                  tooltip = 'Distance Tool';
                  break;
              }

              // Return a styled button for each tool
              return Padding(
                padding: const EdgeInsets.symmetric(horizontal: 4.0),
                child: Tooltip(
                  message: tooltip,
                  child: Material(
                    color:
                        isActive ? AppColors.primaryAccent : Colors.transparent,
                    borderRadius: BorderRadius.circular(4.0),
                    child: InkWell(
                      borderRadius: BorderRadius.circular(4.0),
                      onTap: () {
                        // Set active tool when clicked
                        model.setActiveTool(tool);
                      },
                      child: Container(
                        padding: const EdgeInsets.all(8.0),
                        child: Icon(
                          iconData,
                          color: isActive
                              ? AppColors.textOnDark
                              : AppColors.textPrimary,
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
