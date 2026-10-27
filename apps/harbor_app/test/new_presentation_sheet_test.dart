import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_app/services/preferences.dart';
import 'package:harbor_app/surfaces/work/deck_designer_labels.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/deck_designer.dart';
import 'package:harbor_ui/deck_designer_testing.dart';
import 'package:harbor_ui/harbor_ui.dart';

Widget host(
  Widget home, {
  Locale locale = const Locale('en'),
  double textScale = 1,
}) {
  return MaterialApp(
    locale: locale,
    localizationsDelegates: const [
      AppLocalizations.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    supportedLocales: AppLocalizations.supportedLocales,
    theme: harborThemeData(dark: false, arabic: locale.languageCode == 'ar'),
    builder: (context, c) => MediaQuery(
      data: MediaQuery.of(context)
          .copyWith(textScaler: TextScaler.linear(textScale)),
      child: HarborTheme(
        colors: HarborColors.light,
        text: HarborType(arabic: locale.languageCode == 'ar'),
        child: HarborServiceProvider(
          service: null,
          failed: false,
          child: c!,
        ),
      ),
    ),
    home: home,
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
                labels: deckDesignerLabels(AppLocalizations.of(context)!),
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

  // The sheet's behaviour is covered in packages/harbor_ui; these prove the
  // app hands it its own localisations, preferences and error text.
  Widget sheet({DeckDesignPrefs? initial, ValueChanged<DeckDesignPrefs>? on}) =>
      Builder(
          builder: (context) => NewPresentationSheet(
              designer: designer,
              labels: deckDesignerLabels(AppLocalizations.of(context)!),
              initial: initial ?? const DeckDesignPrefs(),
              onRemember: on));

  testWidgets('opens on the purpose step with the app\'s English strings',
      (tester) async {
    size(tester, 1200, 800);
    await tester.pumpWidget(host(sheet()));
    expect(find.text('Loading designs…'), findsOneWidget);
    await settle(tester);
    expect(find.text('New presentation'), findsWidgets);
    expect(find.text('What is this presentation for?'), findsOneWidget);
    expect(find.text('Each purpose comes with a ready-made storyline.'),
        findsOneWidget);
    expect(find.text('Pitch deck'), findsOneWidget);
    expect(find.text('Preview'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
      'create returns the starter and remembers the choices in '
      'the app\'s DeckDesignPrefs', (tester) async {
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
    expect(designer.starters.single.purpose, 'project_report');
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

  testWidgets('remembered preferences from the app restore the choices',
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

  testWidgets('a core error shows the core\'s message, not the wrapper',
      (tester) async {
    size(tester, 1200, 800);
    designer.failStarter = 1;
    await tester.pumpWidget(host(sheet()));
    await settle(tester);
    await next(tester);
    await next(tester);
    await tester.tap(find.text('Create presentation'));
    await settle(tester);
    expect(find.text("Couldn't create the presentation"), findsOneWidget);
    expect(find.textContaining('the core said no'), findsOneWidget);
  });

  test('deckErrorText unwraps the core\'s exception', () {
    expect(deckErrorText(ffi.HarborCoreException('bad deck')), 'bad deck');
    expect(deckErrorText(StateError('x')), contains('x'));
  });

  testWidgets(
      'layout: two panes wide, stepper on a phone, no overflow at '
      'large text', (tester) async {
    size(tester, 900, 800);
    await tester.pumpWidget(host(sheet()));
    await settle(tester);
    expect(find.byKey(const ValueKey('deck-preview-slide')), findsOneWidget);
    size(tester, 360, 740);
    await tester.pumpWidget(
        KeyedSubtree(key: UniqueKey(), child: host(sheet(), textScale: 2)));
    await settle(tester);
    expect(find.byKey(const ValueKey('deck-preview-slide')), findsNothing);
    for (var i = 0; i < 2; i++) {
      expect(tester.takeException(), isNull, reason: 'step $i');
      await tester.tap(find.widgetWithText(FilledButton, 'Next'));
      await settle(tester);
    }
    expect(tester.takeException(), isNull);
  });

  testWidgets('Arabic (RTL) uses the Arabic strings on a phone and wide',
      (tester) async {
    for (final w in [360.0, 1200.0]) {
      size(tester, w, 740);
      await tester.pumpWidget(KeyedSubtree(
          key: UniqueKey(), child: host(sheet(), locale: const Locale('ar'))));
      await settle(tester);
      expect(find.text('الغرض'), findsWidgets);
      await tester.tap(find.byType(FilledButton).first);
      await settle(tester);
      await tester.tap(find.byType(FilledButton).first);
      await settle(tester);
      expect(tester.takeException(), isNull, reason: 'width $w');
    }
  });

  testWidgets('semantics: steps are labelled with the app\'s strings',
      (tester) async {
    final handle = tester.ensureSemantics();
    size(tester, 1200, 800);
    await tester.pumpWidget(host(sheet()));
    await settle(tester);
    expect(find.bySemanticsLabel('Step 1 of 3: Purpose'), findsOneWidget);
    expect(find.bySemanticsLabel('Pitch deck, Raise a round.'), findsOneWidget);
    handle.dispose();
  });

  test('every locale has translated designer strings (not English copies)',
      () async {
    final en = await AppLocalizations.delegate.load(const Locale('en'));
    for (final l in AppLocalizations.supportedLocales) {
      if (l.languageCode == 'en') continue;
      final t = await AppLocalizations.delegate.load(l);
      final pairs = {
        'deckCreate': (en.deckCreate, t.deckCreate),
        'deckNext': (en.deckNext, t.deckNext),
        'deckRetry': (en.deckRetry, t.deckRetry),
        'deckPurposeHeading': (en.deckPurposeHeading, t.deckPurposeHeading),
        'deckChangeDesignDisabled': (
          en.deckChangeDesignDisabled,
          t.deckChangeDesignDisabled
        ),
        'workNewPresentation': (en.workNewPresentation, t.workNewPresentation),
        'deckMotionCinematicDesc': (
          en.deckMotionCinematicDesc,
          t.deckMotionCinematicDesc
        ),
      };
      pairs.forEach(
          (k, v) => expect(v.$2, isNot(v.$1), reason: '${l.languageCode} $k'));
      expect(t.deckStepOf(2, 3), contains('2'), reason: l.languageCode);
      expect(t.deckCurrentDesign('X'), contains('X'), reason: l.languageCode);
    }
  });

  testWidgets('preferences round-trip the deck choices', (tester) async {
    const prefs = HarborPreferences(
        deckDesign: DeckDesignPrefs(
            purpose: 'pitch_deck',
            design: 'ocean',
            palette: 'mint',
            motion: 'none',
            rtl: true));
    final back = HarborPreferences.fromJson(prefs.toJson());
    expect(back.deckDesign, prefs.deckDesign);
    expect(back, prefs);
    expect(HarborPreferences.fromJson(const {}).deckDesign, isNull);
  });
}
