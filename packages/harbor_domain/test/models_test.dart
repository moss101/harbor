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
    expect(s.runnable, isFalse);
    expect(s.graph, isNull);

    final g = SkillSummary.fromMap({
      'id': 'placeholder-fill',
      'title': 'Placeholder & Form Fill',
      'family': 'Document Intelligence',
      'schema': 'harbor.skill/v2',
      'runnable': true,
      'tools': ['artifact.placeholders'],
      'graph': {
        'id': 'placeholder-fill',
        'version': 1,
        'node_count': 8,
        'model_nodes': 0,
        'tools': ['artifact.placeholders', 'artifact.fill_placeholders'],
        'budgets': {'max_steps': 6, 'max_tool_calls': 2},
        'inputs': {
          'type': 'object',
          'properties': {
            'artifact_id': {'type': 'string'},
            'values': {'type': 'object'},
          },
          'required': ['artifact_id', 'values'],
        },
      },
    });
    expect(g.runnable, isTrue);
    expect(g.graph!.usesModel, isFalse);
    expect(g.graph!.nodeCount, 8);
    expect(g.graph!.inputProperties.map((e) => e.key).toList(),
        ['artifact_id', 'values']);
    expect(g.graph!.requiredInputs, ['artifact_id', 'values']);

    final m = ModelInfo.fromMap(
        {'id': 'm1', 'files': 2, 'total_bytes': 1536 * 1024 * 1024});
    expect(m.sizeLabel, '1.5 GB');

    final r = RunSummary.fromMap(
        {'run_id': 'r1', 'state': 'RUNNING', 'active_compute_ms_total': 42});
    expect(r.activeComputeMs, 42);

    final fit = FitBandVM.fromCoreBand('toolarge', []);
    expect(fit.label, 'Too large');
  });
}
