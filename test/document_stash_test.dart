/// The per-tab stash of Flutter-only document state
/// (`doc/design_multiple_documents.md` §5.3, Phase 3).
///
/// Capture, restore and the tab-list adoption are pure Dart over the model —
/// `StructureDesignerModel()` is constructible without the Rust library, and
/// `adoptDocuments` takes the tab list as data — so the whole round trip runs
/// here without the kernel.
library;

import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

APIDocumentTab _tab(int id, {bool active = false}) => APIDocumentTab(
      id: BigInt.from(id),
      displayName: 'doc$id.cnnd',
      filePath: 'C:/designs/doc$id.cnnd',
      isDirty: false,
      isActive: active,
    );

/// Every stashed field set to something other than its default.
final DocumentUiState _stateA = DocumentUiState(
  activeScopeChain: [BigInt.from(3), BigInt.from(7)],
  propertyEditorScopeChain: [BigInt.from(3)],
  selectedAiHistorySeq: BigInt.from(42),
  aiHistoryVersion: BigInt.from(9),
  unreadAiEditCount: 2,
  lastLoadParamIdRepairs: const ['repaired x'],
);

void main() {
  test('capture after restore gives back every stashed field', () {
    final model = StructureDesignerModel();
    model.restoreDocumentUiState(_stateA);
    expect(model.captureDocumentUiState(), _stateA);
    expect(model.activeScopeChain, _stateA.activeScopeChain);
    expect(model.propertyEditorScopeChain, _stateA.propertyEditorScopeChain);
    expect(model.selectedAiHistorySeq, _stateA.selectedAiHistorySeq);
    expect(model.unreadAiEditCount, 2);
    expect(model.lastLoadParamIdRepairs, ['repaired x']);
  });

  test('restore of a document with no entry gives the post-load defaults', () {
    final model = StructureDesignerModel();
    model.restoreDocumentUiState(_stateA);
    model.restoreDocumentUiState(null);
    expect(model.captureDocumentUiState(), DocumentUiState.postLoadDefaults);
    expect(model.activeScopeChain, isEmpty);
    expect(model.selectedAiHistorySeq, isNull);
    expect(model.lastLoadLibraryReport, isNull);
  });

  test('a switch stashes the outgoing document and restores the incoming one',
      () {
    final model = StructureDesignerModel();
    // The first refresh only learns which document is active.
    expect(model.adoptDocuments([_tab(1, active: true), _tab(2)]), isFalse);
    model.restoreDocumentUiState(_stateA);

    // Switch to 2: 1's state is stashed, 2 starts from the defaults.
    expect(model.adoptDocuments([_tab(1), _tab(2, active: true)]), isTrue);
    expect(model.stashedDocumentUiState(BigInt.one), _stateA);
    expect(model.captureDocumentUiState(), DocumentUiState.postLoadDefaults);
    expect(model.activeDocumentId, BigInt.two);

    // Something set in 2 …
    model.activeScopeChain = [BigInt.from(5)];

    // … and back to 1: exactly what it had, and 2's state is stashed.
    expect(model.adoptDocuments([_tab(1, active: true), _tab(2)]), isTrue);
    expect(model.captureDocumentUiState(), _stateA);
    expect(model.stashedDocumentUiState(BigInt.two)?.activeScopeChain,
        [BigInt.from(5)]);
    // The active document's own state is never in the stash.
    expect(model.stashedDocumentUiState(BigInt.one), isNull);
  });

  test('a refresh without a switch leaves the state alone', () {
    final model = StructureDesignerModel();
    model.adoptDocuments([_tab(1, active: true)]);
    model.restoreDocumentUiState(_stateA);
    expect(model.adoptDocuments([_tab(1, active: true), _tab(2)]), isFalse);
    expect(model.captureDocumentUiState(), _stateA);
  });

  test('an id no longer in the tab list is dropped', () {
    final model = StructureDesignerModel();
    model.adoptDocuments([_tab(1, active: true), _tab(2)]);
    model.restoreDocumentUiState(_stateA);
    model.adoptDocuments([_tab(1), _tab(2, active: true)]);
    expect(model.stashedDocumentUiState(BigInt.one), isNotNull);

    // Tab 1 closed while parked.
    model.adoptDocuments([_tab(2, active: true)]);
    expect(model.stashedDocumentUiState(BigInt.one), isNull);
  });

  test('closing the active tab does not stash it', () {
    final model = StructureDesignerModel();
    model.adoptDocuments([_tab(1, active: true), _tab(2)]);
    model.restoreDocumentUiState(_stateA);
    // Tab 1 closed: its neighbour is active and 1 is gone from the list.
    model.adoptDocuments([_tab(2, active: true)]);
    expect(model.stashedDocumentUiState(BigInt.one), isNull);
    expect(model.captureDocumentUiState(), DocumentUiState.postLoadDefaults);
  });

  test('a renumbered document (in-place load, D8) starts from the defaults',
      () {
    final model = StructureDesignerModel();
    model.adoptDocuments([_tab(1, active: true)]);
    model.restoreDocumentUiState(_stateA);
    // Same tab, new id: the content was replaced in place.
    model.adoptDocuments([_tab(3, active: true)]);
    expect(model.captureDocumentUiState(), DocumentUiState.postLoadDefaults);
    expect(model.stashedDocumentUiState(BigInt.one), isNull);
  });
}
