import 'package:flutter/material.dart';
import 'package:flutter_cad/cad_viewport.dart';
import 'package:flutter_cad/node_network.dart';
import 'package:flutter_cad/src/rust/frb_generated.dart';

Future<void> main() async {
  await RustLib.init();
  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    final graphModel = GraphModel()..init("sample");
    
    return MaterialApp(
      home: Scaffold(
        body: Column(
          children: [
            Expanded(
              flex: 2,
              child: CadViewport(),
            ),
            Expanded(
              flex: 1,
              child: NodeNetwork(graphModel: graphModel),
            ),
          ],
        ),
      ),
    );
  }
}
