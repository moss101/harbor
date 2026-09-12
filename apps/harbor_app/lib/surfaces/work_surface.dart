import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';
import '../shell/adaptive_shell.dart';

/// Work Canvas (goal §22): the artifact workspace. The user must always be
/// able to determine which file, which version, proposed vs committed,
/// verified values, fidelity limits, conflicts and approvals.
class WorkSurface extends StatelessWidget {
  const WorkSurface({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final width = MediaQuery.sizeOf(context).width;
    final canvasMinApplies = HarborBreakpoints
        .enforceWorkCanvasMin(HarborBreakpoints.classify(width));
    return Center(
      child: Column(children: [
        Padding(
          padding: const EdgeInsets.all(HarborSpace.s3),
          child: Row(children: [
            TextButton.icon(
              onPressed: () => openHarborLens(context),
              icon: const Icon(Icons.insights_outlined, size: 16),
              label: Text(l10n.lensButton),
            ),
            const Spacer(),
            Flexible(
              child: Text(
                canvasMinApplies
                    ? l10n.workCanvasRuleActive(
                        HarborLayout.workCanvasMin.toInt())
                    : l10n.canvasViewportEditing,
                textAlign: TextAlign.end,
                style: t.text.captionOf(t.colors.inkMuted),
              ),
            ),
          ]),
        ),
        Expanded(
          child: Builder(builder: (context) {
            final sp = HarborServiceProvider.of(context);
            final service = sp.notifier;
            final preview = service?.preview;
            if (preview == null) {
              return HarborEmptyState(
                title: l10n.workEmptyTitle,
                body: l10n.workEmptyBody,
                actionLabel: l10n.openFile,
              );
            }
            if (preview['kind'] == 'docx') {
              final data = preview['preview'] as Map;
              final paras = (data['paragraphs'] as List).cast<Map>();
              return ListView(
                padding: const EdgeInsets.all(HarborSpace.s4),
                children: [
                  for (final p in paras)
                    Padding(
                      padding: const EdgeInsets.only(bottom: HarborSpace.s2),
                      child: Text(
                        p['style'] != null
                            ? '[${p['style']}] ${p['text']}'
                            : '${p['text']}',
                        style: t.text.smallOf(t.colors.ink),
                      ),
                    ),
                ],
              );
            }
            if (preview['kind'] == 'pdf') {
              final data = preview['preview'] as Map;
              final pages = (data['pages'] as List).cast<Map>();
              return ListView(
                padding: const EdgeInsets.all(HarborSpace.s4),
                children: [
                  for (final pg in pages)
                    Padding(
                      padding: const EdgeInsets.only(bottom: HarborSpace.s2),
                      child: Text(
                        'p${pg['index']}: ${pg['text']}',
                        style: t.text.monoOf(t.colors.ink, size: 12),
                      ),
                    ),
                ],
              );
            }
            if (preview['kind'] == 'workbook') {
              final data = preview['preview'] as Map;
              final cells = (data['cells'] as List).cast<Map>();
              return SingleChildScrollView(
                padding: const EdgeInsets.all(HarborSpace.s4),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(l10n.sheetLabel(data['sheet'] as String),
                        style: t.text.captionOf(t.colors.inkMuted)),
                    const SizedBox(height: HarborSpace.s2),
                    for (final c in cells)
                      Padding(
                        padding: const EdgeInsets.only(bottom: 2),
                        child: Text(
                          'r${c['row']}c${c['col']}: '
                          '${c['formula'] ?? c['value'] ?? ''}',
                          style: t.text.monoOf(t.colors.ink, size: 12),
                        ),
                      ),
                  ],
                ),
              );
            }
            final data = preview['preview'] as Map;
            final slides = (data['slides'] as List).cast<Map>();
            return ListView(
              padding: const EdgeInsets.all(HarborSpace.s4),
              children: [
                for (final s in slides)
                  Card(
                    margin: const EdgeInsets.only(bottom: HarborSpace.s2),
                    child: ListTile(
                      title: Text('${s['index']}. ${s['title']}'),
                      subtitle: Text(
                          (s['bullets'] as List).cast<String>().join(' · ')),
                    ),
                  ),
              ],
            );
          }),
        ),
      ]),
    );
  }
}
