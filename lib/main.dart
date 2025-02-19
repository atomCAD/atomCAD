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
            Center(
              child: SizedBox(
                width: 1280,
                height: 544,
                child: CadViewport(),
              ),
            ),
            Expanded(
              child: NodeNetwork(graphModel: graphModel),
            ),
          ],
        ),
      ),
    );
  }
}
