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
      {required this.hasAdditionalCapacity})
      : super._();

  /// True when the atom has lone pairs / empty orbitals
  /// (switch to Dative bond mode to access them).
  final bool hasAdditionalCapacity;

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
                other.hasAdditionalCapacity == hasAdditionalCapacity));
  }

  @override
  int get hashCode => Object.hash(runtimeType, hasAdditionalCapacity);

  @override
  String toString() {
    return 'GuidedPlacementApiResult.atomSaturated(hasAdditionalCapacity: $hasAdditionalCapacity)';
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
  $Res call({bool hasAdditionalCapacity});
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
  }) {
    return _then(GuidedPlacementApiResult_AtomSaturated(
      hasAdditionalCapacity: null == hasAdditionalCapacity
          ? _self.hasAdditionalCapacity
          : hasAdditionalCapacity // ignore: cast_nullable_to_non_nullable
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
