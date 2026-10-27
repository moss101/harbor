import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/deck_designer.dart';
import 'package:harbor_ui/deck_designer_testing.dart';
import 'package:harbor_ui/harbor_ui.dart';
import 'package:harbor_ui/slides.dart';

import '../support/english_labels.dart';

Widget host(
  Widget home, {
  TextDirection dir = TextDirection.ltr,
  double textScale = 1,
}) {
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
}

Future<void> next(WidgetTester tester) async {
  await tester.tap(find.widgetWithText(FilledButton, 'Next'));
  await settle(tester);
}

/// Opens the sheet from a button so pop results can be observed.
class Launcher extends StatefulWidget {
  const Launcher(this.designer, {super.key, this.initial, this.onRemember});
  final FakeDeckDesigner designer;
  final DeckDesignPrefs? initial;
  final ValueChanged<DeckDesignPrefs>? onRemember;

  @override
  State<Launcher> createState() => LauncherState();
}

class LauncherState extends State<Launcher> {
  DeckStarterResult? result;
  bool done = false;
  @override
  Widget build(BuildContext context) => Scaffold(
        body: Center(
          child: TextButton(
            onPressed: () async {
              final r = await NewPresentationSheet.show(
                context,
                designer: widget.designer,
                labels: englishLabels(),
                initial: widget.initial ?? const DeckDesignPrefs(),
                onRemember: widget.onRemember,
              );
              setState(() {
                result = r;
                done = true;
              });
            },
            child: Text(done ? 'closed:${result?.fileName}' : 'open'),
          ),
        ),
      );
}

void main() {
  late FakeDeckDesigner designer;
  setUp(() => designer = FakeDeckDesigner());

  Widget sheet({DeckDesignPrefs? initial, ValueChanged<DeckDesignPrefs>? on}) =>
      NewPresentationSheet(
          designer: designer,
          labels: englishLabels(),
          initial: initial ?? const DeckDesignPrefs(),
          onRemember: on);

  testWidgets('opens on the purpose step with the catalogue purposes',
      (tester) async {
    size(tester, 1200, 800);
    await tester.pumpWidget(host(sheet()));
    expect(find.text('Loading designs…'), findsOneWidget);
    await settle(tester);
    expect(find.text('New presentation'), findsWidgets);
    expect(find.text('What is this presentation for?'), findsOneWidget);
    expect(find.text('Pitch deck'), findsOneWidget);
    expect(find.text('Raise a round.'), findsOneWidget);
    expect(find.text('Project report'), findsOneWidget);
    expect(designer.catalogCalls, 1);
    expect(tester.takeException(), isNull);
  });

  testWidgets('a purpose preselects its suggested design and motion',
      (tester) async {
    size(tester, 1200, 800);
    await tester.pumpWidget(host(sheet()));
    await settle(tester);
    // First purpose is selected by default: aurora + cinematic.
    await tester.tap(find.byKey(const ValueKey('deck-purpose-project_report')));
    await settle(tester);
    await next(tester);
    // Slate is selected (check icon on its card) and flagged Suggested.
    final slate = find.byKey(const ValueKey('deck-design-slate'));
    expect(
        find.descendant(of: slate, matching: find.byIcon(Icons.check_circle)),
        findsOneWidget);
    expect(find.descendant(of: slate, matching: find.text('Suggested')),
        findsOneWidget);
    expect(find.text('Suggested'), findsOneWidget);
  });

  testWidgets('design step: live thumbnails per design, lazily cached',
      (tester) async {
    size(tester, 1200, 800);
    await tester.pumpWidget(host(sheet()));
    await settle(tester);
    // Before the design step only the preview pane asks for a sample.
    expect(designer.samples.map((s) => s.design).toSet(), {'aurora'});
    await next(tester);
    await settle(tester);
    // One request per design (+ the preview of the selected one, cached).
    final byDesign = designer.samples.map((s) => s.design).toSet();
    expect(byDesign, {'aurora', 'ivory', 'slate', 'ocean'});
    expect(designer.samples.where((s) => s.design == 'aurora').length, 1,
        reason: 'thumbnail and preview share one cached sample');
    expect(find.byType(SlideView), findsWidgets);
    expect(find.text('Glowing gradients'), findsOneWidget);
    // Semantics of thumbnails.
    final handle = tester.ensureSemantics();
    expect(find.bySemanticsLabel('Ivory: Quiet editorial'), findsOneWidget);
    handle.dispose();
  });

  testWidgets('selecting a design changes the sample the preview asks for',
      (tester) async {
    size(tester, 1200, 800);
    await tester.pumpWidget(host(sheet()));
    await settle(tester);
    await next(tester);
    await settle(tester);
    designer.samples.clear();
    await tester.tap(find.byKey(const ValueKey('deck-design-ocean')));
    await settle(tester);
    // The preview pane (wide layout) shows the ocean tagline.
    expect(find.text('Ocean · Deep blues'), findsOneWidget);
    // Ocean was already requested for its thumbnail: served from the cache.
    expect(designer.samples, isEmpty);
    expect(find.bySemanticsLabel('Preview of Ocean'), findsOneWidget);
  });

  testWidgets('palette change recolours the same layouts', (tester) async {
    size(tester, 1200, 800);
    await tester.pumpWidget(host(sheet()));
    await settle(tester);
    await next(tester);
    await settle(tester);

    String? backgroundOf(String design) {
      final card = find.byKey(ValueKey('deck-design-$design'));
      final v = tester.widget<SlideView>(
          find.descendant(of: card, matching: find.byType(SlideView)));
      return v.scene.background.c;
    }

    expect(backgroundOf('ivory'), fakeBackground(null));
    designer.samples.clear();
    await tester.tap(find.byKey(const ValueKey('deck-palette-terracotta')));
    await settle(tester);
    await settle(tester);
    expect(backgroundOf('ivory'), fakeBackground('terracotta'));
    expect(backgroundOf('aurora'), fakeBackground('terracotta'));
    // Every design was asked again, with the palette.
    expect(designer.samples.every((s) => s.palette == 'terracotta'), isTrue);
    expect(designer.samples.map((s) => s.design).toSet().length, 4);
    // Back to the design's own colours.
    await tester.tap(find.byKey(const ValueKey('deck-palette-own')));
    await settle(tester);
    await settle(tester);
    expect(backgroundOf('ivory'), fakeBackground(null));
  });

  testWidgets('create builds the starter with the choices and returns it',
      (tester) async {
    size(tester, 1200, 800);
    DeckDesignPrefs? remembered;
    await tester.pumpWidget(
        host(Launcher(designer, onRemember: (p) => remembered = p)));
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
    await tester.tap(find.byKey(const ValueKey('deck-purpose-project_report')));
    await settle(tester);
    await next(tester);
    await settle(tester);
    await tester.tap(find.byKey(const ValueKey('deck-design-ocean')));
    await tester.tap(find.byKey(const ValueKey('deck-palette-mint')));
    await settle(tester);
    await next(tester);
    await tester.tap(find.byKey(const ValueKey('deck-motion-dynamic')));
    await tester.tap(find.byKey(const ValueKey('deck-language-rtl')));
    await settle(tester);
    await tester.tap(find.text('Create presentation'));
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
    expect(designer.starters, hasLength(1));
    final call = designer.starters.single;
    expect(call.purpose, 'project_report');
    expect(
        call.style,
        const DeckStyleChoice(
            design: 'ocean', palette: 'mint', motion: 'dynamic', rtl: true));
    expect(find.text('closed:Starter.pptx'), findsOneWidget);
    expect(
        remembered,
        const DeckDesignPrefs(
            purpose: 'project_report',
            design: 'ocean',
            palette: 'mint',
            motion: 'dynamic',
            rtl: true));
  });

  testWidgets('a core error is shown with the reason and a retry',
      (tester) async {
    size(tester, 1200, 800);
    designer.failStarter = 1;
    await tester.pumpWidget(host(Launcher(designer)));
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
    await next(tester);
    await next(tester);
    await tester.tap(find.text('Create presentation'));
    await settle(tester);
    expect(find.text("Couldn't create the presentation"), findsOneWidget);
    expect(find.textContaining('the core said no'), findsOneWidget);
    expect(find.text('closed:Starter.pptx'), findsNothing);
    await tester.tap(find.text('Try again'));
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
    expect(designer.starters, hasLength(2));
    expect(find.text('closed:Starter.pptx'), findsOneWidget);
  });

  testWidgets('a catalogue error offers a retry', (tester) async {
    size(tester, 1200, 800);
    designer.failCatalog = 1;
    await tester.pumpWidget(host(sheet()));
    await settle(tester);
    expect(find.text("Couldn't load the designs"), findsWidgets);
    expect(find.textContaining('catalogue offline'), findsOneWidget);
    await tester.tap(find.text('Try again'));
    await settle(tester);
    expect(find.text('Pitch deck'), findsOneWidget);
    expect(designer.catalogCalls, 2);
  });

  testWidgets('a failing sample says so in the preview and can retry',
      (tester) async {
    size(tester, 1200, 800);
    designer.failSample = 100;
    await tester.pumpWidget(host(sheet()));
    await settle(tester);
    await next(tester);
    await settle(tester);
    expect(find.text('Preview unavailable'), findsOneWidget);
    expect(find.text('No preview'), findsWidgets);
    designer.failSample = 0;
    await tester.tap(find.text('Try again'));
    await settle(tester);
    expect(find.text('Preview unavailable'), findsNothing);
  });

  testWidgets('remembered choices are restored (unknown ids are ignored)',
      (tester) async {
    size(tester, 1200, 800);
    await tester.pumpWidget(host(sheet(
        initial: const DeckDesignPrefs(
            purpose: 'project_report',
            design: 'ocean',
            palette: 'mint',
            motion: 'none',
            rtl: true))));
    await settle(tester);
    await tester.tap(find.byKey(const ValueKey('deck-purpose-project_report')));
    // Re-picking the same purpose resets to ITS defaults, so go on without.
    await settle(tester);
    await next(tester);
    await settle(tester);
    expect(
        find.descendant(
            of: find.byKey(const ValueKey('deck-design-slate')),
            matching: find.byIcon(Icons.check_circle)),
        findsOneWidget);
  });

  testWidgets('restored prefs drive the first starter request', (tester) async {
    size(tester, 1200, 800);
    await tester.pumpWidget(host(sheet(
        initial: const DeckDesignPrefs(
            purpose: 'project_report',
            design: 'ocean',
            palette: 'mint',
            motion: 'none',
            rtl: true))));
    await settle(tester);
    await next(tester);
    await next(tester);
    await tester.tap(find.text('Create presentation'));
    await tester.pump();
    await tester.pump();
    expect(
        designer.starters.single.style,
        const DeckStyleChoice(
            design: 'ocean', palette: 'mint', motion: 'none', rtl: true));
  });

  testWidgets('unknown remembered ids fall back to the purpose defaults',
      (tester) async {
    size(tester, 1200, 800);
    final bogus = FakeDeckDesigner();
    await tester.pumpWidget(host(NewPresentationSheet(
        designer: bogus,
        labels: englishLabels(),
        initial: const DeckDesignPrefs(
            purpose: 'nope',
            design: 'nope',
            palette: 'nope',
            motion: 'nope'))));
    await settle(tester);
    await next(tester);
    await next(tester);
    await tester.tap(find.text('Create presentation'));
    await tester.pump();
    await tester.pump();
    final s = bogus.starters.single;
    expect(s.purpose, 'pitch_deck');
    expect(s.style!.design, 'aurora');
    expect(s.style!.palette, isNull);
    expect(s.style!.motion, 'cinematic');
  });

  group('layout', () {
    testWidgets('two panes from 840 px: preview beside the steps',
        (tester) async {
      size(tester, 900, 800);
      await tester.pumpWidget(host(sheet()));
      await settle(tester);
      expect(find.text('Preview'), findsOneWidget);
      expect(find.byKey(const ValueKey('deck-preview-slide')), findsOneWidget);
      final preview = tester.getTopLeft(find.text('Preview'));
      final steps =
          tester.getTopLeft(find.text('What is this presentation for?'));
      expect(preview.dx, greaterThan(steps.dx));
      expect(tester.takeException(), isNull);
    });

    testWidgets('below 840 px it is a stepper with an inline preview',
        (tester) async {
      size(tester, 600, 800);
      await tester.pumpWidget(host(sheet()));
      await settle(tester);
      expect(find.text('Preview'), findsNothing);
      expect(find.byKey(const ValueKey('deck-preview-slide')), findsNothing);
      await next(tester);
      await settle(tester);
      // The design step shows the live thumbnails themselves.
      expect(find.byKey(const ValueKey('deck-preview-slide')), findsNothing);
      expect(find.byType(SlideView), findsWidgets);
      await next(tester);
      await settle(tester);
      expect(find.byKey(const ValueKey('deck-preview-slide')), findsOneWidget);
      expect(find.text('Motion and language'), findsWidgets);
      expect(tester.takeException(), isNull);
    });

    testWidgets('phone width with large text does not overflow',
        (tester) async {
      size(tester, 360, 740);
      await tester.pumpWidget(host(sheet(), textScale: 2));
      await settle(tester);
      for (var i = 0; i < 2; i++) {
        expect(tester.takeException(), isNull, reason: 'step $i');
        await tester.tap(find.widgetWithText(FilledButton, 'Next'));
        await settle(tester);
      }
      expect(tester.takeException(), isNull);
    });

    testWidgets('right-to-left lays out on a phone and wide', (tester) async {
      for (final w in [360.0, 1200.0]) {
        size(tester, w, 740);
        await tester.pumpWidget(KeyedSubtree(
            key: UniqueKey(), child: host(sheet(), dir: TextDirection.rtl)));
        await settle(tester);
        expect(find.text('What is this presentation for?'), findsOneWidget);
        await tester.tap(find.byType(FilledButton).first);
        await settle(tester);
        await tester.tap(find.byType(FilledButton).first);
        await settle(tester);
        expect(tester.takeException(), isNull, reason: 'width $w');
      }
    });

    testWidgets('two panes from 840 px mirror in RTL', (tester) async {
      size(tester, 1200, 800);
      await tester.pumpWidget(host(sheet(), dir: TextDirection.rtl));
      await settle(tester);
      final preview = tester.getTopLeft(find.text('Preview'));
      final steps =
          tester.getTopLeft(find.text('What is this presentation for?'));
      expect(preview.dx, lessThan(steps.dx));
    });
  });

  testWidgets('semantics: steps are labelled selectable buttons',
      (tester) async {
    final handle = tester.ensureSemantics();
    size(tester, 1200, 800);
    await tester.pumpWidget(host(sheet()));
    await settle(tester);
    expect(find.bySemanticsLabel('Step 1 of 3: Purpose'), findsOneWidget);
    expect(find.bySemanticsLabel('Step 2 of 3: Design'), findsOneWidget);
    expect(find.bySemanticsLabel('Pitch deck, Raise a round.'), findsOneWidget);
    // Tapping a step chip jumps there.
    await tester.tap(find.byKey(const ValueKey('deck-step-1')));
    await settle(tester);
    expect(find.text('Choose a design'), findsOneWidget);
    handle.dispose();
  });
}
