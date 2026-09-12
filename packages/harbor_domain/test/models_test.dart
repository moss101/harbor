import 'package:harbor_domain/harbor_domain.dart';
import 'package:test/test.dart';

void main() {
  test('models parse core JSON maps', () {
    final s = SkillSummary.fromMap({
      'id': 'doc-intelligence',
      'title': 'Document Intelligence',
      'family': 'Document Intelligence',
      'description': 'd',
      'tools': ['artifact.read'],
    });
    expect(s.id, 'doc-intelligence');
    expect(s.tools, ['artifact.read']);

    final m = ModelInfo.fromMap({'id': 'm1', 'files': 2, 'total_bytes': 1536 * 1024 * 1024});
    expect(m.sizeLabel, '1.5 GB');

    final r = RunSummary.fromMap({'run_id': 'r1', 'state': 'RUNNING', 'active_compute_ms_total': 42});
    expect(r.activeComputeMs, 42);

    final fit = FitBandVM.fromCoreBand('toolarge', []);
    expect(fit.label, 'Too large');
  });
}
