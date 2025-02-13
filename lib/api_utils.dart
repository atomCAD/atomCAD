import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:vector_math/vector_math.dart' as vector_math;
import 'package:flutter/material.dart';

vector_math.Vector3 APIVec3ToVector3(APIVec3 v) {
  return vector_math.Vector3(v.x, v.y, v.z);
}

APIVec3 Vector3ToAPIVec3(vector_math.Vector3 v) {
  return APIVec3(x: v.x, y: v.y, z: v.z);
}

Offset APIVec2ToOffset(APIVec2 v) {
  return Offset(v.x, v.y);
}

APIVec2 OffsetToAPIVec2(Offset v) {
  return APIVec2(x: v.dx, y: v.dy);
}
