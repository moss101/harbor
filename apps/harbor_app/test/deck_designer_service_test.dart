import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_ui/deck_designer.dart';

final repoRoot = Directory.current.parent.parent.path; // apps/harbor_app
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';

/// The designed-deck service wiring, driven against the real core (plain
/// async tests: real FFI futures must not run inside a testWidgets
/// fake-async zone). Skipped when the built dylib predates the `deck.*`
/// arms (rebuild with `cd core && cargo build -p harbor_ffi`).
void main() {
  late HarborService service;
  late Directory root;
  String? skip;

  setUpAll(() async {
    final lib = File(dylibPath);
    if (!lib.existsSync()) {
      skip = 'libharbor_ffi.dylib is not built';
      return;
    }
    if (!latin1.decode(lib.readAsBytesSync()).contains('deck.restyle')) {
      skip = 'libharbor_ffi.dylib predates the deck.* FFI arms';
      return;
    }
    root = await Directory.systemTemp.createTemp('harbor-app-deck-designer-');
    service = await HarborService.open(
        libraryPath: dylibPath,
        dataRoot: root.path,
        workspaceId: 'ws-deck-designer',
        deviceRootHex:
            'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
  });

  /// A test that reports itself skipped when the dylib cannot serve it.
  void guarded(String name, Future<void> Function() body) {
    test(name, () async {
      if (skip != null) {
        markTestSkipped(skip!);
        return;
      }
      await body();
    });
  }

  tearDownAll(() async {
    if (skip == null) {
      await service.close();
      await root.delete(recursive: true);
    }
  });

  guarded('catalogue: designs, palettes, the four motions; cached', () async {
    final c = await service.deckCatalog();
    expect(c.designs, isNotEmpty);
    expect(c.palettes, isNotEmpty);
    expect(c.purposes, isNotEmpty);
    expect(
        c.motions.map((m) => m.id), ['none', 'subtle', 'dynamic', 'cinematic']);
    expect(identical(await service.deckCatalog(), c), isTrue);
  });

  guarded(
      'starter -> preview -> restyle swaps the working copy and keeps '
      'the slides', () async {
    final r = await service.deckStarter(
        purpose: 'pitch_deck',
        style: const DeckStyleChoice(
            design: 'ocean', palette: 'mint', motion: 'dynamic'));
    expect(r.fileName, endsWith('.pptx'));
    expect(r.scene!.slides.length, r.slides);

    await service.loadPreviewFromBytes(r.bytes, name: r.fileName);
    final design = (service.preview!['preview'] as Map)['design'] as Map;
    expect(design['restylable'], isTrue);
    expect(design['theme'], 'ocean');
    expect(design['palette'], 'mint');

    final res = await service.restyleOpenDeck(const DeckStyleChoice(
        design: 'slate', palette: null, motion: 'subtle', rtl: false));
    expect(res.scene!.slides.length, r.slides);
    final after = (service.preview!['preview'] as Map)['design'] as Map;
    expect(after['theme'], 'slate');
    expect(after['palette'], isNull);
    expect(service.currentWorkingBytes(), res.bytes);
  });

  guarded('a restyle leaves something to save; saving a copy clears it',
      () async {
    final r = await service.deckStarter(purpose: 'pitch_deck');
    await service.loadPreviewFromBytes(r.bytes, name: r.fileName);
    expect(service.hasUnsavedChanges, isFalse);
    service.markSaved(); // harmless when nothing is pending
    expect(service.hasUnsavedChanges, isFalse);

    final res = await service.restyleOpenDeck(const DeckStyleChoice(
        design: 'ivory', palette: null, motion: 'subtle', rtl: false));
    expect(service.hasUnsavedChanges, isTrue);
    expect(service.currentWorkingBytes(), res.bytes);

    service.markSaved();
    expect(service.hasUnsavedChanges, isFalse);
    expect(service.currentWorkingBytes(), res.bytes,
        reason: 'saving a copy keeps the working copy');

    await service.restyleOpenDeck(const DeckStyleChoice(
        design: 'slate', palette: null, motion: 'subtle', rtl: false));
    expect(service.hasUnsavedChanges, isTrue);

    // Reopening a file starts clean; closing it leaves nothing pending.
    await service.loadPreviewFromBytes(r.bytes, name: r.fileName);
    expect(service.hasUnsavedChanges, isFalse);
    service.clearPreview();
    expect(service.hasUnsavedChanges, isFalse);
  });

  guarded('restyle refuses when the open artifact is not a deck', () async {
    final docx = await service.convertMarkdownToDocx('# Title\n\nBody\n');
    await service.loadPreviewFromBytes(docx, name: 'Doc.docx');
    await expectLater(
        service.restyleOpenDeck(const DeckStyleChoice(design: 'ivory')),
        throwsA(isA<Exception>()));
  });
}
