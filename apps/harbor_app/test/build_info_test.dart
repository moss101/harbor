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

  // Keeping build_info in sync with pubspec is useless if the UI does not
  // read it. Settings -> About rendered a hardcoded '1.0.0' on a 1.1.0+2
  // build, and disagreed with the diagnostics export in the same file,
  // which did use harborAppVersion. A beta tester reporting a problem
  // would have quoted the wrong build.
  test('the Settings About card reads the version, never a literal', () {
    final src = File('lib/surfaces/settings_surface.dart').readAsStringSync();
    expect(src.contains('value: harborAppVersion'), isTrue,
        reason: 'the About card must render build_info.harborAppVersion');
    final literal = RegExp(r"value: '\d+\.\d+\.\d+");
    final hit = literal.firstMatch(src)?.group(0);
    expect(hit, isNull,
        reason: 'hardcoded version literal in settings_surface.dart: $hit');
  });
}
