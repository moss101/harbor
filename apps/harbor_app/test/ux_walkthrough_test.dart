import 'dart:convert' show jsonDecode;
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/main.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_app/services/preferences.dart';
import 'package:harbor_ui/harbor_ui.dart';

final repoRoot = Directory.current.parent.parent.path; // apps/harbor_app
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';
final coreAvailable = File(dylibPath).existsSync();

/// UX walkthrough evidence (ACC-033/034/035): the three P0 UX gates whose
/// substance is a user journey, executed as real-dylib widget tests — the
/// same flows a human walkthrough follows, machine-verifiable.
Future<void> pumpApp(WidgetTester tester, HarborService? service) async {
  tester.view.physicalSize = const Size(1280, 800);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(MaterialApp(
    localizationsDelegates: const [
      AppLocalizations.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    supportedLocales: const [Locale('en')],
    theme: harborThemeData(dark: false, arabic: false),
    builder: (_, child) => HarborTheme(
      colors: HarborColors.light,
      text: const HarborType(arabic: false),
      child: child!,
    ),
    home: HarborApp(service: service, preferences: MemoryPreferencesStore()),
  ));
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 200));
}

void main() {
  late HarborService service;

  setUpAll(() async {
    if (!coreAvailable) return;
    final dir = await Directory.systemTemp.createTemp('harbor-ux-test-');
    service = await HarborService.open(
        libraryPath: dylibPath,
        dataRoot: dir.path,
        workspaceId: 'ws-ux',
        deviceRootHex:
            'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
    // Import the committed signed catalog from fixtures (same document the
    // app bundles) so the Recommended tab has real entries.
    final doc = jsonDecode(await File(
            '$repoRoot/fixtures/catalog/signed_catalog.json')
        .readAsString()) as Map<String, dynamic>;
    final root =
        await File('$repoRoot/fixtures/catalog/root_public.hex').readAsString();
    final imported = await service.importCatalog(doc, rootPublicHex: root.trim());
    expect(imported, isTrue, reason: 'signed catalog must import for walkthroughs');
    // Durable run for the Activity walkthrough (real FFI calls must run
    // outside the widget tests' fake-async zone).
    final run = await service.callForTest('run.create', {'run_id': 'ux-trail-run'});
    expect(run['run_id'], 'ux-trail-run');
    await service.callForTest('run.log_request',
        {'run_id': 'ux-trail-run', 'text': 'Summarize the quarterly report'});
    await service.refresh();
  });

  tearDownAll(() async {
    if (coreAvailable) await service.close();
  });

  testWidgets(
      'ACC-033 walkthrough: novice installs without quantization jargon '
      'until Advanced is expanded', (tester) async {
    if (!coreAvailable) return;
    await pumpApp(tester, service);
    // Go to Models → Recommended.
    await tester.tap(find.text('Models').first);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    // A real catalog entry renders (qwen chat model first).
    expect(find.text('qwen2.5-1.5b-instruct'), findsOneWidget);
    // NOVICE: no quantization jargon on the card.
    expect(find.textContaining('Q4_K_M'), findsNothing);
    expect(find.textContaining('4096 tokens context'), findsNothing);
    // The Advanced affordance exists and reveals the jargon on demand.
    // One per visible catalog card: 7 signed packages minus the hidden
    // test-tier model (epoch 5 added embeddinggemma-2 and -multimodal).
    expect(find.text('Advanced details'), findsNWidgets(6));
    await tester.tap(find
        .byKey(const ValueKey('catalog-advanced-qwen2.5-1.5b-instruct')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.textContaining('Q4_K_M'), findsOneWidget);
  });

  testWidgets(
      'ACC-034 walkthrough: expert inspects repo/revision/quant/context/'
      'backend/license/files BEFORE install', (tester) async {
    if (!coreAvailable) return;
    await pumpApp(tester, service);
    await tester.tap(find.text('Models').first);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    await tester.tap(find
        .byKey(const ValueKey('catalog-advanced-qwen2.5-1.5b-instruct')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));
    // Every install-relevant fact is inspectable pre-install.
    expect(find.textContaining('Qwen/Qwen2.5-1.5B-Instruct-GGUF'),
        findsOneWidget); // repo
    expect(find.textContaining('revision main'), findsOneWidget);
    expect(find.textContaining('Q4_K_M'), findsOneWidget); // quantization
    expect(find.textContaining('4096 tokens context'), findsOneWidget);
    expect(find.textContaining('gguf/llama.cpp'), findsOneWidget); // backend
    expect(find.textContaining('Apache-2.0'), findsWidgets); // license
    // Pinned file with its sha prefix (identity before download).
    expect(find.textContaining('qwen2.5-1.5b-instruct-q4_k_m.gguf'),
        findsOneWidget);
    expect(find.textContaining('6a1a2eb6d156'), findsOneWidget);
    // And install is offered on the same card.
    expect(find.byKey(const ValueKey('catalog-install-qwen2.5-1.5b-instruct')),
        findsOneWidget);
  });

  testWidgets(
      'ACC-035 walkthrough: Activity lists the run with readable status '
      '(trail expandability is the harbor_ui contract test)', (tester) async {
    if (!coreAvailable) return;
    await pumpApp(tester, service);
    await tester.tap(find.text('Activity').first);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 600));
    // The durable run is listed under Activity with its readable state.
    expect(find.textContaining('ux-trail-run'), findsWidgets);
    // The trail's user-readable + expandable-technical contract is
    // executed in packages/harbor_ui/test/trail_test.dart (UX-016).
  });
}
