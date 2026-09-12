import 'dart:io';

import 'package:harbor_native/harbor_ffi.dart';
import 'package:test/test.dart';

const dylibPath =
    '/Users/mohsin/projects/harbor/core/target/debug/libharbor_ffi.dylib';

void main() {
  test('boundary round-trips runs and blobs with policy facts', () {
    if (!File(dylibPath).existsSync()) {
      fail('build core first: cargo build -p harbor_ffi');
    }
    final dir = Directory.systemTemp.createTempSync('harbor-ffi-dart-');
    addTearDown(() => dir.deleteSync(recursive: true));
    final client = HarborCoreClient.open(
        dylibPath, dir.path, 'ws-dart-1', HarborPrivacyMode.localOnly);
    addTearDown(client.close);

    // Trust Pulse facts come from the core, not the UI.
    final pulse = client.trustPulse();
    expect(pulse['policy'], 'LOCAL_ONLY');

    // Durable runs across the boundary.
    client.createRun('run-ffi-1');
    final state = client.runState('run-ffi-1');
    expect(state['state'], 'CREATED');
    expect(state['executor_generation'], 0);

    // Private blobs round-trip through the encrypted store.
    final payload = <int>[1, 2, 3, 250, 251];
    final r = client.putBlob(payload);
    expect(r.size, payload.length);
    expect(client.getBlob(r.blobId), payload);

    // Typed errors surface through the envelope.
    expect(() => client.runState('missing-run'),
        throwsA(isA<HarborCoreException>()));

    // DOCX preview through the boundary (real fixture bytes).
    final docx = File(
            '/Users/mohsin/projects/harbor/fixtures/office/structured.docx')
        .readAsBytesSync();
    final preview = client.previewArtifact(docx);
    expect(preview['kind'], 'docx');
    final paras = (preview['preview']['paragraphs'] as List).cast<Map>();
    expect(paras.first['text'], contains('Harbor Plan'));
  });
}
