import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
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
              label: const Text('Lens'),
            ),
            const Spacer(),
            Text(
              canvasMinApplies
                  ? 'canvas ≥ ${HarborLayout.workCanvasMin.toInt()}px rule active'
                  : 'viewport-sized editing',
              style: t.text.captionOf(t.colors.inkMuted),
            ),
          ]),
        ),
        Expanded(
          child: HarborEmptyState(
            title: l10n.workEmptyTitle,
            body: l10n.workEmptyBody,
            actionLabel: 'Open file',
          ),
        ),
      ]),
    );
  }
}
