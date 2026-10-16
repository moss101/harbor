import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/services/harbor_service.dart';

/// OCR / speech reads as core ops (`op.start_tools_read`) through the real
/// core. No engine or model is needed: a text attachment is passed through
/// and a recording without a registered speech engine is a typed refusal.
final repoRoot = Directory.current.parent.parent.path;
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('ocr and speech reads run as ops and fail with typed errors',
      (tester) async {
    if (!File(dylibPath).existsSync()) return;
    await tester.runAsync(() async {
      final dir = Directory.systemTemp.createTempSync('harbor-reads-test-');
      final service = await HarborService.open(
        libraryPath: dylibPath,
        dataRoot: dir.path,
        workspaceId: 'ws-reads',
        deviceRootHex:
            'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c',
      );
      addTearDown(() {
        service.close();
        dir.deleteSync(recursive: true);
      });

      final text = await service.ocrDocument(
          utf8.encode('Invoice 1001 4,200'), 'invoice.txt');
      expect(text['source'], 'text_file');
      expect(text['text'], 'Invoice 1001 4,200');

      Object? refused;
      try {
        await service.transcribeAudio([0, 1, 2, 3], 'memo.wav');
      } catch (e) {
        refused = e;
      }
      expect(refused, isNotNull, reason: 'no speech engine in the test core');

      final status = await service.engineStatus();
      expect(status['ocr']!['available'], false);
      expect(status['speech']!['available'], false);
    });
  });
}
