import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';
import '../main.dart';

/// The adaptive product shell (UI authority §20):
/// - <=599: bottom navigation, full-width canvas, Lens as modal sheet
/// - 600-1023: NavigationRail (compact), transient Lens
/// - 1024-1179: collapsed 72px rail, Lens overlay; canvas >= 640
/// - 1180-1279: full rail (only while the canvas stays viable)
/// - 1280+: 220px rail + persistent 320px Lens, canvas >= 640
class AdaptiveShell extends StatelessWidget {
  const AdaptiveShell({super.key, required this.state});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final width = MediaQuery.sizeOf(context).width;
    final windowClass = HarborBreakpoints.classify(width);
    final destinations = harborDestinations(l10n);
    final canvas = surfaceFor(state.surfaceIndex, state);

    final body = switch (windowClass) {
      HarborWindowClass.compact => Scaffold(
          body: SafeArea(child: canvas),
          bottomNavigationBar: NavigationBar(
            selectedIndex: state.surfaceIndex,
            onDestinationSelected: state.selectSurface,
            destinations: [
              for (final d in destinations)
                NavigationDestination(icon: Icon(d.icon), label: d.label),
            ],
          ),
        ),
      _ => Scaffold(
          body: Row(children: [
            NavigationRail(
              selectedIndex: state.surfaceIndex,
              onDestinationSelected: state.selectSurface,
              extended: HarborBreakpoints.railWidth(windowClass) > 72,
              minExtendedWidth: HarborLayout.desktopRail,
              labelType: NavigationRailLabelType.none,
              destinations: [
                for (final d in destinations)
                  NavigationRailDestination(
                      icon: Icon(d.icon), label: Text(d.label)),
              ],
            ),
            const VerticalDivider(width: 1),
            Expanded(child: canvas),
          ]),
        ),
    };

    // Harbor Lens: persistent at 1280+, otherwise reachable as a sheet.
    if (HarborBreakpoints.lensIsPersistent(windowClass)) {
      return Row(children: [
        Expanded(child: body),
        const VerticalDivider(width: 1),
        const SizedBox(width: HarborLayout.desktopLens, child: HarborLens()),
      ]);
    }
    return body;
  }
}

/// Opens the Harbor Lens as a transient sheet (medium + expanded classes).
void openHarborLens(BuildContext context) {
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (_) => const FractionallySizedBox(
      heightFactor: 0.75,
      child: HarborLens(),
    ),
  );
}

/// Harbor Lens: context, run and knowledge inspector. The Run Trail shows
/// the REAL durable runs from the core — never sample data.
class HarborLens extends StatelessWidget {
  const HarborLens({super.key});

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final l10n = AppLocalizations.of(context)!;
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final runs = service?.runs ?? const [];
    return Material(
      color: t.colors.surfaceRaised,
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(HarborSpace.s4),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(l10n.surfaceActivity, style: t.text.h2Of(t.colors.ink)),
            const SizedBox(height: HarborSpace.s3),
            TrustPulse(
              policyLabel: service?.policy ?? l10n.trustPolicy,
              executionLabel: l10n.trustExecutionOnDevice,
              executionSemantic: ExecutionSemantic.local,
            ),
            const SizedBox(height: HarborSpace.s4),
            Text(l10n.surfaceActivity,
                style: t.text.captionOf(t.colors.inkMuted)),
            const SizedBox(height: HarborSpace.s2),
            if (runs.isEmpty)
              Text(l10n.runTrailEmpty, style: t.text.smallOf(t.colors.inkMuted))
            else
              RunTrail(
                entries: [
                  for (final r in runs)
                    RunTrailEntry(
                      r['run_id'] as String,
                      detail: l10n.runStateLine(
                        r['state'] as String,
                        r['active_compute_ms_total'] as int,
                      ),
                      icon: Icons.circle_outlined,
                    ),
                ],
              ),
          ],
        ),
      ),
    );
  }
}
