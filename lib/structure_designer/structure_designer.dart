import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_viewport.dart';
import 'package:flutter_cad/structure_designer/node_network.dart';
import 'package:flutter_cad/structure_designer/graph_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_data_widget.dart';

/// The structure designer editor.
class StructureDesigner extends StatefulWidget {
  const StructureDesigner({super.key});

  @override
  State<StructureDesigner> createState() => _StructureDesignerState();
}

class _StructureDesignerState extends State<StructureDesigner> {
  late GraphModel graphModel;

  @override
  void initState() {
    super.initState();
    graphModel = GraphModel();
  }

  @override
  Widget build(BuildContext context) {
    // Initialize the graph model here
    graphModel.init("sample");

    return Column(
      children: [
        Center(
          child: SizedBox(
            width: 1280,
            height: 544,
            child: StructureDesignerViewport(graphModel: graphModel),
          ),
        ),
        Expanded(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(
                flex: 4,
                child: NodeNetwork(graphModel: graphModel),
              ),
              Container(
                width: 300,
                padding: const EdgeInsets.all(8.0),
                decoration: const BoxDecoration(
                  border: Border(
                    left: BorderSide(
                      color: Colors.grey,
                      width: 1,
                    ),
                  ),
                ),
                child: NodeDataWidget(graphModel: graphModel),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
