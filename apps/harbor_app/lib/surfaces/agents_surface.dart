import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';

/// Agents (UX-014): honest disabled state. Agent orchestration is not a
/// shipped capability of this release, so the surface says exactly that,
/// explains what a profile will contain, and links to what exists today
/// (skill families, durable runs) — no button that would do nothing.
class AgentsSurface extends StatelessWidget {
  const AgentsSurface({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final state = AppStateScope.maybeOf(context);
    final parts = [
      (Icons.memory_outlined, l10n.agentsPartModel, l10n.agentsPartModelBody),
      (
        Icons.construction_outlined,
        l10n.agentsPartTools,
        l10n.agentsPartToolsBody
      ),
      (
        Icons.library_books_outlined,
        l10n.agentsPartKnowledge,
        l10n.agentsPartKnowledgeBody
      ),
      (
        Icons.verified_user_outlined,
        l10n.agentsPartPolicy,
        l10n.agentsPartPolicyBody
      ),
    ];
    return Column(children: [
      HarborSurfaceHeader(
          title: l10n.surfaceAgents, subtitle: l10n.agentsSubtitle),
      Expanded(
        child: HarborPage(
          maxWidth: HarborLayout.readingMax + 160,
          children: [
            HarborBanner(
              tone: HarborBannerTone.warning,
              icon: Icons.lock_clock_outlined,
              title: l10n.agentsUnavailableTitle,
              body: l10n.agentsUnavailableBody,
              action: state == null
                  ? null
                  : Wrap(
                      spacing: HarborSpace.s2,
                      runSpacing: HarborSpace.s2,
                      children: [
                          OutlinedButton.icon(
                            onPressed: () => state.goTo(HarborSurface.skills),
                            icon: const Icon(Icons.construction_outlined,
                                size: 16),
                            label: Text(l10n.agentsGoSkills),
                          ),
                          OutlinedButton.icon(
                            onPressed: () => state.goTo(HarborSurface.activity),
                            icon: const Icon(Icons.timeline_outlined, size: 16),
                            label: Text(l10n.agentsGoActivity),
                          ),
                        ]),
            ),
            const SizedBox(height: HarborSpace.s6),
            HarborSectionHeader(
                title: l10n.agentsWhatTitle, subtitle: l10n.agentsEmptyBody),
            LayoutBuilder(builder: (context, constraints) {
              final columns = constraints.maxWidth >= 720 ? 2 : 1;
              final width =
                  (constraints.maxWidth - (columns - 1) * HarborSpace.s3) /
                      columns;
              return Wrap(
                spacing: HarborSpace.s3,
                runSpacing: HarborSpace.s3,
                children: [
                  for (final (icon, title, body) in parts)
                    SizedBox(
                      width: width,
                      child: HarborCard(
                        child: Row(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Container(
                                width: 36,
                                height: 36,
                                decoration: BoxDecoration(
                                  color: t.colors.brandSoft,
                                  borderRadius:
                                      BorderRadius.circular(HarborRadius.sm),
                                ),
                                child:
                                    Icon(icon, size: 18, color: t.colors.brand),
                              ),
                              const SizedBox(width: HarborSpace.s3),
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(title,
                                        style:
                                            t.text.bodyStrongOf(t.colors.ink)),
                                    const SizedBox(height: 2),
                                    Text(body,
                                        style:
                                            t.text.smallOf(t.colors.inkMuted)),
                                  ],
                                ),
                              ),
                            ]),
                      ),
                    ),
                ],
              );
            }),
          ],
        ),
      ),
    ]);
  }
}
