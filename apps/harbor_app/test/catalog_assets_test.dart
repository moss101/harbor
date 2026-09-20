import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// The bundled catalog is the release trust anchor: it must be the exact
/// signed document and root key committed under fixtures/catalog (the
/// core's `committed_signed_catalog_verifies_against_committed_root_key`
/// test pins those two files to each other).
void main() {
  test('bundled catalog assets are byte-identical to fixtures/catalog', () {
    final repo = Directory.current.parent.parent.path; // apps/harbor_app
    for (final name in ['signed_catalog.json', 'root_public.hex']) {
      final asset = File('assets/catalog/$name').readAsBytesSync();
      final fixture = File('$repo/fixtures/catalog/$name').readAsBytesSync();
      expect(asset, equals(fixture), reason: name);
    }
  });
}
