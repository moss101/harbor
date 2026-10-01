import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_office/services/harbor_service.dart';

final repoRoot = Directory.current.parent.parent.path; // apps/harbor_office
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';

/// Undo/redo + autosave-draft recovery, driven straight against the real
/// core (plain async tests: real FFI futures must not run inside a
/// testWidgets fake-async zone).
void main() {
  late HarborService service;
  late Directory root;

  setUpAll(() async {
    root = await Directory.systemTemp.createTemp('harbor-history-test-');
    service = await HarborService.open(
        libraryPath: dylibPath,
        dataRoot: root.path,
        workspaceId: 'ws-history',
        deviceRootHex:
            'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
  });

  tearDown(() async {
    service.clearPreview();
  });

  tearDownAll(() async {
    await service.close();
  });

  test('workbook edits undo and redo through the real core', () async {
    final fixture =
        File('$repoRoot/fixtures/office/dcf_model.xlsx').readAsBytesSync();
    await service.loadPreviewFromBytes(fixture, name: 'dcf_model.xlsx');
    expect(service.canUndo, isFalse);
    expect(service.canRedo, isFalse);

    await service.editWorkbookCells([
      {'sheet': 'DCF', 'row': 1, 'col': 1, 'kind': 'number', 'value': 42},
    ]);
    expect(service.canUndo, isTrue);

    // Undo restores the pre-edit bytes.
    final undone = await service.undo();
    expect(undone, isTrue);
    expect(service.canUndo, isFalse);
    expect(service.canRedo, isTrue);
    final afterUndo = service.currentWorkingBytes()!;
    final reloaded = await service.extractPreview(afterUndo);
    final cells = (reloaded?['preview']['cells'] as List)
        .cast<Map>()
        .where((c) => c['row'] == 1 && c['col'] == 1);
    final value = cells.isEmpty ? null : cells.first['value'];
    expect(value == null || value != '42', isTrue,
        reason: 'undo must restore the original cell');

    // Redo reapplies the edit.
    final redone = await service.redo();
    expect(redone, isTrue);
    final afterRedo = service.currentWorkingBytes()!;
    final reloaded2 = await service.extractPreview(afterRedo);
    final cells2 = (reloaded2?['preview']['cells'] as List)
        .cast<Map>()
        .firstWhere((c) => c['row'] == 1 && c['col'] == 1,
            orElse: () => <String, dynamic>{});
    expect(cells2['value'], '42');
  });

  test('autosave draft survives a restart and markSaved clears it',
      () async {
    final autosaveDir = '${root.path}/autosave';
    await service.attachDraftStore(autosaveDir);
    final fixture =
        File('$repoRoot/fixtures/office/dcf_model.xlsx').readAsBytesSync();
    await service.loadPreviewFromBytes(fixture, name: 'dcf_model.xlsx');
    await service.editWorkbookCells([
      {'sheet': 'DCF', 'row': 2, 'col': 1, 'kind': 'number', 'value': 7},
    ]);
    // Draft file now exists with the edited bytes.
    final draft = File('$autosaveDir/current-edit.draft');
    expect(await draft.exists(), isTrue);

    // A NEW service over the same root restores the draft.
    final second = await HarborService.open(
        libraryPath: dylibPath,
        dataRoot: root.path,
        workspaceId: 'ws-history-2',
        deviceRootHex:
            'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
    addTearDown(second.close);
    await second.attachDraftStore(autosaveDir);
    expect(second.draftRestored, isTrue, reason: 'draft must be restored');
    expect(second.draftName, 'dcf_model.xlsx');
    final restored = second.currentWorkingBytes()!;
    final preview = await second.extractPreview(restored);
    final cell = (preview?['preview']['cells'] as List)
        .cast<Map>()
        .firstWhere((c) => c['row'] == 2 && c['col'] == 1,
            orElse: () => <String, dynamic>{});
    expect(cell['value'], '7', reason: 'the edited value came back');

    // Saving clears the draft.
    await second.markSaved();
    expect(await draft.exists(), isFalse);
    expect(second.draftRestored, isFalse);
  });

  test('document exports to PDF and workbook row ops shift content',
      () async {
    final d = await service.createDocument(title: 'Export');
    await service.loadPreviewFromBytes(d, name: 'Export.docx');
    final pdf = await service.exportDocxToPdf(title: 'Export');
    expect(pdf.isNotEmpty, isTrue);
    expect(pdf[0], 0x25); // '%PDF'
    // A non-document open must refuse honestly.
    final wb = await service.createWorkbook();
    await service.loadPreviewFromBytes(wb, name: 'Book.xlsx');
    try {
      await service.exportDocxToPdf();
      fail('workbook must refuse docx-only export');
    } catch (e) {
      expect(e, isA<Exception>());
    }
    // Structure op through the generic edit path: insert a row at 1.
    await service.editWorkbook(ops: [
      {'op': 'insert_row', 'sheet': 'Sheet1', 'index': 1, 'count': 1},
    ]);
    expect(service.canUndo, isTrue, reason: 'structure ops are undoable');
  });

  test('multi-sheet focus switches and unknown sheets refuse', () async {
    final fixture =
        File('$repoRoot/fixtures/office/board_demo.xlsx').readAsBytesSync();
    await service.loadPreviewFromBytes(fixture, name: 'board_demo.xlsx');
    final sheets =
        ((service.preview!['preview'] as Map)['sheets'] as List).cast<String>();
    if (sheets.length < 2) {
      // Single-sheet fixture: switching to its own sheet is a no-op.
      await service.switchSheet(sheets.first);
      expect((service.preview!['preview'] as Map)['sheet'], sheets.first);
      return;
    }
    await service.switchSheet(sheets[1]);
    expect((service.preview!['preview'] as Map)['sheet'], sheets[1]);
    expect(
      () => service.switchSheet('does-not-exist'),
      throwsA(isA<Exception>()),
    );
  });
}
