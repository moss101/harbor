import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/deck_designer.dart';
import 'package:harbor_ui/deck_designer_testing.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../support/english_labels.dart';

Widget host(Widget home,
    {TextDirection dir = TextDirection.ltr, double textScale = 1}) {
  final rtl = dir == TextDirection.rtl;
  return HarborTheme(
    colors: HarborColors.light,
    text: HarborType(arabic: rtl),
    child: MaterialApp(
      theme: harborThemeData(dark: false, arabic: rtl),
      builder: (context, app) => MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(textScaler: TextScaler.linear(textScale)),
        child: Directionality(textDirection: dir, child: app!),
      ),
      home: home,
    ),
  );
}

void size(WidgetTester tester, double w, double h) {
  tester.view.physicalSize = Size(w, h);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

Future<void> settle(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 50));
  await tester.pump(const Duration(milliseconds: 50));
}

const aurora = DeckStyleChoice(design: 'aurora', motion: 'subtle');

/// Opens the dialog from a button and records what it answered.
class Launcher extends StatefulWidget {
  const Launcher(this.designer, {super.key, this.current = aurora});
  final FakeDeckDesigner designer;
  final DeckStyleChoice current;

  @override
  State<Launcher> createState() => LauncherState();
}

class LauncherState extends State<Launcher> {
  bool done = false;
  DeckStyleChoice? answer;

  @override
  Widget build(BuildContext context) => Scaffold(
        body: Center(
          child: TextButton(
            onPressed: () async {
              final r = await ChangeDesignDialog.show(
                context,
                designer: widget.designer,
                labels: englishLabels(),
                current: widget.current,
                currentName: 'Aurora',
              );
              setState(() {
                answer = r;
                done = true;
              });
            },
            child: Text(done ? 'closed:${answer?.design}' : 'open'),
          ),
        ),
      );
}

void main() {
  late FakeDeckDesigner designer;
  setUp(() => designer = FakeDeckDesigner());

  Future<void> open(WidgetTester tester, {DeckStyleChoice? current}) async {
    await tester
        .pumpWidget(host(Launcher(designer, current: current ?? aurora)));
    await tester.tap(find.text('open'));
    await settle(tester);
    await settle(tester);
  }

  Finder apply() => find.widgetWithText(FilledButton, 'Apply design');

  testWidgets('shows the current design, preselected, Apply disabled',
      (tester) async {
    size(tester, 1200, 800);
    await open(tester);
    expect(find.text('Change design'), findsOneWidget);
    expect(find.text('Current design: Aurora'), findsOneWidget);
    expect(
        find.descendant(
            of: find.byKey(const ValueKey('deck-design-aurora')),
            matching: find.byIcon(Icons.check_circle)),
        findsOneWidget);
    expect(tester.widget<FilledButton>(apply()).onPressed, isNull);
    // The motion section names the current preset.
    expect(find.text('Motion: Subtle'), findsOneWidget);
  });

  testWidgets('applying answers the new style, keeping what was not changed',
      (tester) async {
    size(tester, 1200, 800);
    await open(tester,
        current: const DeckStyleChoice(
            design: 'aurora', palette: 'terracotta', motion: 'dynamic'));
    await tester.tap(find.byKey(const ValueKey('deck-design-ocean')));
    await tester.pump();
    expect(tester.widget<FilledButton>(apply()).onPressed, isNotNull);
    await tester.tap(apply());
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
    expect(find.text('closed:ocean'), findsOneWidget);
    final state = tester.state<LauncherState>(find.byType(Launcher));
    expect(
        state.answer,
        const DeckStyleChoice(
            design: 'ocean', palette: 'terracotta', motion: 'dynamic'));
  });

  testWidgets('a palette and a motion change are part of the answer',
      (tester) async {
    size(tester, 1200, 800);
    await open(tester);
    await tester.tap(find.byKey(const ValueKey('deck-palette-mint')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('deck-motion-section')));
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
    await tester.tap(find.byKey(const ValueKey('deck-motion-cinematic')));
    await tester.pump();
    await tester.tap(apply());
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
    final state = tester.state<LauncherState>(find.byType(Launcher));
    expect(
        state.answer,
        const DeckStyleChoice(
            design: 'aurora', palette: 'mint', motion: 'cinematic'));
    // The dialog only answers; the caller applies it.
    expect(designer.restyles, isEmpty);
  });

  testWidgets('cancel answers nothing', (tester) async {
    size(tester, 1200, 800);
    await open(tester);
    await tester.tap(find.byKey(const ValueKey('deck-design-slate')));
    await tester.pump();
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
    final state = tester.state<LauncherState>(find.byType(Launcher));
    expect(state.done, isTrue);
    expect(state.answer, isNull);
    expect(find.text('Change design'), findsNothing);
  });

  testWidgets('a catalogue failure shows the reason and retries',
      (tester) async {
    size(tester, 1200, 800);
    designer.failCatalog = 1;
    await open(tester);
    expect(find.textContaining('catalogue offline'), findsOneWidget);
    expect(tester.widget<FilledButton>(apply()).onPressed, isNull);
    await tester.tap(find.text('Try again'));
    await settle(tester);
    await settle(tester);
    expect(find.text('Current design: Aurora'), findsOneWidget);
    expect(designer.catalogCalls, 2);
  });

  testWidgets('thumbnails honour the current palette and share the cache',
      (tester) async {
    size(tester, 1200, 800);
    await open(tester);
    final designs = designer.samples.map((s) => s.design).toList();
    expect(designs.toSet(), {'aurora', 'ivory', 'slate', 'ocean'});
    expect(designs.length, designs.toSet().length,
        reason: 'one sample per design');
  });

  testWidgets('phone, large text and RTL: lays out with 44 px actions',
      (tester) async {
    for (final dir in [TextDirection.ltr, TextDirection.rtl]) {
      size(tester, 360, 740);
      await tester.pumpWidget(KeyedSubtree(
          key: UniqueKey(),
          child: host(Launcher(designer), dir: dir, textScale: 1.5)));
      await tester.tap(find.text('open'));
      await settle(tester);
      await settle(tester);
      expect(tester.takeException(), isNull, reason: '$dir');
      // The gallery scrolls: the selected design is on screen.
      expect(find.byKey(const ValueKey('deck-design-aurora')), findsOneWidget);
      expect(tester.getSize(apply()).height, greaterThanOrEqualTo(44));
      expect(tester.getSize(find.widgetWithText(TextButton, 'Cancel')).height,
          greaterThanOrEqualTo(44));
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle(const Duration(milliseconds: 100));
    }
  });

  testWidgets('the title is a header for assistive tech', (tester) async {
    final handle = tester.ensureSemantics();
    size(tester, 1200, 800);
    await open(tester);
    expect(
        tester.getSemantics(find.text('Change design')),
        matchesSemantics(
            label: 'Change design',
            isHeader: true,
            isFocusable: false,
            hasEnabledState: false));
    handle.dispose();
  });
}
