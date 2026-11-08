/// One installable choice from a Hugging Face GGUF repo: a single
/// quantization's weights (all parts of a split file), optionally with the
/// repo's vision projector. A repo usually ships many quantizations of the
/// same model; installing all of them is tens of GB and never what the
/// person meant.
class GgufVariant {
  const GgufVariant({
    required this.label,
    required this.weights,
    required this.projector,
    required this.totalBytes,
  });

  /// Display name, e.g. `Q4_K_M` (or the file name when no quant tag).
  final String label;

  /// Weight file paths, in part order.
  final List<String> weights;

  /// Vision projector (`mmproj`) path, when the repo has one.
  final String? projector;

  /// Weights plus projector, in bytes.
  final int totalBytes;

  /// The `files` argument for preflight / acquire.
  List<Map<String, String>> get acquireFiles => [
        for (final w in weights) {'path': w, 'role': 'weights', 'sha256': ''},
        if (projector != null)
          {'path': projector!, 'role': 'mmproj', 'sha256': ''},
      ];
}

final _splitPart =
    RegExp(r'-(\d{5})-of-\d{5}(?=\.gguf$)', caseSensitive: false);
final _quantTag = RegExp(
    r'(?:^|[-_.])((?:I?Q\d[A-Z0-9_]*)|BF16|F16|F32)(?=\.gguf$)',
    caseSensitive: false);

/// The quantization picked when the person doesn't choose: the usual
/// size/quality balance for phones and laptops.
const preferredQuant = 'Q4_K_M';

/// Group a repo's `.gguf` files ([{path, size}], as `models.hf_files`
/// returns them) into installable variants, smallest first. Split files
/// (`-00001-of-00003`) are one variant; `mmproj` files are not variants but
/// ride along with each one (the smallest projector).
List<GgufVariant> ggufVariants(List<Map<String, dynamic>> files) {
  int size(Map<String, dynamic> f) => (f['size'] as num? ?? 0).toInt();
  final projectors = [
    for (final f in files)
      if ((f['path'] as String).toLowerCase().contains('mmproj')) f
  ]..sort((a, b) => size(a).compareTo(size(b)));
  final projector = projectors.isEmpty ? null : projectors.first;

  final groups = <String, List<Map<String, dynamic>>>{};
  for (final f in files) {
    final path = f['path'] as String;
    if (!path.toLowerCase().endsWith('.gguf') ||
        path.toLowerCase().contains('mmproj')) {
      continue;
    }
    groups.putIfAbsent(path.replaceFirst(_splitPart, ''), () => []).add(f);
  }

  String fileLabel(String key) => key
      .split('/')
      .last
      .replaceFirst(RegExp(r'\.gguf$', caseSensitive: false), '');
  String quantLabel(String key) =>
      _quantTag.firstMatch(key)?.group(1)?.toUpperCase() ?? fileLabel(key);
  // Two files with the same quant tag (`X-Q4_0` and `X-QAD-Q4_0`) are
  // told apart by their file names.
  final tagCounts = <String, int>{};
  for (final key in groups.keys) {
    tagCounts.update(quantLabel(key), (n) => n + 1, ifAbsent: () => 1);
  }
  final variants = [
    for (final MapEntry(key: key, value: parts) in groups.entries)
      GgufVariant(
        label:
            tagCounts[quantLabel(key)]! > 1 ? fileLabel(key) : quantLabel(key),
        weights: [for (final p in parts) p['path'] as String]..sort(),
        projector: projector?['path'] as String?,
        totalBytes: parts.fold<int>(0, (n, p) => n + size(p)) +
            (projector == null ? 0 : size(projector)),
      ),
  ]..sort((a, b) => a.totalBytes.compareTo(b.totalBytes));
  return variants;
}

/// Index of the variant to preselect: [preferredQuant] when present, else
/// the smallest.
int preferredVariant(List<GgufVariant> variants) {
  final i = variants.indexWhere((v) => v.label == preferredQuant);
  return i < 0 ? 0 : i;
}
