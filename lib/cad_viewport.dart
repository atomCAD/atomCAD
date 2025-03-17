import 'package:flutter/material.dart';

abstract class CadViewport extends StatefulWidget {
  const CadViewport({Key? key}) : super(key: key);
}

abstract class CadViewportState<T extends CadViewport> extends State<T> {
  // Place common state methods, properties, and lifecycle logic here.
}
