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
  const APIMeasurement_Distance({required this.distance}) : super._();

  final double distance;

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
                other.distance == distance));
  }

  @override
  int get hashCode => Object.hash(runtimeType, distance);

  @override
  String toString() {
    return 'APIMeasurement.distance(distance: $distance)';
  }
}

/// @nodoc
abstract mixin class $APIMeasurement_DistanceCopyWith<$Res>
    implements $APIMeasurementCopyWith<$Res> {
  factory $APIMeasurement_DistanceCopyWith(APIMeasurement_Distance value,
          $Res Function(APIMeasurement_Distance) _then) =
      _$APIMeasurement_DistanceCopyWithImpl;
  @useResult
  $Res call({double distance});
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
  }) {
    return _then(APIMeasurement_Distance(
      distance: null == distance
          ? _self.distance
          : distance // ignore: cast_nullable_to_non_nullable
              as double,
    ));
  }
}

/// @nodoc

class APIMeasurement_Angle extends APIMeasurement {
  const APIMeasurement_Angle({required this.angleDegrees}) : super._();

  final double angleDegrees;

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
                other.angleDegrees == angleDegrees));
  }

  @override
  int get hashCode => Object.hash(runtimeType, angleDegrees);

  @override
  String toString() {
    return 'APIMeasurement.angle(angleDegrees: $angleDegrees)';
  }
}

/// @nodoc
abstract mixin class $APIMeasurement_AngleCopyWith<$Res>
    implements $APIMeasurementCopyWith<$Res> {
  factory $APIMeasurement_AngleCopyWith(APIMeasurement_Angle value,
          $Res Function(APIMeasurement_Angle) _then) =
      _$APIMeasurement_AngleCopyWithImpl;
  @useResult
  $Res call({double angleDegrees});
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
  }) {
    return _then(APIMeasurement_Angle(
      angleDegrees: null == angleDegrees
          ? _self.angleDegrees
          : angleDegrees // ignore: cast_nullable_to_non_nullable
              as double,
    ));
  }
}

/// @nodoc

class APIMeasurement_Dihedral extends APIMeasurement {
  const APIMeasurement_Dihedral({required this.angleDegrees}) : super._();

  final double angleDegrees;

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
                other.angleDegrees == angleDegrees));
  }

  @override
  int get hashCode => Object.hash(runtimeType, angleDegrees);

  @override
  String toString() {
    return 'APIMeasurement.dihedral(angleDegrees: $angleDegrees)';
  }
}

/// @nodoc
abstract mixin class $APIMeasurement_DihedralCopyWith<$Res>
    implements $APIMeasurementCopyWith<$Res> {
  factory $APIMeasurement_DihedralCopyWith(APIMeasurement_Dihedral value,
          $Res Function(APIMeasurement_Dihedral) _then) =
      _$APIMeasurement_DihedralCopyWithImpl;
  @useResult
  $Res call({double angleDegrees});
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
  }) {
    return _then(APIMeasurement_Dihedral(
      angleDegrees: null == angleDegrees
          ? _self.angleDegrees
          : angleDegrees // ignore: cast_nullable_to_non_nullable
              as double,
    ));
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
