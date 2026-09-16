import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/harbor_ui.dart';

Widget host(Widget child,
    {bool dark = false, bool arabic = false, bool reducedMotion = false}) {
  return HarborTheme(
    colors: dark ? HarborColors.dark : HarborColors.light,
    text: HarborType(arabic: arabic),
    child: MaterialApp(
      theme: harborThemeData(dark: dark, arabic: arabic),
      builder: (context, app) => MediaQuery(
        data: MediaQuery.of(context).copyWith(disableAnimations: reducedMotion),
        child: app!,
      ),
      home: Scaffold(body: child),
    ),
  );
}

void main() {
  test('breakpoint contract matches authority §20', () {
    expect(HarborBreakpoints.classify(320), HarborWindowClass.compact);
    expect(HarborBreakpoints.classify(599), HarborWindowClass.compact);
    expect(HarborBreakpoints.classify(600), HarborWindowClass.medium);
    expect(HarborBreakpoints.classify(1023), HarborWindowClass.medium);
    expect(HarborBreakpoints.classify(1024), HarborWindowClass.expanded);
    expect(HarborBreakpoints.classify(1179), HarborWindowClass.expanded);
    expect(HarborBreakpoints.classify(1180), HarborWindowClass.wideRail);
    expect(HarborBreakpoints.classify(1280), HarborWindowClass.full);

    // No rail below 1024; collapsed 72 at 1024-1179; 220 from 1180.
    expect(HarborBreakpoints.railWidth(HarborWindowClass.medium), 0);
    expect(HarborBreakpoints.railWidth(HarborWindowClass.expanded), 72);
    expect(HarborBreakpoints.railWidth(HarborWindowClass.wideRail), 220);
    expect(HarborBreakpoints.railWidth(HarborWindowClass.full), 220);
    // Lens persistent only at 1280+.
    expect(
        HarborBreakpoints.lensIsPersistent(HarborWindowClass.wideRail), false);
    expect(HarborBreakpoints.lensIsPersistent(HarborWindowClass.full), true);
    // Work canvas min applies from 1024 only.
    expect(HarborBreakpoints.enforceWorkCanvasMin(HarborWindowClass.medium),
        false);
    expect(HarborBreakpoints.enforceWorkCanvasMin(HarborWindowClass.expanded),
        true);
    // At 1280: 1280 - 220 rail - 320 lens = 740 >= 640 canvas preserved.
    expect(
        HarborBreakpoints.workCanvasWidth(HarborWindowClass.full, 1280), 740);
    // Gutters follow the layout tokens (16 / 24 / 28).
    expect(HarborBreakpoints.gutter(HarborWindowClass.compact), 16);
    expect(HarborBreakpoints.gutter(HarborWindowClass.medium), 24);
    expect(HarborBreakpoints.gutter(HarborWindowClass.full), 28);
  });

  testWidgets('status badge conveys meaning with icon AND label AND color',
      (tester) async {
    await tester.pumpWidget(host(const Center(
        child: StatusBadge(
            semantic: ExecutionSemantic.local, label: 'ON DEVICE'))));
    expect(find.byIcon(Icons.shield_outlined), findsOneWidget);
    expect(find.text('ON DEVICE'), findsOneWidget);
  });

  testWidgets('trust pulse shows policy and execution as separate facts',
      (tester) async {
    await tester.pumpWidget(host(const Center(
      child: TrustPulse(
        policyLabel: 'Policy: LOCAL_ONLY',
        executionLabel: 'Execution: ON DEVICE',
        executionSemantic: ExecutionSemantic.local,
        policyHeading: 'Workspace policy',
        executionHeading: 'Current execution',
      ),
    )));
    // Each fact renders exactly once, under its own heading.
    expect(find.text('Policy: LOCAL_ONLY'), findsOneWidget);
    expect(find.text('Execution: ON DEVICE'), findsOneWidget);
    expect(find.text('Workspace policy'), findsOneWidget);
    expect(find.text('Current execution'), findsOneWidget);
  });

  testWidgets('trust chip is a labeled button', (tester) async {
    var taps = 0;
    await tester.pumpWidget(host(Center(
      child: TrustChip(
        label: 'LOCAL',
        semantic: ExecutionSemantic.local,
        onTap: () => taps++,
        tooltip: 'Open the Harbor Lens',
      ),
    )));
    expect(find.text('LOCAL'), findsOneWidget);
    expect(find.byIcon(Icons.shield_outlined), findsOneWidget);
    await tester.tap(find.text('LOCAL'));
    expect(taps, 1);
  });

  testWidgets('run state badge maps every §11 state to an icon + semantic',
      (tester) async {
    const states = [
      'CREATED',
      'PLANNING',
      'RUNNING',
      'PAUSED',
      'WAITING_APPROVAL',
      'CANCELLING',
      'COMPLETED',
      'FAILED',
      'CANCELLED',
      'OUTCOME_UNKNOWN',
    ];
    final icons = <IconData>{};
    for (final s in states) {
      await tester.pumpWidget(host(Center(child: RunStateBadge(state: s))));
      expect(find.text(s), findsOneWidget);
      icons.add(RunStateBadge.visual(s).icon);
    }
    // Distinct glyphs: uncertainty is never drawn as failure or success.
    expect(icons.length, states.length);
    expect(RunStateBadge.visual('OUTCOME_UNKNOWN').semantic,
        ExecutionSemantic.danger);
    expect(RunStateBadge.visual('COMPLETED').semantic, ExecutionSemantic.local);
    expect(RunStateBadge.visual('PAUSED').semantic, ExecutionSemantic.hybrid);
  });

  testWidgets('fit score renders all five bands with reasons', (tester) async {
    for (final band in FitBand.values) {
      await tester.pumpWidget(host(Center(
          child: FitScoreBadge(band: band, reasons: const ['reason one']))));
      expect(find.text(band.label), findsOneWidget);
      expect(find.textContaining('reason one'), findsOneWidget);
    }
    expect(FitBandLabel.parse('excellent'), FitBand.excellent);
    expect(FitBandLabel.parse('toolarge'), FitBand.tooLarge);
  });

  testWidgets('harbor sheet exposes approve/deny with diff binding',
      (tester) async {
    var approved = false;
    await tester.pumpWidget(host(HarborSheet(
      title: 'Save board deck',
      explanation: 'base v7 → new version; the file must not have changed.',
      diffSummary: ArtifactDiffView(
        baseVersion: 'v7',
        proposedHash: 'a' * 64,
        entries: const [
          DiffEntryVM(
              summary: 'Create board deck', before: null, after: 'slides: 1')
        ],
      ),
      onApprove: () => approved = true,
      onDeny: () {},
    )));
    expect(find.text('base: v7'), findsOneWidget);
    await tester.tap(find.text('Approve'));
    expect(approved, isTrue);
  });

  testWidgets('run trail renders entries and empty state', (tester) async {
    await tester.pumpWidget(host(const RunTrail(entries: [])));
    expect(find.text('No activity yet.'), findsOneWidget);
    await tester.pumpWidget(host(const RunTrail(entries: [
      RunTrailEntry('Recalculated 6 formulas'),
      RunTrailEntry('Safe save failed on conflict',
          failed: true, icon: Icons.error_outline, detail: 'base changed'),
      RunTrailEntry('Await approval', marker: RunTrailMarker.pending),
    ])));
    expect(find.text('Recalculated 6 formulas'), findsOneWidget);
    expect(find.textContaining('conflict'), findsOneWidget);
    expect(find.byIcon(Icons.error_outline), findsOneWidget);
    expect(find.byIcon(Icons.circle_outlined), findsNWidgets(2));
  });

  testWidgets('banner pairs icon with label for every tone', (tester) async {
    for (final tone in HarborBannerTone.values) {
      await tester.pumpWidget(host(HarborBanner(
        tone: tone,
        title: 'Title ${tone.name}',
        body: 'Body text',
      )));
      expect(find.text('Title ${tone.name}'), findsOneWidget);
      expect(find.byType(Icon), findsOneWidget);
    }
  });

  testWidgets('identifiers keep LTR direction inside RTL layouts',
      (tester) async {
    await tester.pumpWidget(host(
      const Directionality(
        textDirection: TextDirection.rtl,
        child: Center(child: HarborIdentifier('run-0badc0ffee')),
      ),
      arabic: true,
    ));
    final text = tester.widget<Text>(find.text('run-0badc0ffee'));
    final context = tester.element(find.text('run-0badc0ffee'));
    expect(Directionality.of(context), TextDirection.ltr);
    expect(text.style!.fontFamily, HarborType.familyMono);
    expect(text.style!.fontFamilyFallback, contains('Menlo'));
    expect(HarborIdentifier.shorten('a' * 64), 'aaaaaaaa…aaaaaa');
  });

  testWidgets('rail renders wordmark, labels and selection semantics',
      (tester) async {
    var selected = 0;
    Widget build(double width) => host(Row(children: [
          HarborRail(
            width: width,
            destinations: const [
              HarborDestination('Home', Icons.home_outlined),
              HarborDestination('Work', Icons.description_outlined),
            ],
            selectedIndex: selected,
            onSelected: (i) => selected = i,
          ),
          const Expanded(child: SizedBox()),
        ]));
    await tester.pumpWidget(build(220));
    expect(find.text('Harbor'), findsOneWidget);
    expect(find.text('Home'), findsOneWidget);
    await tester.tap(find.text('Work'));
    expect(selected, 1);
    // Collapsed rail: icons only, labels move to tooltips.
    await tester.pumpWidget(build(72));
    expect(find.text('Work'), findsNothing);
    expect(find.byTooltip('Work'), findsOneWidget);
    expect(tester.getSize(find.byType(HarborRail)).width, 72);
  });

  testWidgets('motion collapses to zero under reduced motion', (tester) async {
    late HarborMotion normal;
    late HarborMotion reduced;
    await tester.pumpWidget(host(Builder(builder: (context) {
      normal = HarborMotion.of(context);
      return const SizedBox();
    })));
    await tester.pumpWidget(host(Builder(builder: (context) {
      reduced = HarborMotion.of(context);
      return const SizedBox();
    }), reducedMotion: true));
    expect(normal.normal, const Duration(milliseconds: 180));
    expect(normal.panel, const Duration(milliseconds: 320));
    expect(reduced.normal, Duration.zero);
    expect(reduced.panel, Duration.zero);
  });

  testWidgets('op progress renders determinate bar and cancel', (tester) async {
    var cancelled = false;
    await tester.pumpWidget(host(HarborOpProgress(
      title: 'Downloading',
      identifier: 'org/model',
      progressLine: '12 of 40 MiB',
      fraction: 0.3,
      onCancel: () => cancelled = true,
    )));
    expect(find.text('Downloading'), findsOneWidget);
    expect(find.text('org/model'), findsOneWidget);
    await tester.tap(find.text('Cancel'));
    expect(cancelled, isTrue);
  });

  test('arabic typography uses taller line heights than latin', () {
    const ar = HarborType(arabic: true);
    const la = HarborType(arabic: false);
    expect(ar.bodyLine, 24);
    expect(la.bodyLine, 21);
    expect(ar.smallLine, greaterThan(la.smallLine));
    expect(ar.captionLine, greaterThan(la.captionLine));
    // Token weights ride on variable-font axes (650/620/450 exactly).
    expect(la.titleOf(Colors.black).fontVariations,
        [const FontVariation.weight(650)]);
    expect(la.captionOf(Colors.black).fontVariations,
        [const FontVariation.weight(450)]);
    expect(la.monoOf(Colors.black).fontFamilyFallback, contains('monospace'));
  });

  test('theme data is fully derived from tokens for both modes', () {
    for (final dark in [false, true]) {
      final c = dark ? HarborColors.dark : HarborColors.light;
      final theme = harborThemeData(dark: dark, arabic: false);
      expect(theme.colorScheme.primary, c.brand);
      expect(theme.scaffoldBackgroundColor, c.canvas);
      expect(theme.cardTheme.color, c.surface);
      expect(theme.dividerColor, c.border);
      expect(theme.colorScheme.brightness,
          dark ? Brightness.dark : Brightness.light);
    }
  });
}
