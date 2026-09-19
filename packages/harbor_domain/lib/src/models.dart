library;

// Typed views over core JSON. Construction is total (fromMap) and
// validated at the boundary so surfaces never parse raw maps.

final class SkillSummary {
  const SkillSummary({
    required this.id,
    required this.title,
    required this.family,
    required this.description,
    required this.tools,
    this.schema = 'harbor.skill/v1',
    this.runnable = false,
    this.graph,
    this.requires = const [],
  });

  factory SkillSummary.fromMap(Map<String, dynamic> m) => SkillSummary(
        id: m['id'] as String,
        title: m['title'] as String,
        family: m['family'] as String,
        description: m['description'] as String? ?? '',
        tools: (m['tools'] as List?)?.cast<String>() ?? const [],
        schema: m['schema'] as String? ?? 'harbor.skill/v1',
        runnable: m['runnable'] as bool? ?? false,
        graph: m['graph'] is Map
            ? SkillGraphInfo.fromMap(
                (m['graph'] as Map).cast<String, dynamic>())
            : null,
        requires: (m['requires'] as List?)?.cast<String>() ?? const [],
      );

  final String id, title, family, description, schema;
  final List<String> tools, requires;

  /// True when the core carries an executable graph for this skill
  /// (decision 0006); prose-only skills are declarations.
  final bool runnable;
  final SkillGraphInfo? graph;
}

/// Graph facts the core reports for a runnable skill: enough for the UI to
/// build an input form and say honestly whether the model is involved.
final class SkillGraphInfo {
  const SkillGraphInfo({
    required this.id,
    required this.version,
    required this.nodeCount,
    required this.modelNodes,
    required this.tools,
    required this.inputs,
    required this.maxSteps,
    required this.maxToolCalls,
  });

  factory SkillGraphInfo.fromMap(Map<String, dynamic> m) => SkillGraphInfo(
        id: m['id'] as String,
        version: (m['version'] as num?)?.toInt() ?? 1,
        nodeCount: (m['node_count'] as num?)?.toInt() ?? 0,
        modelNodes: (m['model_nodes'] as num?)?.toInt() ?? 0,
        tools: (m['tools'] as List?)?.cast<String>() ?? const [],
        inputs: (m['inputs'] as Map?)?.cast<String, dynamic>() ?? const {},
        maxSteps: (m['budgets']?['max_steps'] as num?)?.toInt() ?? 0,
        maxToolCalls: (m['budgets']?['max_tool_calls'] as num?)?.toInt() ?? 0,
      );

  final String id;
  final int version, nodeCount, modelNodes, maxSteps, maxToolCalls;
  final List<String> tools;

  /// JSON Schema of the run inputs (bound at /input).
  final Map<String, dynamic> inputs;

  bool get usesModel => modelNodes > 0;

  /// Input properties in declaration order with their schemas.
  List<MapEntry<String, Map<String, dynamic>>> get inputProperties {
    final props = (inputs['properties'] as Map?)?.cast<String, dynamic>();
    if (props == null) return const [];
    return [
      for (final e in props.entries)
        MapEntry(e.key, (e.value as Map?)?.cast<String, dynamic>() ?? const {}),
    ];
  }

  List<String> get requiredInputs =>
      (inputs['required'] as List?)?.cast<String>() ?? const [];
}

final class ModelInfo {
  const ModelInfo({
    required this.id,
    required this.fileCount,
    required this.totalBytes,
    required this.runtime,
  });

  factory ModelInfo.fromMap(Map<String, dynamic> m) => ModelInfo(
        id: m['id'] as String,
        fileCount: m['files'] as int,
        totalBytes: m['total_bytes'] as int,
        runtime: m['runtime'] as String? ?? 'gguf/llama.cpp',
      );

  final String id;
  final int fileCount, totalBytes;
  final String runtime;

  String get sizeLabel {
    final mb = totalBytes ~/ (1024 * 1024);
    if (mb >= 1024) return '${(mb / 1024).toStringAsFixed(1)} GB';
    return '$mb MB';
  }
}

final class RunSummary {
  const RunSummary(
      {required this.runId,
      required this.state,
      required this.activeComputeMs});

  factory RunSummary.fromMap(Map<String, dynamic> m) => RunSummary(
        runId: m['run_id'] as String,
        state: m['state'] as String,
        activeComputeMs: m['active_compute_ms_total'] as int,
      );

  final String runId;
  final String state;
  final int activeComputeMs;
}

final class FitBandVM {
  const FitBandVM._(this.label, this.severity);
  final String label;
  final int severity; // 0 best .. 4 worst

  static FitBandVM fromCoreBand(String band, List<String> reasons) {
    switch (band) {
      case 'excellent':
        return FitBandVM._('Excellent', 0);
      case 'good':
        return FitBandVM._('Good', 0);
      case 'limited':
        return FitBandVM._('Limited', 1);
      case 'unsupported':
        return FitBandVM._('Unsupported', 4);
      case 'toolarge':
      case 'too_large':
        return FitBandVM._('Too large', 4);
      default:
        return FitBandVM._(band, 4);
    }
  }
}

final class TrailEntry {
  const TrailEntry(
      {required this.seq, required this.summary, required this.actor});
  factory TrailEntry.fromMap(Map<String, dynamic> m) => TrailEntry(
        seq: (m['seq'] as num).toInt(),
        summary: m['summary'] as String? ?? '',
        actor: m['actor'] as String? ?? '',
      );
  final int seq;
  final String summary;
  final String actor;
}
