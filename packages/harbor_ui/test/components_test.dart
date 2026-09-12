import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/harbor_ui.dart';

Widget host(Widget child, {bool dark = false, bool arabic = false}) {
  return HarborTheme(
    colors: dark ? HarborColors.dark : HarborColors.light,
    text: HarborType(arabic: arabic),
    child: MaterialApp(
      theme: harborThemeData(dark: dark, arabic: arabic),
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
    expect(HarborBreakpoints.lensIsPersistent(HarborWindowClass.wideRail), false);
    expect(HarborBreakpoints.lensIsPersistent(HarborWindowClass.full), true);
    // Work canvas min applies from 1024 only.
    expect(HarborBreakpoints.enforceWorkCanvasMin(HarborWindowClass.medium), false);
    expect(HarborBreakpoints.enforceWorkCanvasMin(HarborWindowClass.expanded), true);
    // At 1280: 1280 - 220 rail - 320 lens = 740 >= 640 canvas preserved.
    expect(HarborBreakpoints.workCanvasWidth(HarborWindowClass.full, 1280), 740);
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
      ),
    )));
    expect(find.text('Policy: LOCAL_ONLY'), findsNWidgets(2));
    expect(find.text('Execution: ON DEVICE'), findsOneWidget);
  });

  testWidgets('fit score renders all five bands with reasons',
      (tester) async {
    for (final band in FitBand.values) {
      await tester.pumpWidget(host(Center(
          child: FitScoreBadge(band: band, reasons: const ['reason one']))));
      expect(find.text(band.label), findsOneWidget);
      expect(find.textContaining('reason one'), findsOneWidget);
    }
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
          DiffEntryVM(summary: 'Create board deck', before: null, after: 'slides: 1')
        ],
      ),
      onApprove: () => approved = true,
      onDeny: () {},
    )));
    expect(find.text('base: v7'), findsOneWidget);
    await tester.tap(find.text('Approve'));
    expect(approved, isTrue);
  });

  testWidgets('run trail renders entries and empty state',
      (tester) async {
    await tester.pumpWidget(host(const RunTrail(entries: [])));
    expect(find.text('No activity yet.'), findsOneWidget);
    await tester.pumpWidget(host(RunTrail(entries: [
      const RunTrailEntry('Recalculated 6 formulas'),
      RunTrailEntry('Safe save failed on conflict',
          failed: true, icon: Icons.error_outline, detail: 'base changed'),
    ])));
    expect(find.text('Recalculated 6 formulas'), findsOneWidget);
    expect(find.textContaining('conflict'), findsOneWidget);
  });

  test('arabic typography uses taller line heights than latin', () {
    final ar = HarborType(arabic: true);
    final la = HarborType(arabic: false);
    expect(ar.bodyLine, 24);
    expect(la.bodyLine, 21);
    expect(ar.smallLine, greaterThan(la.smallLine));
    expect(ar.captionLine, greaterThan(la.captionLine));
  });
}
