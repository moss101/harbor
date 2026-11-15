import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/harbor_ui.dart';

Widget _host(Widget child, {bool reduced = false, bool dark = false}) =>
    MaterialApp(
      home: MediaQuery(
        data: MediaQueryData(disableAnimations: reduced),
        child: HarborTheme(
          colors: dark ? HarborColors.dark : HarborColors.light,
          text: const HarborType(arabic: false),
          child: Scaffold(body: child),
        ),
      ),
    );

void main() {
  testWidgets('lines: the requested count, the last one shorter',
      (tester) async {
    await tester.pumpWidget(_host(HarborSkeleton.lines(3)));
    final blocks = find.byType(HarborSkeletonBlock);
    expect(blocks, findsNWidgets(3));
    final first = tester.getSize(blocks.first).width;
    final last = tester.getSize(blocks.last).width;
    expect(last, lessThan(first));
  });

  testWidgets('decorative: nothing reaches a screen reader', (tester) async {
    final handle = tester.ensureSemantics();
    await tester.pumpWidget(_host(Column(children: [
      const Text('Writing'),
      HarborSkeleton.lines(2),
    ])));
    expect(find.bySemanticsLabel('Writing'), findsOneWidget);
    expect(find.byType(ExcludeSemantics), findsWidgets);
    handle.dispose();
  });

  testWidgets('reduced motion holds still (the tree settles)', (tester) async {
    await tester.pumpWidget(_host(HarborSkeleton.lines(2), reduced: true));
    await tester.pumpAndSettle();
    expect(find.byType(HarborSkeletonBlock), findsNWidgets(2));
  });

  testWidgets('the sheen animates otherwise, in either theme', (tester) async {
    await tester.pumpWidget(_host(HarborSkeleton.lines(2), dark: true));
    await tester.pump(const Duration(milliseconds: 300));
    expect(tester.binding.hasScheduledFrame, isTrue);
  });
}
