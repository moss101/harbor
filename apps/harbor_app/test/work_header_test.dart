import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/surfaces/work_surface.dart';

import 'fake_service.dart';

/// The Work header on a phone: badge + Open file + close must all be
/// reachable inside the viewport, in English and in Arabic (Arabic labels
/// are longer, which used to push "Open file" past the edge of a
/// horizontally scrolling row).
void main() {
  for (final lang in ['en', 'ar']) {
    testWidgets('phone header actions fit the viewport ($lang)',
        (tester) async {
      final s = FakeHarborService(models: const [
        {'id': 'qwen-chat'}
      ])
        ..fakePreview = docxPreview()
        ..fakePreviewName = 'report.docx'
        ..fakeBytes = [1];
      await pumpSurface(tester, s, (_) => const WorkSurface(),
          size: const Size(390, 844), locale: Locale(lang));
      await tester.pump(const Duration(milliseconds: 300));

      const w = 390.0;
      final button = find.byType(OutlinedButton);
      expect(button, findsWidgets);
      final close = find.byIcon(Icons.close);
      for (final f in [button.first, close.first]) {
        final r = tester.getRect(f);
        expect(r.left, greaterThanOrEqualTo(0), reason: '$lang $f clipped');
        expect(r.right, lessThanOrEqualTo(w), reason: '$lang $f clipped');
      }
      expect(tester.takeException(), isNull);
    });
  }
}
