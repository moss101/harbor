import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/harbor_ui.dart';

/// UX-016 / ACC-035 contract: the Run Trail shows user-readable status;
/// the technical line is COLLAPSED by default and expands on demand.
void main() {
  testWidgets('run trail technical detail is collapsed then expandable',
      (tester) async {
    await tester.pumpWidget(HarborTheme(
      colors: HarborColors.light,
      text: HarborType(arabic: false),
      child: const MaterialApp(
        home: Scaffold(
          body: RunTrail(entries: [
            RunTrailEntry(
              'Summarize the quarterly report',
              detail: 'user',
              technical: 'seq 1 · request_logged',
            ),
          ]),
        ),
      ),
    ));
    // User-readable summary visible…
    expect(find.text('Summarize the quarterly report'), findsOneWidget);
    // …technical line collapsed by default.
    expect(find.text('seq 1 · request_logged'), findsNothing);
    // Expand on demand.
    await tester.tap(find.byIcon(Icons.expand_more));
    await tester.pump();
    expect(find.text('seq 1 · request_logged'), findsOneWidget);
    // And collapse again.
    await tester.tap(find.byIcon(Icons.expand_less));
    await tester.pump();
    expect(find.text('seq 1 · request_logged'), findsNothing);
  });
}
