import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/harbor_ui.dart';

Widget _host(Widget child) => HarborTheme(
      colors: HarborColors.light,
      text: const HarborType(arabic: false),
      child: MaterialApp(
        theme: harborThemeData(dark: false, arabic: false),
        home: Scaffold(body: child),
      ),
    );

void main() {
  testWidgets('on a phone every header action stays on screen, none is hidden',
      (tester) async {
    tester.view.physicalSize = const Size(360, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(_host(HarborSurfaceHeader(
      title: 'Untitled document',
      subtitle: 'Document · 1 paragraph',
      showTitleOnCompact: true,
      actions: [
        const StatusBadge(
            semantic: ExecutionSemantic.hybrid, label: 'Read-only preview'),
        for (final k in ['save', 'pdf', 'print', 'open', 'close'])
          IconButton(
              key: ValueKey(k), onPressed: () {}, icon: const Icon(Icons.save)),
      ],
    )));
    await tester.pump();
    expect(tester.takeException(), isNull);
    for (final k in ['save', 'pdf', 'print', 'open', 'close']) {
      final r = tester.getRect(find.byKey(ValueKey(k)));
      expect(r.left, greaterThanOrEqualTo(0), reason: k);
      expect(r.right, lessThanOrEqualTo(360), reason: '$k is clipped');
    }
  });
}
