import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// A fresh temp data root for a real-core test, deleted when the test ends.
///
/// Every `HarborService.open` in a test writes a whole data root (database,
/// indexes, model copies); left behind, these filled `$TMPDIR` by gigabytes.
///
/// Call it before registering the service's `close` tear-down: tear-downs run
/// last-in-first-out, so the core closes before its files are removed. It is
/// synchronous, so it works inside or outside `tester.runAsync`. Not for
/// `setUpAll` paired with a `tearDownAll` close: that tear-down is declared
/// first, so it would run after the delete — delete in the `tearDownAll`.
Directory tempDataRoot(String prefix) {
  final dir = Directory.systemTemp.createTempSync(prefix);
  addTearDown(() {
    try {
      dir.deleteSync(recursive: true);
    } on FileSystemException {
      // Best effort: a test that already removed it, or a file still held.
    }
  });
  return dir;
}
