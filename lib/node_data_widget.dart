import 'package:flutter/material.dart';
import 'package:flutter_cad/graph_model.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';
import 'package:provider/provider.dart';

/// A widget that displays and allows editing of node-specific data
/// based on the currently selected node in the graph.
class NodeDataWidget extends StatelessWidget {
  final GraphModel graphModel;

  const NodeDataWidget({
    super.key,
    required this.graphModel,
  });

  @override
  Widget build(BuildContext context) {
    // Listen to changes in the graph model
    return ChangeNotifierProvider.value(
      value: graphModel,
      child: Consumer<GraphModel>(
        builder: (context, model, child) {
          final nodeNetworkView = model.nodeNetworkView;
          if (nodeNetworkView == null) return const SizedBox.shrink();

          // Find the selected node
          final selectedNode = nodeNetworkView.nodes.entries
              .where((entry) => entry.value.selected)
              .map((entry) => entry.value)
              .firstOrNull;

          if (selectedNode == null) {
            return const Center(
              child: Text('No node selected'),
            );
          }

          // Based on the node type, show the appropriate editor
          switch (selectedNode.nodeTypeName) {
            case 'cuboid':
              return _CuboidEditor(
                nodeNetworkName: nodeNetworkView.name,
                nodeId: selectedNode.id,
              );
            case 'sphere':
              return _SphereEditor(
                nodeNetworkName: nodeNetworkView.name,
                nodeId: selectedNode.id,
              );
            case 'half_space':
              return _HalfSpaceEditor(
                nodeNetworkName: nodeNetworkView.name,
                nodeId: selectedNode.id,
              );
            default:
              return Center(
                child: Text(
                    'No editor available for ${selectedNode.nodeTypeName}'),
              );
          }
        },
      ),
    );
  }
}

/// Editor widget for Cuboid nodes
class _CuboidEditor extends StatefulWidget {
  final String nodeNetworkName;
  final BigInt nodeId;

  const _CuboidEditor({
    required this.nodeNetworkName,
    required this.nodeId,
  });

  @override
  State<_CuboidEditor> createState() => _CuboidEditorState();
}

class _CuboidEditorState extends State<_CuboidEditor> {
  APICuboidData? _data;

  @override
  void initState() {
    super.initState();
    _loadData();
  }

  Future<void> _loadData() async {
    final data = getCuboidData(
      nodeNetworkName: widget.nodeNetworkName,
      nodeId: widget.nodeId,
    );
    if (mounted) {
      setState(() => _data = data);
    }
  }

  void _updateData(APICuboidData newData) {
    setCuboidData(
      nodeNetworkName: widget.nodeNetworkName,
      nodeId: widget.nodeId,
      data: newData,
    );
    setState(() => _data = newData);
  }

  @override
  Widget build(BuildContext context) {
    if (_data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Cuboid Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          _Vec3Input(
            label: 'Min Corner',
            value: _data!.minCorner,
            onChanged: (newValue) {
              _updateData(APICuboidData(
                minCorner: newValue,
                extent: _data!.extent,
              ));
            },
          ),
          const SizedBox(height: 8),
          _Vec3Input(
            label: 'Extent',
            value: _data!.extent,
            onChanged: (newValue) {
              _updateData(APICuboidData(
                minCorner: _data!.minCorner,
                extent: newValue,
              ));
            },
          ),
        ],
      ),
    );
  }
}

/// Editor widget for Sphere nodes
class _SphereEditor extends StatefulWidget {
  final String nodeNetworkName;
  final BigInt nodeId;

  const _SphereEditor({
    required this.nodeNetworkName,
    required this.nodeId,
  });

  @override
  State<_SphereEditor> createState() => _SphereEditorState();
}

class _SphereEditorState extends State<_SphereEditor> {
  APISphereData? _data;

  @override
  void initState() {
    super.initState();
    _loadData();
  }

  Future<void> _loadData() async {
    final data = getSphereData(
      nodeNetworkName: widget.nodeNetworkName,
      nodeId: widget.nodeId,
    );
    if (mounted) {
      setState(() => _data = data);
    }
  }

  void _updateData(APISphereData newData) {
    setSphereData(
      nodeNetworkName: widget.nodeNetworkName,
      nodeId: widget.nodeId,
      data: newData,
    );
    setState(() => _data = newData);
  }

  @override
  Widget build(BuildContext context) {
    if (_data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Sphere Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          _Vec3Input(
            label: 'Center',
            value: _data!.center,
            onChanged: (newValue) {
              _updateData(APISphereData(
                center: newValue,
                radius: _data!.radius,
              ));
            },
          ),
          const SizedBox(height: 8),
          _IntInput(
            label: 'Radius',
            value: _data!.radius,
            onChanged: (newValue) {
              _updateData(APISphereData(
                center: _data!.center,
                radius: newValue,
              ));
            },
          ),
        ],
      ),
    );
  }
}

/// Editor widget for HalfSpace nodes
class _HalfSpaceEditor extends StatefulWidget {
  final String nodeNetworkName;
  final BigInt nodeId;

  const _HalfSpaceEditor({
    required this.nodeNetworkName,
    required this.nodeId,
  });

  @override
  State<_HalfSpaceEditor> createState() => _HalfSpaceEditorState();
}

class _HalfSpaceEditorState extends State<_HalfSpaceEditor> {
  APIHalfSpaceData? _data;

  @override
  void initState() {
    super.initState();
    _loadData();
  }

  Future<void> _loadData() async {
    final data = getHalfSpaceData(
      nodeNetworkName: widget.nodeNetworkName,
      nodeId: widget.nodeId,
    );
    if (mounted) {
      setState(() => _data = data);
    }
  }

  void _updateData(APIHalfSpaceData newData) {
    setHalfSpaceData(
      nodeNetworkName: widget.nodeNetworkName,
      nodeId: widget.nodeId,
      data: newData,
    );
    setState(() => _data = newData);
  }

  @override
  Widget build(BuildContext context) {
    if (_data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Half Space Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          _Vec3Input(
            label: 'Miller Index',
            value: _data!.millerIndex,
            onChanged: (newValue) {
              _updateData(APIHalfSpaceData(
                millerIndex: newValue,
                shift: _data!.shift,
              ));
            },
          ),
          const SizedBox(height: 8),
          _IntInput(
            label: 'Shift',
            value: _data!.shift,
            onChanged: (newValue) {
              _updateData(APIHalfSpaceData(
                millerIndex: _data!.millerIndex,
                shift: newValue,
              ));
            },
          ),
        ],
      ),
    );
  }
}

/// A reusable widget for editing Vec3 values
class _Vec3Input extends StatelessWidget {
  final String label;
  final APIIVec3 value;
  final ValueChanged<APIIVec3> onChanged;

  const _Vec3Input({
    required this.label,
    required this.value,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label),
        Row(
          children: [
            Expanded(
              child: TextField(
                decoration: const InputDecoration(labelText: 'X'),
                controller: TextEditingController(text: value.x.toString()),
                keyboardType: TextInputType.number,
                onChanged: (text) {
                  final newValue = int.tryParse(text) ?? value.x;
                  onChanged(APIIVec3(x: newValue, y: value.y, z: value.z));
                },
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                decoration: const InputDecoration(labelText: 'Y'),
                controller: TextEditingController(text: value.y.toString()),
                keyboardType: TextInputType.number,
                onChanged: (text) {
                  final newValue = int.tryParse(text) ?? value.y;
                  onChanged(APIIVec3(x: value.x, y: newValue, z: value.z));
                },
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                decoration: const InputDecoration(labelText: 'Z'),
                controller: TextEditingController(text: value.z.toString()),
                keyboardType: TextInputType.number,
                onChanged: (text) {
                  final newValue = int.tryParse(text) ?? value.z;
                  onChanged(APIIVec3(x: value.x, y: value.y, z: newValue));
                },
              ),
            ),
          ],
        ),
      ],
    );
  }
}

/// A reusable widget for editing integer values
class _IntInput extends StatelessWidget {
  final String label;
  final int value;
  final ValueChanged<int> onChanged;

  const _IntInput({
    required this.label,
    required this.value,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label),
        TextField(
          decoration: const InputDecoration(
            border: OutlineInputBorder(),
          ),
          controller: TextEditingController(text: value.toString()),
          keyboardType: TextInputType.number,
          onChanged: (text) {
            final newValue = int.tryParse(text);
            if (newValue != null) {
              onChanged(newValue);
            }
          },
        ),
      ],
    );
  }
}
