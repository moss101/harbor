import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// EN/AR string-coverage gate: scans the app's UI sources for hardcoded
/// Latin prose in user-facing string slots. Everything user-visible must
/// come from the arb files (or be data from the core / on the allowlist).
void main() {
  test('EN and AR arb files have identical key sets', () {
    final en = _keys('lib/l10n/app_en.arb');
    final ar = _keys('lib/l10n/app_ar.arb');
    expect(
      en.difference(ar),
      isEmpty,
      reason: 'keys missing from app_ar.arb: '
          '${en.difference(ar).toList()}',
    );
    expect(
      ar.difference(en),
      isEmpty,
      reason: 'keys missing from app_en.arb: '
          '${ar.difference(en).toList()}',
    );
  });

  test('no hardcoded user-facing prose outside l10n', () {
    final dirs = ['lib/surfaces', 'lib/shell'];
    final violations = <String>[];
    // Slots that render user-visible text.
    final slotRe = RegExp(
        r"(Text|title|body|message|actionLabel|label|tooltip|hintText)\s*[:\(]\s*(?:const\s+)?'([^']*)'",
        multiLine: true);
    // >= 2 consecutive latin words = prose (not a token/technical string).
    final proseRe = RegExp(r"[A-Za-z]+[A-Za-z ,]'?[A-Za-z ,]{6,}");
    // Strings that are policy tokens or technical renders, not prose.
    final allow = [
      'LOCAL ONLY',
      'LOCAL',
      'OFFLINE',
      'Approve',
      'Deny',
      'Retry',
      'Search',
    ];
    for (final dir in dirs) {
      final d = Directory(dir);
      if (!d.existsSync()) continue;
      for (final f in d.listSync(recursive: true)) {
        if (f is! File || !f.path.endsWith('.dart')) continue;
        final lines = f.readAsLinesSync();
        for (var i = 0; i < lines.length; i++) {
          final line = lines[i];
          // l10n-sourced and data-driven lines are fine.
          if (line.contains('l10n.') ||
              line.contains('c[\'') ||
              line.contains('m[\'') ||
              line.contains('r[\'') ||
              line.contains('s.title') ||
              line.contains('d.label')) {
            continue;
          }
          for (final m in slotRe.allMatches(line)) {
            final value = m.group(2)!;
            if (value.trim().isEmpty) continue;
            if (allow.contains(value.trim())) continue;
            if (proseRe.hasMatch(value)) {
              violations.add('${f.path}:${i + 1}: "$value"');
            }
          }
        }
      }
    }
    expect(
      violations,
      isEmpty,
      reason: 'Hardcoded user-facing strings found — move them to the arb '
          'files (lib/l10n/app_en.arb, app_ar.arb):\n${violations.join("\n")}',
    );
  });
}

Set<String> _keys(String path) {
  final doc = jsonDecode(File(path).readAsStringSync()) as Map<String, dynamic>;
  return doc.keys.where((k) => !k.startsWith('@')).toSet();
}
