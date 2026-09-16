import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import '../widgets/ops.dart';
import '../widgets/trust.dart';

/// Harbor Lens: context, run and knowledge inspector. Every fact comes
/// from the core (Trust Pulse, active model, background work, durable
/// runs, index status) — never sample data.
class HarborLens extends StatelessWidget {
  const HarborLens({super.key, this.onClose});

  /// Present when the Lens is transient (drawer / sheet).
  final VoidCallback? onClose;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final l10n = AppLocalizations.of(context)!;
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final state = AppStateScope.maybeOf(context);
    final runs = service?.runs ?? const [];
    final ops = service?.activeOps ?? const [];
    final models = service?.installedModels ?? const [];
    final chatModel = state?.chatModel;
    final activeModel = models.isEmpty
        ? null
        : models.firstWhere(
            (m) => m['id'] == chatModel,
            orElse: () => models.first,
          );

    void go(HarborSurface s) {
      onClose?.call();
      state?.goTo(s);
    }

    return Material(
      color: t.colors.surfaceRaised,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsetsDirectional.fromSTEB(
                HarborSpace.s4, HarborSpace.s4, HarborSpace.s2, HarborSpace.s2),
            child: Row(children: [
              Icon(Icons.insights_outlined, size: 18, color: t.colors.brand),
              const SizedBox(width: HarborSpace.s2),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(l10n.lensTitle, style: t.text.h2Of(t.colors.ink)),
                    Text(l10n.lensSubtitle,
                        style: t.text.captionOf(t.colors.inkMuted)),
                  ],
                ),
              ),
              if (onClose != null)
                IconButton(
                  tooltip: l10n.closeAction,
                  onPressed: onClose,
                  icon: const Icon(Icons.close),
                ),
            ]),
          ),
          Divider(height: 1, color: t.colors.border),
          Expanded(
            child: ListView(
              padding: const EdgeInsets.all(HarborSpace.s4),
              children: [
                _LensSection(
                  title: l10n.lensSectionTrust,
                  child: TrustPulseCard(service: service, failed: sp.failed),
                ),
                if (sp.failed)
                  Padding(
                    padding: const EdgeInsets.only(bottom: HarborSpace.s5),
                    child: HarborBanner(
                      tone: HarborBannerTone.danger,
                      title: l10n.coreDegradedTitle,
                      body: l10n.coreStartFailed,
                      dense: true,
                      action: sp.failureDetail == null
                          ? null
                          : HarborIdentifier(sp.failureDetail!,
                              size: 11, maxLines: 6),
                    ),
                  ),
                _LensSection(
                  title: l10n.lensSectionModel,
                  action: state == null
                      ? null
                      : TextButton(
                          onPressed: () => go(HarborSurface.models),
                          child: Text(l10n.surfaceModels),
                        ),
                  child: activeModel == null
                      ? Text(l10n.modelsInstalledCount(models.length),
                          style: t.text.smallOf(t.colors.inkMuted))
                      : ModelDock(
                          modelLabel: activeModel['id'] as String,
                          runtimeLabel: runtimeLabelFor(service),
                          semantic: ExecutionSemantic.local,
                          detail: activeModel['runtime'] as String?,
                          onTap: state == null
                              ? null
                              : () => go(HarborSurface.models),
                        ),
                ),
                _LensSection(
                  title: l10n.lensSectionOps,
                  child: ops.isEmpty
                      ? Text(l10n.activityOpsEmptyTitle,
                          style: t.text.smallOf(t.colors.inkMuted))
                      : Column(children: [
                          for (final op in ops)
                            Padding(
                              padding:
                                  const EdgeInsets.only(bottom: HarborSpace.s2),
                              child: OpProgressCard(
                                  progress: op, service: service),
                            ),
                        ]),
                ),
                _LensSection(
                  title: l10n.lensSectionRuns,
                  action: state == null || runs.isEmpty
                      ? null
                      : TextButton(
                          onPressed: () => go(HarborSurface.activity),
                          child: Text(l10n.viewAllAction),
                        ),
                  child: runs.isEmpty
                      ? Text(l10n.runTrailEmpty,
                          style: t.text.smallOf(t.colors.inkMuted))
                      : Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            for (final r in runs.take(5))
                              Padding(
                                padding: const EdgeInsets.only(
                                    bottom: HarborSpace.s2),
                                child: Row(children: [
                                  RunStateBadge(state: r['state'] as String),
                                  const SizedBox(width: HarborSpace.s2),
                                  Expanded(
                                    child: HarborIdentifier(
                                        r['run_id'] as String,
                                        size: 11,
                                        color: t.colors.inkMuted),
                                  ),
                                ]),
                              ),
                          ],
                        ),
                ),
                _LensSection(
                  title: l10n.lensSectionKnowledge,
                  action: state == null
                      ? null
                      : TextButton(
                          onPressed: () => go(HarborSurface.knowledge),
                          child: Text(l10n.surfaceKnowledge),
                        ),
                  child: Row(children: [
                    Icon(
                      service?.knowledgeOpen == true
                          ? Icons.check_circle_outline
                          : Icons.radio_button_unchecked,
                      size: 16,
                      color: service?.knowledgeOpen == true
                          ? t.colors.statusLocalText
                          : t.colors.inkMuted,
                    ),
                    const SizedBox(width: HarborSpace.s2),
                    Expanded(
                      child: Text(
                        service?.knowledgeOpen == true
                            ? l10n.knowledgeSourcesCount(
                                service!.knowledgeSources.length)
                            : l10n.homeKnowledgeNotOpen,
                        style: t.text.smallOf(t.colors.ink),
                      ),
                    ),
                  ]),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _LensSection extends StatelessWidget {
  const _LensSection({required this.title, required this.child, this.action});
  final String title;
  final Widget child;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: HarborSpace.s5),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(children: [
            Expanded(
              child: Text(title.toUpperCase(),
                  style: t.text
                      .captionOf(t.colors.inkMuted)
                      .copyWith(letterSpacing: 0.6)),
            ),
            if (action != null) action!,
          ]),
          const SizedBox(height: HarborSpace.s2),
          child,
        ],
      ),
    );
  }
}
