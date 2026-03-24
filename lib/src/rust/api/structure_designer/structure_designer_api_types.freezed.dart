// dart format width=80
// coverage:ignore-file
// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'structure_designer_api_types.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;

/// @nodoc
mixin _$APIMeasurement {
  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is APIMeasurement);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'APIMeasurement()';
  }
}

/// @nodoc
class $APIMeasurementCopyWith<$Res> {
  $APIMeasurementCopyWith(APIMeasurement _, $Res Function(APIMeasurement) __);
}

/// @nodoc

class APIMeasurement_Distance extends APIMeasurement {
  const APIMeasurement_Distance(
      {required this.distance,
      required this.atom1Id,
      required this.atom2Id,
      required this.atom1Symbol,
      required this.atom2Symbol,
      required this.isBonded})
      : super._();

  final double distance;

  /// Result-space atom IDs for the two atoms.
  final int atom1Id;
  final int atom2Id;

  /// Element symbols for display labels.
  final String atom1Symbol;
  final String atom2Symbol;

  /// Whether the two atoms are bonded (enables Default button in dialog).
  final bool isBonded;

  /// Create a copy of APIMeasurement
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $APIMeasurement_DistanceCopyWith<APIMeasurement_Distance> get copyWith =>
      _$APIMeasurement_DistanceCopyWithImpl<APIMeasurement_Distance>(
          this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is APIMeasurement_Distance &&
            (identical(other.distance, distance) ||
                other.distance == distance) &&
            (identical(other.atom1Id, atom1Id) || other.atom1Id == atom1Id) &&
            (identical(other.atom2Id, atom2Id) || other.atom2Id == atom2Id) &&
            (identical(other.atom1Symbol, atom1Symbol) ||
                other.atom1Symbol == atom1Symbol) &&
            (identical(other.atom2Symbol, atom2Symbol) ||
                other.atom2Symbol == atom2Symbol) &&
            (identical(other.isBonded, isBonded) ||
                other.isBonded == isBonded));
  }

  @override
  int get hashCode => Object.hash(runtimeType, distance, atom1Id, atom2Id,
      atom1Symbol, atom2Symbol, isBonded);

  @override
  String toString() {
    return 'APIMeasurement.distance(distance: $distance, atom1Id: $atom1Id, atom2Id: $atom2Id, atom1Symbol: $atom1Symbol, atom2Symbol: $atom2Symbol, isBonded: $isBonded)';
  }
}

/// @nodoc
abstract mixin class $APIMeasurement_DistanceCopyWith<$Res>
    implements $APIMeasurementCopyWith<$Res> {
  factory $APIMeasurement_DistanceCopyWith(APIMeasurement_Distance value,
          $Res Function(APIMeasurement_Distance) _then) =
      _$APIMeasurement_DistanceCopyWithImpl;
  @useResult
  $Res call(
      {double distance,
      int atom1Id,
      int atom2Id,
      String atom1Symbol,
      String atom2Symbol,
      bool isBonded});
}

/// @nodoc
class _$APIMeasurement_DistanceCopyWithImpl<$Res>
    implements $APIMeasurement_DistanceCopyWith<$Res> {
  _$APIMeasurement_DistanceCopyWithImpl(this._self, this._then);

  final APIMeasurement_Distance _self;
  final $Res Function(APIMeasurement_Distance) _then;

  /// Create a copy of APIMeasurement
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? distance = null,
    Object? atom1Id = null,
    Object? atom2Id = null,
    Object? atom1Symbol = null,
    Object? atom2Symbol = null,
    Object? isBonded = null,
  }) {
    return _then(APIMeasurement_Distance(
      distance: null == distance
          ? _self.distance
          : distance // ignore: cast_nullable_to_non_nullable
              as double,
      atom1Id: null == atom1Id
          ? _self.atom1Id
          : atom1Id // ignore: cast_nullable_to_non_nullable
              as int,
      atom2Id: null == atom2Id
          ? _self.atom2Id
          : atom2Id // ignore: cast_nullable_to_non_nullable
              as int,
      atom1Symbol: null == atom1Symbol
          ? _self.atom1Symbol
          : atom1Symbol // ignore: cast_nullable_to_non_nullable
              as String,
      atom2Symbol: null == atom2Symbol
          ? _self.atom2Symbol
          : atom2Symbol // ignore: cast_nullable_to_non_nullable
              as String,
      isBonded: null == isBonded
          ? _self.isBonded
          : isBonded // ignore: cast_nullable_to_non_nullable
              as bool,
    ));
  }
}

/// @nodoc

class APIMeasurement_Angle extends APIMeasurement {
  const APIMeasurement_Angle(
      {required this.angleDegrees,
      required this.vertexId,
      required this.vertexSymbol,
      required this.armAId,
      required this.armASymbol,
      required this.armBId,
      required this.armBSymbol})
      : super._();

  final double angleDegrees;

  /// Vertex atom identity.
  final int vertexId;
  final String vertexSymbol;

  /// Arm atoms (indices 0 and 1 for move choice).
  final int armAId;
  final String armASymbol;
  final int armBId;
  final String armBSymbol;

  /// Create a copy of APIMeasurement
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $APIMeasurement_AngleCopyWith<APIMeasurement_Angle> get copyWith =>
      _$APIMeasurement_AngleCopyWithImpl<APIMeasurement_Angle>(
          this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is APIMeasurement_Angle &&
            (identical(other.angleDegrees, angleDegrees) ||
                other.angleDegrees == angleDegrees) &&
            (identical(other.vertexId, vertexId) ||
                other.vertexId == vertexId) &&
            (identical(other.vertexSymbol, vertexSymbol) ||
                other.vertexSymbol == vertexSymbol) &&
            (identical(other.armAId, armAId) || other.armAId == armAId) &&
            (identical(other.armASymbol, armASymbol) ||
                other.armASymbol == armASymbol) &&
            (identical(other.armBId, armBId) || other.armBId == armBId) &&
            (identical(other.armBSymbol, armBSymbol) ||
                other.armBSymbol == armBSymbol));
  }

  @override
  int get hashCode => Object.hash(runtimeType, angleDegrees, vertexId,
      vertexSymbol, armAId, armASymbol, armBId, armBSymbol);

  @override
  String toString() {
    return 'APIMeasurement.angle(angleDegrees: $angleDegrees, vertexId: $vertexId, vertexSymbol: $vertexSymbol, armAId: $armAId, armASymbol: $armASymbol, armBId: $armBId, armBSymbol: $armBSymbol)';
  }
}

/// @nodoc
abstract mixin class $APIMeasurement_AngleCopyWith<$Res>
    implements $APIMeasurementCopyWith<$Res> {
  factory $APIMeasurement_AngleCopyWith(APIMeasurement_Angle value,
          $Res Function(APIMeasurement_Angle) _then) =
      _$APIMeasurement_AngleCopyWithImpl;
  @useResult
  $Res call(
      {double angleDegrees,
      int vertexId,
      String vertexSymbol,
      int armAId,
      String armASymbol,
      int armBId,
      String armBSymbol});
}

/// @nodoc
class _$APIMeasurement_AngleCopyWithImpl<$Res>
    implements $APIMeasurement_AngleCopyWith<$Res> {
  _$APIMeasurement_AngleCopyWithImpl(this._self, this._then);

  final APIMeasurement_Angle _self;
  final $Res Function(APIMeasurement_Angle) _then;

  /// Create a copy of APIMeasurement
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? angleDegrees = null,
    Object? vertexId = null,
    Object? vertexSymbol = null,
    Object? armAId = null,
    Object? armASymbol = null,
    Object? armBId = null,
    Object? armBSymbol = null,
  }) {
    return _then(APIMeasurement_Angle(
      angleDegrees: null == angleDegrees
          ? _self.angleDegrees
          : angleDegrees // ignore: cast_nullable_to_non_nullable
              as double,
      vertexId: null == vertexId
          ? _self.vertexId
          : vertexId // ignore: cast_nullable_to_non_nullable
              as int,
      vertexSymbol: null == vertexSymbol
          ? _self.vertexSymbol
          : vertexSymbol // ignore: cast_nullable_to_non_nullable
              as String,
      armAId: null == armAId
          ? _self.armAId
          : armAId // ignore: cast_nullable_to_non_nullable
              as int,
      armASymbol: null == armASymbol
          ? _self.armASymbol
          : armASymbol // ignore: cast_nullable_to_non_nullable
              as String,
      armBId: null == armBId
          ? _self.armBId
          : armBId // ignore: cast_nullable_to_non_nullable
              as int,
      armBSymbol: null == armBSymbol
          ? _self.armBSymbol
          : armBSymbol // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class APIMeasurement_Dihedral extends APIMeasurement {
  const APIMeasurement_Dihedral(
      {required this.angleDegrees,
      required this.chainAId,
      required this.chainASymbol,
      required this.chainBId,
      required this.chainBSymbol,
      required this.chainCId,
      required this.chainCSymbol,
      required this.chainDId,
      required this.chainDSymbol})
      : super._();

  final double angleDegrees;

  /// Chain A-B-C-D atom identities.
  final int chainAId;
  final String chainASymbol;
  final int chainBId;
  final String chainBSymbol;
  final int chainCId;
  final String chainCSymbol;
  final int chainDId;
  final String chainDSymbol;

  /// Create a copy of APIMeasurement
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $APIMeasurement_DihedralCopyWith<APIMeasurement_Dihedral> get copyWith =>
      _$APIMeasurement_DihedralCopyWithImpl<APIMeasurement_Dihedral>(
          this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is APIMeasurement_Dihedral &&
            (identical(other.angleDegrees, angleDegrees) ||
                other.angleDegrees == angleDegrees) &&
            (identical(other.chainAId, chainAId) ||
                other.chainAId == chainAId) &&
            (identical(other.chainASymbol, chainASymbol) ||
                other.chainASymbol == chainASymbol) &&
            (identical(other.chainBId, chainBId) ||
                other.chainBId == chainBId) &&
            (identical(other.chainBSymbol, chainBSymbol) ||
                other.chainBSymbol == chainBSymbol) &&
            (identical(other.chainCId, chainCId) ||
                other.chainCId == chainCId) &&
            (identical(other.chainCSymbol, chainCSymbol) ||
                other.chainCSymbol == chainCSymbol) &&
            (identical(other.chainDId, chainDId) ||
                other.chainDId == chainDId) &&
            (identical(other.chainDSymbol, chainDSymbol) ||
                other.chainDSymbol == chainDSymbol));
  }

  @override
  int get hashCode => Object.hash(
      runtimeType,
      angleDegrees,
      chainAId,
      chainASymbol,
      chainBId,
      chainBSymbol,
      chainCId,
      chainCSymbol,
      chainDId,
      chainDSymbol);

  @override
  String toString() {
    return 'APIMeasurement.dihedral(angleDegrees: $angleDegrees, chainAId: $chainAId, chainASymbol: $chainASymbol, chainBId: $chainBId, chainBSymbol: $chainBSymbol, chainCId: $chainCId, chainCSymbol: $chainCSymbol, chainDId: $chainDId, chainDSymbol: $chainDSymbol)';
  }
}

/// @nodoc
abstract mixin class $APIMeasurement_DihedralCopyWith<$Res>
    implements $APIMeasurementCopyWith<$Res> {
  factory $APIMeasurement_DihedralCopyWith(APIMeasurement_Dihedral value,
          $Res Function(APIMeasurement_Dihedral) _then) =
      _$APIMeasurement_DihedralCopyWithImpl;
  @useResult
  $Res call(
      {double angleDegrees,
      int chainAId,
      String chainASymbol,
      int chainBId,
      String chainBSymbol,
      int chainCId,
      String chainCSymbol,
      int chainDId,
      String chainDSymbol});
}

/// @nodoc
class _$APIMeasurement_DihedralCopyWithImpl<$Res>
    implements $APIMeasurement_DihedralCopyWith<$Res> {
  _$APIMeasurement_DihedralCopyWithImpl(this._self, this._then);

  final APIMeasurement_Dihedral _self;
  final $Res Function(APIMeasurement_Dihedral) _then;

  /// Create a copy of APIMeasurement
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? angleDegrees = null,
    Object? chainAId = null,
    Object? chainASymbol = null,
    Object? chainBId = null,
    Object? chainBSymbol = null,
    Object? chainCId = null,
    Object? chainCSymbol = null,
    Object? chainDId = null,
    Object? chainDSymbol = null,
  }) {
    return _then(APIMeasurement_Dihedral(
      angleDegrees: null == angleDegrees
          ? _self.angleDegrees
          : angleDegrees // ignore: cast_nullable_to_non_nullable
              as double,
      chainAId: null == chainAId
          ? _self.chainAId
          : chainAId // ignore: cast_nullable_to_non_nullable
              as int,
      chainASymbol: null == chainASymbol
          ? _self.chainASymbol
          : chainASymbol // ignore: cast_nullable_to_non_nullable
              as String,
      chainBId: null == chainBId
          ? _self.chainBId
          : chainBId // ignore: cast_nullable_to_non_nullable
              as int,
      chainBSymbol: null == chainBSymbol
          ? _self.chainBSymbol
          : chainBSymbol // ignore: cast_nullable_to_non_nullable
              as String,
      chainCId: null == chainCId
          ? _self.chainCId
          : chainCId // ignore: cast_nullable_to_non_nullable
              as int,
      chainCSymbol: null == chainCSymbol
          ? _self.chainCSymbol
          : chainCSymbol // ignore: cast_nullable_to_non_nullable
              as String,
      chainDId: null == chainDId
          ? _self.chainDId
          : chainDId // ignore: cast_nullable_to_non_nullable
              as int,
      chainDSymbol: null == chainDSymbol
          ? _self.chainDSymbol
          : chainDSymbol // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class APIMeasurement_AtomInfo extends APIMeasurement {
  const APIMeasurement_AtomInfo(
      {required this.symbol,
      required this.elementName,
      required this.bondCount,
      required this.x,
      required this.y,
      required this.z,
      required this.hybridizationOverride,
      required this.inferredHybridization})
      : super._();

  /// Element symbol (e.g., "C").
  final String symbol;

  /// Full element name (e.g., "Carbon").
  final String elementName;

  /// Number of bonds on this atom (coordination number).
  final int bondCount;

  /// Position in Angstroms.
  final double x;
  final double y;
  final double z;

  /// Hybridization override (0=Auto, 1=Sp3, 2=Sp2, 3=Sp1).
  final int hybridizationOverride;

  /// Inferred hybridization from bond orders (1=Sp3, 2=Sp2, 3=Sp1, 0=unknown/terminal).
  final int inferredHybridization;

  /// Create a copy of APIMeasurement
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $APIMeasurement_AtomInfoCopyWith<APIMeasurement_AtomInfo> get copyWith =>
      _$APIMeasurement_AtomInfoCopyWithImpl<APIMeasurement_AtomInfo>(
          this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is APIMeasurement_AtomInfo &&
            (identical(other.symbol, symbol) || other.symbol == symbol) &&
            (identical(other.elementName, elementName) ||
                other.elementName == elementName) &&
            (identical(other.bondCount, bondCount) ||
                other.bondCount == bondCount) &&
            (identical(other.x, x) || other.x == x) &&
            (identical(other.y, y) || other.y == y) &&
            (identical(other.z, z) || other.z == z) &&
            (identical(other.hybridizationOverride, hybridizationOverride) ||
                other.hybridizationOverride == hybridizationOverride) &&
            (identical(other.inferredHybridization, inferredHybridization) ||
                other.inferredHybridization == inferredHybridization));
  }

  @override
  int get hashCode => Object.hash(runtimeType, symbol, elementName, bondCount,
      x, y, z, hybridizationOverride, inferredHybridization);

  @override
  String toString() {
    return 'APIMeasurement.atomInfo(symbol: $symbol, elementName: $elementName, bondCount: $bondCount, x: $x, y: $y, z: $z, hybridizationOverride: $hybridizationOverride, inferredHybridization: $inferredHybridization)';
  }
}

/// @nodoc
abstract mixin class $APIMeasurement_AtomInfoCopyWith<$Res>
    implements $APIMeasurementCopyWith<$Res> {
  factory $APIMeasurement_AtomInfoCopyWith(APIMeasurement_AtomInfo value,
          $Res Function(APIMeasurement_AtomInfo) _then) =
      _$APIMeasurement_AtomInfoCopyWithImpl;
  @useResult
  $Res call(
      {String symbol,
      String elementName,
      int bondCount,
      double x,
      double y,
      double z,
      int hybridizationOverride,
      int inferredHybridization});
}

/// @nodoc
class _$APIMeasurement_AtomInfoCopyWithImpl<$Res>
    implements $APIMeasurement_AtomInfoCopyWith<$Res> {
  _$APIMeasurement_AtomInfoCopyWithImpl(this._self, this._then);

  final APIMeasurement_AtomInfo _self;
  final $Res Function(APIMeasurement_AtomInfo) _then;

  /// Create a copy of APIMeasurement
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? symbol = null,
    Object? elementName = null,
    Object? bondCount = null,
    Object? x = null,
    Object? y = null,
    Object? z = null,
    Object? hybridizationOverride = null,
    Object? inferredHybridization = null,
  }) {
    return _then(APIMeasurement_AtomInfo(
      symbol: null == symbol
          ? _self.symbol
          : symbol // ignore: cast_nullable_to_non_nullable
              as String,
      elementName: null == elementName
          ? _self.elementName
          : elementName // ignore: cast_nullable_to_non_nullable
              as String,
      bondCount: null == bondCount
          ? _self.bondCount
          : bondCount // ignore: cast_nullable_to_non_nullable
              as int,
      x: null == x
          ? _self.x
          : x // ignore: cast_nullable_to_non_nullable
              as double,
      y: null == y
          ? _self.y
          : y // ignore: cast_nullable_to_non_nullable
              as double,
      z: null == z
          ? _self.z
          : z // ignore: cast_nullable_to_non_nullable
              as double,
      hybridizationOverride: null == hybridizationOverride
          ? _self.hybridizationOverride
          : hybridizationOverride // ignore: cast_nullable_to_non_nullable
              as int,
      inferredHybridization: null == inferredHybridization
          ? _self.inferredHybridization
          : inferredHybridization // ignore: cast_nullable_to_non_nullable
              as int,
    ));
  }
}

/// @nodoc
mixin _$APIViewportPickResult {
  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is APIViewportPickResult);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'APIViewportPickResult()';
  }
}

/// @nodoc
class $APIViewportPickResultCopyWith<$Res> {
  $APIViewportPickResultCopyWith(
      APIViewportPickResult _, $Res Function(APIViewportPickResult) __);
}

/// @nodoc

class APIViewportPickResult_ActiveNodeHit extends APIViewportPickResult {
  const APIViewportPickResult_ActiveNodeHit() : super._();

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is APIViewportPickResult_ActiveNodeHit);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'APIViewportPickResult.activeNodeHit()';
  }
}

/// @nodoc

class APIViewportPickResult_ActivateNode extends APIViewportPickResult {
  const APIViewportPickResult_ActivateNode(
      {required this.nodeId, required this.nodeName})
      : super._();

  final BigInt nodeId;
  final String nodeName;

  /// Create a copy of APIViewportPickResult
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $APIViewportPickResult_ActivateNodeCopyWith<
          APIViewportPickResult_ActivateNode>
      get copyWith => _$APIViewportPickResult_ActivateNodeCopyWithImpl<
          APIViewportPickResult_ActivateNode>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is APIViewportPickResult_ActivateNode &&
            (identical(other.nodeId, nodeId) || other.nodeId == nodeId) &&
            (identical(other.nodeName, nodeName) ||
                other.nodeName == nodeName));
  }

  @override
  int get hashCode => Object.hash(runtimeType, nodeId, nodeName);

  @override
  String toString() {
    return 'APIViewportPickResult.activateNode(nodeId: $nodeId, nodeName: $nodeName)';
  }
}

/// @nodoc
abstract mixin class $APIViewportPickResult_ActivateNodeCopyWith<$Res>
    implements $APIViewportPickResultCopyWith<$Res> {
  factory $APIViewportPickResult_ActivateNodeCopyWith(
          APIViewportPickResult_ActivateNode value,
          $Res Function(APIViewportPickResult_ActivateNode) _then) =
      _$APIViewportPickResult_ActivateNodeCopyWithImpl;
  @useResult
  $Res call({BigInt nodeId, String nodeName});
}

/// @nodoc
class _$APIViewportPickResult_ActivateNodeCopyWithImpl<$Res>
    implements $APIViewportPickResult_ActivateNodeCopyWith<$Res> {
  _$APIViewportPickResult_ActivateNodeCopyWithImpl(this._self, this._then);

  final APIViewportPickResult_ActivateNode _self;
  final $Res Function(APIViewportPickResult_ActivateNode) _then;

  /// Create a copy of APIViewportPickResult
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? nodeId = null,
    Object? nodeName = null,
  }) {
    return _then(APIViewportPickResult_ActivateNode(
      nodeId: null == nodeId
          ? _self.nodeId
          : nodeId // ignore: cast_nullable_to_non_nullable
              as BigInt,
      nodeName: null == nodeName
          ? _self.nodeName
          : nodeName // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class APIViewportPickResult_Disambiguation extends APIViewportPickResult {
  const APIViewportPickResult_Disambiguation(
      {required final List<APICandidateNode> candidates})
      : _candidates = candidates,
        super._();

  final List<APICandidateNode> _candidates;
  List<APICandidateNode> get candidates {
    if (_candidates is EqualUnmodifiableListView) return _candidates;
    // ignore: implicit_dynamic_type
    return EqualUnmodifiableListView(_candidates);
  }

  /// Create a copy of APIViewportPickResult
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $APIViewportPickResult_DisambiguationCopyWith<
          APIViewportPickResult_Disambiguation>
      get copyWith => _$APIViewportPickResult_DisambiguationCopyWithImpl<
          APIViewportPickResult_Disambiguation>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is APIViewportPickResult_Disambiguation &&
            const DeepCollectionEquality()
                .equals(other._candidates, _candidates));
  }

  @override
  int get hashCode => Object.hash(
      runtimeType, const DeepCollectionEquality().hash(_candidates));

  @override
  String toString() {
    return 'APIViewportPickResult.disambiguation(candidates: $candidates)';
  }
}

/// @nodoc
abstract mixin class $APIViewportPickResult_DisambiguationCopyWith<$Res>
    implements $APIViewportPickResultCopyWith<$Res> {
  factory $APIViewportPickResult_DisambiguationCopyWith(
          APIViewportPickResult_Disambiguation value,
          $Res Function(APIViewportPickResult_Disambiguation) _then) =
      _$APIViewportPickResult_DisambiguationCopyWithImpl;
  @useResult
  $Res call({List<APICandidateNode> candidates});
}

/// @nodoc
class _$APIViewportPickResult_DisambiguationCopyWithImpl<$Res>
    implements $APIViewportPickResult_DisambiguationCopyWith<$Res> {
  _$APIViewportPickResult_DisambiguationCopyWithImpl(this._self, this._then);

  final APIViewportPickResult_Disambiguation _self;
  final $Res Function(APIViewportPickResult_Disambiguation) _then;

  /// Create a copy of APIViewportPickResult
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? candidates = null,
  }) {
    return _then(APIViewportPickResult_Disambiguation(
      candidates: null == candidates
          ? _self._candidates
          : candidates // ignore: cast_nullable_to_non_nullable
              as List<APICandidateNode>,
    ));
  }
}

/// @nodoc

class APIViewportPickResult_NoHit extends APIViewportPickResult {
  const APIViewportPickResult_NoHit() : super._();

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is APIViewportPickResult_NoHit);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'APIViewportPickResult.noHit()';
  }
}

/// @nodoc
mixin _$GuidedPlacementApiResult {
  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is GuidedPlacementApiResult);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'GuidedPlacementApiResult()';
  }
}

/// @nodoc
class $GuidedPlacementApiResultCopyWith<$Res> {
  $GuidedPlacementApiResultCopyWith(
      GuidedPlacementApiResult _, $Res Function(GuidedPlacementApiResult) __);
}

/// @nodoc

class GuidedPlacementApiResult_NoAtomHit extends GuidedPlacementApiResult {
  const GuidedPlacementApiResult_NoAtomHit() : super._();

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is GuidedPlacementApiResult_NoAtomHit);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'GuidedPlacementApiResult.noAtomHit()';
  }
}

/// @nodoc

class GuidedPlacementApiResult_AtomSaturated extends GuidedPlacementApiResult {
  const GuidedPlacementApiResult_AtomSaturated(
      {required this.hasAdditionalCapacity, required this.dativeIncompatible})
      : super._();

  /// True when the atom has lone pairs / empty orbitals
  /// (switch to Dative bond mode to access them).
  final bool hasAdditionalCapacity;

  /// True when has_additional_capacity is true but the new element cannot
  /// form a dative bond with the anchor (no valid donor-acceptor pair).
  final bool dativeIncompatible;

  /// Create a copy of GuidedPlacementApiResult
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $GuidedPlacementApiResult_AtomSaturatedCopyWith<
          GuidedPlacementApiResult_AtomSaturated>
      get copyWith => _$GuidedPlacementApiResult_AtomSaturatedCopyWithImpl<
          GuidedPlacementApiResult_AtomSaturated>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is GuidedPlacementApiResult_AtomSaturated &&
            (identical(other.hasAdditionalCapacity, hasAdditionalCapacity) ||
                other.hasAdditionalCapacity == hasAdditionalCapacity) &&
            (identical(other.dativeIncompatible, dativeIncompatible) ||
                other.dativeIncompatible == dativeIncompatible));
  }

  @override
  int get hashCode =>
      Object.hash(runtimeType, hasAdditionalCapacity, dativeIncompatible);

  @override
  String toString() {
    return 'GuidedPlacementApiResult.atomSaturated(hasAdditionalCapacity: $hasAdditionalCapacity, dativeIncompatible: $dativeIncompatible)';
  }
}

/// @nodoc
abstract mixin class $GuidedPlacementApiResult_AtomSaturatedCopyWith<$Res>
    implements $GuidedPlacementApiResultCopyWith<$Res> {
  factory $GuidedPlacementApiResult_AtomSaturatedCopyWith(
          GuidedPlacementApiResult_AtomSaturated value,
          $Res Function(GuidedPlacementApiResult_AtomSaturated) _then) =
      _$GuidedPlacementApiResult_AtomSaturatedCopyWithImpl;
  @useResult
  $Res call({bool hasAdditionalCapacity, bool dativeIncompatible});
}

/// @nodoc
class _$GuidedPlacementApiResult_AtomSaturatedCopyWithImpl<$Res>
    implements $GuidedPlacementApiResult_AtomSaturatedCopyWith<$Res> {
  _$GuidedPlacementApiResult_AtomSaturatedCopyWithImpl(this._self, this._then);

  final GuidedPlacementApiResult_AtomSaturated _self;
  final $Res Function(GuidedPlacementApiResult_AtomSaturated) _then;

  /// Create a copy of GuidedPlacementApiResult
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? hasAdditionalCapacity = null,
    Object? dativeIncompatible = null,
  }) {
    return _then(GuidedPlacementApiResult_AtomSaturated(
      hasAdditionalCapacity: null == hasAdditionalCapacity
          ? _self.hasAdditionalCapacity
          : hasAdditionalCapacity // ignore: cast_nullable_to_non_nullable
              as bool,
      dativeIncompatible: null == dativeIncompatible
          ? _self.dativeIncompatible
          : dativeIncompatible // ignore: cast_nullable_to_non_nullable
              as bool,
    ));
  }
}

/// @nodoc

class GuidedPlacementApiResult_GuidedPlacementStarted
    extends GuidedPlacementApiResult {
  const GuidedPlacementApiResult_GuidedPlacementStarted(
      {required this.guideCount, required this.anchorAtomId})
      : super._();

  final int guideCount;
  final int anchorAtomId;

  /// Create a copy of GuidedPlacementApiResult
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $GuidedPlacementApiResult_GuidedPlacementStartedCopyWith<
          GuidedPlacementApiResult_GuidedPlacementStarted>
      get copyWith =>
          _$GuidedPlacementApiResult_GuidedPlacementStartedCopyWithImpl<
                  GuidedPlacementApiResult_GuidedPlacementStarted>(
              this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is GuidedPlacementApiResult_GuidedPlacementStarted &&
            (identical(other.guideCount, guideCount) ||
                other.guideCount == guideCount) &&
            (identical(other.anchorAtomId, anchorAtomId) ||
                other.anchorAtomId == anchorAtomId));
  }

  @override
  int get hashCode => Object.hash(runtimeType, guideCount, anchorAtomId);

  @override
  String toString() {
    return 'GuidedPlacementApiResult.guidedPlacementStarted(guideCount: $guideCount, anchorAtomId: $anchorAtomId)';
  }
}

/// @nodoc
abstract mixin class $GuidedPlacementApiResult_GuidedPlacementStartedCopyWith<
    $Res> implements $GuidedPlacementApiResultCopyWith<$Res> {
  factory $GuidedPlacementApiResult_GuidedPlacementStartedCopyWith(
          GuidedPlacementApiResult_GuidedPlacementStarted value,
          $Res Function(GuidedPlacementApiResult_GuidedPlacementStarted)
              _then) =
      _$GuidedPlacementApiResult_GuidedPlacementStartedCopyWithImpl;
  @useResult
  $Res call({int guideCount, int anchorAtomId});
}

/// @nodoc
class _$GuidedPlacementApiResult_GuidedPlacementStartedCopyWithImpl<$Res>
    implements $GuidedPlacementApiResult_GuidedPlacementStartedCopyWith<$Res> {
  _$GuidedPlacementApiResult_GuidedPlacementStartedCopyWithImpl(
      this._self, this._then);

  final GuidedPlacementApiResult_GuidedPlacementStarted _self;
  final $Res Function(GuidedPlacementApiResult_GuidedPlacementStarted) _then;

  /// Create a copy of GuidedPlacementApiResult
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? guideCount = null,
    Object? anchorAtomId = null,
  }) {
    return _then(GuidedPlacementApiResult_GuidedPlacementStarted(
      guideCount: null == guideCount
          ? _self.guideCount
          : guideCount // ignore: cast_nullable_to_non_nullable
              as int,
      anchorAtomId: null == anchorAtomId
          ? _self.anchorAtomId
          : anchorAtomId // ignore: cast_nullable_to_non_nullable
              as int,
    ));
  }
}

// dart format on
