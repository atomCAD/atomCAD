import 'package:flutter/material.dart';
import 'package:flutter_cad/cad_viewport.dart';
import 'package:flutter_cad/node_network.dart';
import 'package:flutter_cad/src/rust/frb_generated.dart';

Future<void> main() async {
  await RustLib.init();
  //runApp(const MyApp());

  final graphModel = GraphModel();
  graphModel.init("sample");

  runApp(MaterialApp(
    home: Scaffold(
      body: NodeNetwork(graphModel: graphModel),
    ),
  ));
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        appBar: AppBar(title: const Text('flutter atomCAD test')),
        body: Center(
          //child: Text(
          //    'Action: Call Rust `greet("Tom")`\nResult: `${greet(name: "Tom")}`'),
          child: CadViewport(),
        ),
      ),
    );
  }
}
