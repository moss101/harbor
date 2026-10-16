import 'package:flutter/material.dart';
import 'package:harbor_domain/harbor_domain.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/request_router.dart';

/// What the router decided for a task, shown next to the composer: the
/// skill whose run sheet was opened plus the alternatives, or — when no
/// single skill was recommended — the reason, choices and a way to the
/// Skills library. Picking a chip only opens that skill's run sheet.
class RoutePanel extends StatelessWidget {
  const RoutePanel({
    super.key,
    required this.decision,
    required this.onPick,
    this.onBrowse,
    this.onDismiss,
  });
  final RouteDecision decision;
  final ValueChanged<SkillSummary> onPick;
  final VoidCallback? onBrowse;
  final VoidCallback? onDismiss;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final (String note, List<SkillSummary> choices, String? choicesHeading) =
        switch (decision) {
      RouteSkill(:final skill, :final alternatives) => (
          l10n.routeMatched(skill.title),
          alternatives,
          alternatives.isEmpty ? null : l10n.routeOtherSkills,
        ),
      RouteNone(:final reason, :final suggestions) => (
          switch (reason) {
            RouteNoneReason.noIndex => l10n.routeNoIndex,
            RouteNoneReason.ambiguous => l10n.routeAmbiguous,
            RouteNoneReason.noMatch => l10n.routeNoMatch,
            RouteNoneReason.noRunnableSkill => l10n.routeNoRunnable,
          },
          suggestions,
          null,
        ),
      RouteAsk() => ('', const <SkillSummary>[], null),
    };
    if (decision is RouteAsk) return const SizedBox.shrink();
    return HarborCard(
      key: const ValueKey('route-panel'),
      padding: const EdgeInsets.all(HarborSpace.s3),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Icon(
                  decision is RouteSkill
                      ? Icons.account_tree_outlined
                      : Icons.alt_route_outlined,
                  size: 16,
                  color: t.colors.brand),
            ),
            const SizedBox(width: HarborSpace.s2),
            Expanded(
                child: Text(note, style: t.text.smallOf(t.colors.inkMuted))),
            if (onDismiss != null)
              IconButton(
                tooltip: l10n.answerDismiss,
                visualDensity: VisualDensity.compact,
                onPressed: onDismiss,
                icon: const Icon(Icons.close, size: 18),
              ),
          ]),
          if (choicesHeading != null) ...[
            const SizedBox(height: HarborSpace.s2),
            Text(choicesHeading, style: t.text.captionOf(t.colors.inkMuted)),
          ],
          if (choices.isNotEmpty) ...[
            const SizedBox(height: HarborSpace.s2),
            Wrap(
              spacing: HarborSpace.s2,
              runSpacing: HarborSpace.s1,
              children: [
                for (final s in choices)
                  ActionChip(
                    key: ValueKey('route-skill-${s.id}'),
                    avatar: Icon(Icons.account_tree_outlined,
                        size: 16, color: t.colors.brand),
                    // Skill titles are data from the core (English).
                    label: Text(s.title),
                    onPressed: () => onPick(s),
                  ),
              ],
            ),
          ],
          if (onBrowse != null && decision is RouteNone)
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: TextButton.icon(
                key: const ValueKey('route-browse-skills'),
                onPressed: onBrowse,
                icon: const Icon(Icons.auto_awesome_motion_outlined, size: 16),
                label: Text(l10n.routeBrowseSkills),
              ),
            ),
        ],
      ),
    );
  }
}
