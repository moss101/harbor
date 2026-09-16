import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';

/// Localized title for a background-op kind.
String opKindTitle(String? kind, AppLocalizations l10n) => switch (kind) {
      'acquire' => l10n.activityOpKindAcquire,
      'ingest' => l10n.activityOpKindIngest,
      'generate' => l10n.activityOpKindGenerate,
      _ => l10n.lensSectionOps,
    };

IconData opKindIcon(String? kind) => switch (kind) {
      'acquire' => Icons.downloading_outlined,
      'ingest' => Icons.library_add_outlined,
      'generate' => Icons.auto_awesome_outlined,
      _ => Icons.hourglass_top_outlined,
    };

/// Localized phase line for an op snapshot.
String opPhaseLabel(Map<String, dynamic> p, AppLocalizations l10n) {
  final phase = (p['phase'] as String?) ?? '';
  return switch (phase) {
    'resolving' => l10n.opResolving,
    'verifying' => l10n.opVerifying,
    'installing' => l10n.opInstalling,
    'loading' => l10n.opLoadingModel,
    'generating' => l10n.opGenerating,
    'ingesting' || 'indexing' => l10n.opIngesting,
    'downloading' =>
      l10n.opDownloading(_mib(p['bytes_done']), _mib(p['bytes_total'])),
    _ => phase.isEmpty ? '' : phase,
  };
}

int _mib(dynamic v) => ((v as num?)?.toInt() ?? 0) ~/ (1024 * 1024);

/// Progress line + fraction for an op snapshot (bytes first, then items).
({String? line, double? fraction}) opProgressOf(
    Map<String, dynamic> p, AppLocalizations l10n) {
  final bytesDone = (p['bytes_done'] as num?)?.toInt() ?? 0;
  final bytesTotal = (p['bytes_total'] as num?)?.toInt() ?? 0;
  final itemsDone = (p['items_done'] as num?)?.toInt() ?? 0;
  final itemsTotal = (p['items_total'] as num?)?.toInt() ?? 0;
  if (bytesTotal > 0) {
    return (
      line: l10n.opBytesMib(_mib(bytesDone), _mib(bytesTotal)),
      fraction: (bytesDone / bytesTotal).clamp(0, 1).toDouble(),
    );
  }
  if (itemsTotal > 0) {
    return (
      line: l10n.opDownloading(itemsDone, itemsTotal),
      fraction: (itemsDone / itemsTotal).clamp(0, 1).toDouble(),
    );
  }
  if (bytesDone > 0) {
    return (line: l10n.opBytesMib(_mib(bytesDone), 0), fraction: null);
  }
  if (itemsDone > 0) {
    return (line: l10n.askGenerationProgress(itemsDone), fraction: null);
  }
  return (line: null, fraction: null);
}

/// A live background operation with real progress and a real cancel.
class OpProgressCard extends StatelessWidget {
  const OpProgressCard({super.key, required this.progress, this.service});
  final Map<String, dynamic> progress;
  final HarborService? service;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final kind = progress['kind'] as String?;
    final p = opProgressOf(progress, l10n);
    final phase = opPhaseLabel(progress, l10n);
    final detail = (progress['detail'] as String?) ?? '';
    final opId = progress['op_id'] as String?;
    final cancelling = progress['state'] == 'cancelling';
    return HarborOpProgress(
      title: opKindTitle(kind, l10n),
      icon: opKindIcon(kind),
      identifier: detail.isEmpty ? null : detail,
      detail: cancelling ? l10n.cancelAction : phase,
      progressLine: p.line,
      fraction: p.fraction,
      cancelLabel: l10n.cancelAction,
      onCancel: opId == null || service == null || cancelling
          ? null
          : () => service!.cancelOp(opId),
    );
  }
}
