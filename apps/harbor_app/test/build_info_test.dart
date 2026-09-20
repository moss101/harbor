import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/build_info.dart';

void main() {
  test('harborAppVersion matches pubspec.yaml', () {
    final pubspec = File('pubspec.yaml').readAsStringSync();
    final line = pubspec
        .split('\n')
        .firstWhere((l) => l.startsWith('version:'), orElse: () => '');
    expect(line.substring('version:'.length).trim(), harborAppVersion);
  });
}
