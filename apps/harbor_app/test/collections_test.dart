import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/surfaces/home_surface.dart';
import 'package:harbor_app/surfaces/knowledge_surface.dart';

import 'fake_service.dart';

/// Library collections: a source can be grouped (menu on its row), the
/// list filters by collection, and the Home composer's scope chip becomes
/// a menu whose collection choice grounds the answer ONLY on that
/// collection (scripted core).
void main() {
  const List<Map<String, dynamic>> chat = [
    {'id': 'qwen-chat'},
  ];

  List<Map<String, dynamic>> sources() => [
        {
          'source_id': 'contract',
          'title': 'Contract',
          'chunks': 2,
          'bytes': 2048,
          'collection': 'Finance',
        },
        {
          'source_id': 'leave',
          'title': 'Leave policy',
          'chunks': 1,
          'bytes': 1024,
          'collection': null,
        },
        {
          'source_id': 'travel',
          'title': 'Travel',
          'chunks': 1,
          'bytes': 1024,
          'collection': 'Finance',
        },
      ];

  testWidgets('the Library shows, assigns, creates and clears collections',
      (tester) async {
    final s = FakeHarborService(models: chat)..setSources(sources());
    await pumpSurface(tester, s, (_) => const KnowledgeSurface());
    await tester.pump();

    // The collection shows on each source; the filter lists it with a count.
    expect(find.textContaining('Finance'), findsWidgets);
    expect(find.byKey(const ValueKey('collection-filter')), findsOneWidget);
    expect(find.text('Finance · 2'), findsOneWidget);

    // Assign "Leave policy" to the existing collection from its row menu.
    await tester
        .tap(find.byKey(const ValueKey('source-collection-menu-leave')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('collection-item-Finance')));
    await tester.pumpAndSettle();
    expect(s.calls, contains('setSourceCollection:leave:Finance'));
    expect(find.text('Finance · 3'), findsOneWidget);

    // New collection… asks for a name; a different case adopts the
    // existing spelling (the core's rule, mirrored by the fake).
    await tester
        .tap(find.byKey(const ValueKey('source-collection-menu-travel')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('collection-new')));
    await tester.pumpAndSettle();
    await tester.enterText(
        find.byKey(const ValueKey('collection-name-field')), '  Trips  ');
    await tester.tap(find.byKey(const ValueKey('collection-name-confirm')));
    await tester.pumpAndSettle();
    expect(s.calls, contains('setSourceCollection:travel:Trips'));
    expect(find.text('Trips · 1'), findsOneWidget);
    expect(find.text('Finance · 2'), findsOneWidget);

    // Clearing takes the source out of its collection.
    await tester
        .tap(find.byKey(const ValueKey('source-collection-menu-travel')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('collection-clear')));
    await tester.pumpAndSettle();
    expect(s.calls, contains('setSourceCollection:travel:-'));
    expect(find.text('Trips · 1'), findsNothing);
  });

  testWidgets('a refused change is said, not swallowed', (tester) async {
    final s = FakeHarborService(models: chat)
      ..setSources(sources())
      ..collectionsAccepted = false;
    await pumpSurface(tester, s, (_) => const KnowledgeSurface());
    await tester.pump();
    await tester
        .tap(find.byKey(const ValueKey('source-collection-menu-leave')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('collection-item-Finance')));
    await tester.pumpAndSettle();
    expect(
        find.textContaining('Could not change the collection'), findsOneWidget);
  });

  testWidgets('the collection chips filter the source list', (tester) async {
    final s = FakeHarborService(models: chat)..setSources(sources());
    await pumpSurface(tester, s, (_) => const KnowledgeSurface());
    await tester.pump();
    expect(find.text('Leave policy'), findsOneWidget);
    expect(find.text('Contract'), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('collection-filter-Finance')));
    await tester.pump();
    expect(find.text('Contract'), findsOneWidget);
    expect(find.text('Travel'), findsOneWidget);
    expect(find.text('Leave policy'), findsNothing);

    await tester.tap(find.byKey(const ValueKey('collection-filter-all')));
    await tester.pump();
    expect(find.text('Leave policy'), findsOneWidget);
  });

  testWidgets('without collections the Library shows no filter row',
      (tester) async {
    final s = FakeHarborService(models: chat)
      ..setSources([
        {'source_id': 'a', 'title': 'A', 'chunks': 1, 'bytes': 10},
      ]);
    await pumpSurface(tester, s, (_) => const KnowledgeSurface());
    await tester.pump();
    expect(find.byKey(const ValueKey('collection-filter')), findsNothing);
  });

  Future<void> send(WidgetTester tester, String text) async {
    await tester.enterText(find.byKey(const ValueKey('composer-field')), text);
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('composer-send')));
    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
  }

  testWidgets(
      'Home: the scope chip is a menu of Everything and each collection; a '
      'collection-scoped question is answered only from it and says so',
      (tester) async {
    final s = FakeHarborService(models: chat)..setSources(sources());
    await pumpSurface(tester, s, (state) => HomeSurface(state: state));
    await tester.pump();

    // Default: Everything; a question passes no collection.
    expect(find.text('Everything'), findsOneWidget);
    await send(tester, 'What is the contract value?');
    expect(s.generateArgs.last['collection'], isNull);
    expect(find.byKey(const ValueKey('answer-collection-scope')), findsNothing);

    // Choose the collection in the chip menu.
    await tester.tap(find.byKey(const ValueKey('composer-scope-menu')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('composer-scope-everything-')),
        findsOneWidget);
    await tester
        .tap(find.byKey(const ValueKey('composer-scope-collection-Finance')));
    await tester.pumpAndSettle();
    expect(find.text('Collection · Finance'), findsOneWidget);

    await send(tester, 'What is the travel limit?');
    expect(s.generateArgs.last['collection'], 'Finance');
    expect(
        find.byKey(const ValueKey('answer-collection-scope')), findsOneWidget);
    expect(
        find.text('Answered only from the collection Finance'), findsOneWidget);
  });

  testWidgets('Home: a collection-scoped retrieval-only answer passes it too',
      (tester) async {
    // No chat model: the Library evidence alone, still scoped.
    final s = FakeHarborService()..setSources(sources());
    await pumpSurface(tester, s, (state) => HomeSurface(state: state));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('composer-scope-menu')));
    await tester.pumpAndSettle();
    await tester
        .tap(find.byKey(const ValueKey('composer-scope-collection-Finance')));
    await tester.pumpAndSettle();
    await send(tester, 'What is the contract value?');
    expect(s.searchCollections, contains('Finance'));
  });

  testWidgets('Home: with no collections the scope chip is a plain chip',
      (tester) async {
    final s = FakeHarborService(models: chat)
      ..setSources([
        {'source_id': 'a', 'title': 'A', 'chunks': 1, 'bytes': 10},
      ]);
    await pumpSurface(tester, s, (state) => HomeSurface(state: state));
    await tester.pump();
    expect(find.byKey(const ValueKey('composer-scope')), findsOneWidget);
    expect(find.byKey(const ValueKey('composer-scope-menu')), findsNothing);
  });

  testWidgets('phone width and Arabic: Library and Home scope lay out cleanly',
      (tester) async {
    final s = FakeHarborService(models: chat)..setSources(sources());
    await pumpSurface(tester, s, (_) => const KnowledgeSurface(),
        size: const Size(390, 844), locale: const Locale('ar'));
    await tester.pump();
    expect(find.byKey(const ValueKey('collection-filter')), findsOneWidget);
    expect(tester.takeException(), isNull);

    await pumpSurface(tester, s, (state) => HomeSurface(state: state),
        size: const Size(390, 844), locale: const Locale('ar'));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('composer-scope-menu')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('composer-scope-collection-Finance')),
        findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
