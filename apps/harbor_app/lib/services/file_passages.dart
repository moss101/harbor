/// Passages of ONE open file, read from the core's preview IR, for grounded
/// questions about that file (`op.start_generate` with a `document`).
///
/// Every passage carries an id the UI can map back to a place in the Work
/// Canvas (a citation is tapped → the view scrolls there) and a short,
/// language-neutral locator the model can quote:
///
/// - DOCX: one passage per non-empty paragraph — `para-<index>`, `¶<index>`;
/// - PDF: one per page (long pages split) — `page-<n>`, `page <n>`;
/// - PPTX: one per slide, title + bullets — `slide-<n>`, `slide <n>`;
/// - XLSX: one per non-empty cell of the shown sheet — `cell-<A1>`,
///   `<Sheet>!<A1>`.
///
/// Pure (no core, no Flutter), so the shapes are unit-tested directly.
library;

import 'dart:convert' show utf8;

/// One quotable span of the open file.
final class FilePassage {
  const FilePassage(
      {required this.id, required this.text, required this.locator});
  final String id;
  final String text;
  final String locator;

  Map<String, dynamic> toWire() => {'id': id, 'text': text, 'locator': locator};
}

/// Where a passage id points in the Work Canvas.
enum PassageKind { paragraph, page, slide, cell }

/// A parsed passage id: what kind of place, and which one. [index] is the
/// paragraph index / page number / slide number; [cell] the A1 reference.
final class PassageTarget {
  const PassageTarget(this.kind, {this.index = 0, this.cell});
  final PassageKind kind;
  final int index;
  final String? cell;

  /// `para-12`, `page-3`, `page-3~2` (a split page), `slide-2`, `cell-B4`.
  /// Null for anything else (a citation from the Knowledge index).
  static PassageTarget? parse(String? id) {
    if (id == null) return null;
    final base = id.split('~').first;
    final dash = base.indexOf('-');
    if (dash <= 0) return null;
    final prefix = base.substring(0, dash);
    final rest = base.substring(dash + 1);
    if (prefix == 'cell') {
      return rest.isEmpty ? null : PassageTarget(PassageKind.cell, cell: rest);
    }
    final n = int.tryParse(rest);
    if (n == null) return null;
    return switch (prefix) {
      'para' => PassageTarget(PassageKind.paragraph, index: n),
      'page' => PassageTarget(PassageKind.page, index: n),
      'slide' => PassageTarget(PassageKind.slide, index: n),
      _ => null,
    };
  }
}

/// The open file as the core's `document` argument.
final class FileDocument {
  const FileDocument({required this.name, required this.passages});

  /// Display name (the file name), or the kind when the file has none.
  final String name;
  final List<FilePassage> passages;

  /// Upper bounds that keep one request bounded: a workbook with tens of
  /// thousands of cells is cut at [maxPassages] (the first cells in row
  /// order) and a long page is split into [maxPassageChars] pieces.
  /// The total is also kept under [maxTotalBytes] of UTF-8, below the
  /// core's own 4 MiB ceiling for a document.
  static const int maxPassages = 1500;
  static const int maxPassageChars = 1500;
  static const int maxTotalBytes = 3 * 1024 * 1024;

  Map<String, dynamic> toWire() => {
        'name': name,
        'passages': [for (final p in passages) p.toWire()],
      };

  /// Passages from an `artifact.preview` result (`{kind, preview}`); null
  /// when there is no preview.
  static FileDocument? fromPreview(String? name, Map<String, dynamic>? result) {
    if (result == null) return null;
    final kind = result['kind'] as String?;
    final data = result['preview'];
    final map = data is Map ? data : const {};
    final out = <FilePassage>[];
    var bytes = 0;
    void add(String id, String text, String locator) {
      if (out.length >= maxPassages) return;
      final clean = text.trim();
      if (clean.isEmpty) return;
      bool fits(String text) {
        final size = utf8.encode(text).length;
        if (bytes + size > maxTotalBytes) return false;
        bytes += size;
        return true;
      }

      if (clean.length <= maxPassageChars) {
        if (fits(clean)) {
          out.add(FilePassage(id: id, text: clean, locator: locator));
        }
        return;
      }
      var part = 1;
      for (var i = 0; i < clean.length; i += maxPassageChars) {
        if (out.length >= maxPassages) return;
        final end = (i + maxPassageChars).clamp(0, clean.length);
        final piece = clean.substring(i, end);
        if (!fits(piece)) return;
        out.add(FilePassage(
            id: part == 1 ? id : '$id~$part', text: piece, locator: locator));
        part++;
      }
    }

    switch (kind) {
      case 'docx':
        for (final p in (map['paragraphs'] as List? ?? const []).cast<Map>()) {
          final index = (p['index'] as num?)?.toInt();
          if (index == null) continue;
          add('para-$index', p['text'] as String? ?? '', '¶$index');
        }
      case 'pdf':
        for (final p in (map['pages'] as List? ?? const []).cast<Map>()) {
          final index = (p['index'] as num?)?.toInt();
          if (index == null) continue;
          add('page-$index', p['text'] as String? ?? '', 'page $index');
        }
      case 'workbook':
        final sheet = map['sheet'] as String? ?? '';
        final cells = (map['cells'] as List? ?? const []).cast<Map>().toList()
          ..sort((a, b) {
            final r =
                ((a['row'] as num?) ?? 0).compareTo((b['row'] as num?) ?? 0);
            return r != 0
                ? r
                : ((a['col'] as num?) ?? 0).compareTo((b['col'] as num?) ?? 0);
          });
        for (final c in cells) {
          final row = (c['row'] as num?)?.toInt();
          final col = (c['col'] as num?)?.toInt();
          if (row == null || col == null || row < 1 || col < 1) continue;
          final value = c['value']?.toString() ?? '';
          final formula = c['formula'] as String?;
          final ref = a1(row, col);
          final text = formula == null || formula.isEmpty
              ? value
              : (value.isEmpty ? '=$formula' : '$value (=$formula)');
          add('cell-$ref', text, sheet.isEmpty ? ref : '$sheet!$ref');
        }
      default: // deck
        for (final s in (map['slides'] as List? ?? const []).cast<Map>()) {
          final index = (s['index'] as num?)?.toInt();
          if (index == null) continue;
          final lines = [
            s['title'] as String? ?? '',
            for (final b in (s['bullets'] as List? ?? const [])) '$b',
          ].where((l) => l.trim().isNotEmpty);
          add('slide-$index', lines.join('\n'), 'slide $index');
        }
    }
    return FileDocument(
        name: (name == null || name.isEmpty) ? (kind ?? 'file') : name,
        passages: out);
  }

  /// 1-based (row, col) → `B4`.
  static String a1(int row, int col) {
    var c = col;
    var letters = '';
    while (c > 0) {
      final rem = (c - 1) % 26;
      letters = String.fromCharCode(65 + rem) + letters;
      c = (c - 1) ~/ 26;
    }
    return '$letters$row';
  }

  /// `B4` → (4, 2); null when [ref] is not an A1 reference.
  static (int, int)? parseA1(String ref) {
    final m = RegExp(r'^([A-Za-z]+)(\d+)$').firstMatch(ref.split('!').last);
    if (m == null) return null;
    var col = 0;
    for (final ch in m.group(1)!.toUpperCase().codeUnits) {
      col = col * 26 + (ch - 64);
    }
    return (int.parse(m.group(2)!), col);
  }
}
