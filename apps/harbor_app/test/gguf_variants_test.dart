import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/services/gguf_variants.dart';

void main() {
  test('a repo of many quantizations is many choices, not one 88 GB install',
      () {
    final variants = ggufVariants([
      {'path': 'LFM2.5-1.2B-Instruct-Q8_0.gguf', 'size': 1250},
      {'path': 'LFM2.5-1.2B-Instruct-Q4_K_M.gguf', 'size': 730},
      {'path': 'LFM2.5-1.2B-Instruct-F16.gguf', 'size': 2340},
    ]);
    expect([for (final v in variants) v.label], ['Q4_K_M', 'Q8_0', 'F16']);
    expect(variants[preferredVariant(variants)].acquireFiles, [
      {
        'path': 'LFM2.5-1.2B-Instruct-Q4_K_M.gguf',
        'role': 'weights',
        'sha256': ''
      },
    ]);
  });

  test('split parts are one variant; the smallest projector rides along', () {
    final variants = ggufVariants([
      {'path': 'm-Q4_K_M-00002-of-00002.gguf', 'size': 40},
      {'path': 'm-Q4_K_M-00001-of-00002.gguf', 'size': 60},
      {'path': 'mmproj-m-f16.gguf', 'size': 9},
      {'path': 'mmproj-m-Q8_0.gguf', 'size': 5},
      {'path': 'README.md', 'size': 1},
    ]);
    expect(variants, hasLength(1));
    final v = variants.single;
    expect(v.label, 'Q4_K_M');
    expect(v.weights,
        ['m-Q4_K_M-00001-of-00002.gguf', 'm-Q4_K_M-00002-of-00002.gguf']);
    expect(v.projector, 'mmproj-m-Q8_0.gguf');
    expect(v.totalBytes, 105);
    expect(v.acquireFiles.last['role'], 'mmproj');
  });

  test('no GGUF weights means nothing to offer', () {
    expect(
        ggufVariants([
          {'path': 'model.safetensors', 'size': 100},
        ]),
        isEmpty);
    expect(
        preferredVariant(ggufVariants([
          {'path': 'tiny-IQ3_M.gguf', 'size': 3},
          {'path': 'tiny-Q5_K_M.gguf', 'size': 5},
        ])),
        0);
  });

  test('two files with one quant tag are labelled by file name', () {
    final labels = [
      for (final v in ggufVariants([
        {'path': 'LFM-Q4_0.gguf', 'size': 6},
        {'path': 'LFM-QAD-Q4_0.gguf', 'size': 7},
        {'path': 'LFM-Q4_K_M.gguf', 'size': 8},
      ]))
        v.label
    ];
    expect(labels, ['LFM-Q4_0', 'LFM-QAD-Q4_0', 'Q4_K_M']);
  });
}
