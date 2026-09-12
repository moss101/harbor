/// Typed views over core JSON. Construction is total (fromMap) and
/// validated at the boundary so surfaces never parse raw maps.

final class SkillSummary {
  const SkillSummary({
    required this.id,
    required this.title,
    required this.family,
    required this.description,
    required this.tools,
  });

  factory SkillSummary.fromMap(Map<String, dynamic> m) => SkillSummary(
        id: m['id'] as String,
        title: m['title'] as String,
        family: m['family'] as String,
        description: m['description'] as String? ?? '',
        tools: (m['tools'] as List?)?.cast<String>() ?? const [],
      );

  final String id, title, family, description;
  final List<String> tools;
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
  const RunSummary({required this.runId, required this.state, required this.activeComputeMs});

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
  const TrailEntry({required this.seq, required this.summary, required this.actor});
  factory TrailEntry.fromMap(Map<String, dynamic> m) => TrailEntry(
        seq: (m['seq'] as num).toInt(),
        summary: m['summary'] as String? ?? '',
        actor: m['actor'] as String? ?? '',
      );
  final int seq;
  final String summary;
  final String actor;
}
