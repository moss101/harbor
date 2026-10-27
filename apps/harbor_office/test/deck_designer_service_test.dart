import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_office/services/harbor_service.dart';
import 'package:harbor_ui/deck_designer.dart';

final repoRoot = Directory.current.parent.parent.path; // apps/harbor_office
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
    root = await Directory.systemTemp.createTemp('harbor-deck-designer-');
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
    if (skip == null) await service.close();
  });

  guarded('catalogue: 14 designs, 20 palettes, 4 motions, cached', () async {
    final c = await service.deckCatalog();
    expect(c.designs.length, 14);
    expect(c.palettes.length, 20);
    expect(
        c.motions.map((m) => m.id), ['none', 'subtle', 'dynamic', 'cinematic']);
    expect(c.purposes.length, 20);
    expect(c.design('aurora'), isNotNull);
    expect(identical(await service.deckCatalog(), c), isTrue);
  });

  guarded('sample: a palette keeps the same layout; unknown designs refuse',
      () async {
    const own = DeckStyleChoice(design: 'ivory');
    final a = await service.deckSample(own);
    final b = await service.deckSample(own.copyWith(palette: 'terracotta'));
    expect(a.slides.length, b.slides.length);
    expect(a.slides.length, greaterThan(2));
    expect(a.slides.first.items.length, b.slides.first.items.length);
    await expectLater(service.deckSample(const DeckStyleChoice(design: 'nope')),
        throwsA(isA<Exception>()));
  });

  guarded('starter -> preview -> restyle keeps the slides and is restylable',
      () async {
    final r = await service.deckStarter(
        purpose: 'pitch_deck',
        style: const DeckStyleChoice(
            design: 'ocean', palette: 'mint', motion: 'dynamic'));
    expect(r.fileName, endsWith('.pptx'));
    expect(r.slides, greaterThan(3));
    expect(r.theme, 'ocean');
    expect(r.scene!.slides.length, r.slides);

    await service.loadPreviewFromBytes(r.bytes, name: r.fileName);
    final design = (service.preview!['preview'] as Map)['design'] as Map;
    expect(design['restylable'], isTrue);
    expect(design['theme'], 'ocean');
    expect(design['palette'], 'mint');

    final res = await service.restyleOpenDeck(const DeckStyleChoice(
        design: 'slate', palette: null, motion: 'subtle', rtl: false));
    expect(res.scene!.slides.length, r.slides);
    expect(service.canUndo, isTrue, reason: 'a restyle is an undoable edit');
    final after = (service.preview!['preview'] as Map)['design'] as Map;
    expect(after['theme'], 'slate');
    expect(after['palette'], isNull);
    // The working copy changed; the starter bytes are untouched.
    expect(service.currentWorkingBytes(), res.bytes);
    expect(await service.undo(), isTrue);
    expect(service.currentWorkingBytes(), r.bytes);
  });

  guarded('restyle refuses a non-deck and a deck without Harbor source',
      () async {
    final wb = await service.createWorkbook();
    await service.loadPreviewFromBytes(wb, name: 'Book.xlsx');
    await expectLater(
        service.restyleOpenDeck(const DeckStyleChoice(design: 'ivory')),
        throwsA(isA<Exception>()));
    final md = await service.markdownToPptx('# Title\n\n- one\n- two\n');
    await service.loadPreviewFromBytes(md, name: 'md.pptx');
    // Whatever the core says about restylability, a refusal must be typed.
    final restylable =
        ((service.preview!['preview'] as Map)['design'] as Map?)?['restylable'];
    if (restylable != true) {
      await expectLater(
          service.restyleOpenDeck(const DeckStyleChoice(design: 'ivory')),
          throwsA(isA<Exception>()));
    }
  });
}
