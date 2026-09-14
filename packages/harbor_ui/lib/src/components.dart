import 'package:flutter/material.dart';

import 'tokens.dart';

/// Execution semantics shared by Trust Pulse and status badges.
enum ExecutionSemantic { local, hybrid, remote, danger }

/// Status meaning with icon + label + color — never color alone
/// (accessibility requirement, goal §17/§27).
class StatusBadge extends StatelessWidget {
  const StatusBadge({
    super.key,
    required this.semantic,
    required this.label,
    this.tooltip,
  });

  final ExecutionSemantic semantic;
  final String label;
  final String? tooltip;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final (Color text, Color fill, Color border, IconData icon) =
        switch (semantic) {
      ExecutionSemantic.local => (
          t.colors.statusLocalText,
          t.colors.statusLocalFill,
          t.colors.statusLocalBorder,
          Icons.shield_outlined,
        ),
      ExecutionSemantic.hybrid => (
          t.colors.statusHybridText,
          t.colors.statusHybridFill,
          t.colors.statusHybridBorder,
          Icons.sync_alt,
        ),
      ExecutionSemantic.remote => (
          t.colors.statusRemoteText,
          t.colors.statusRemoteFill,
          t.colors.statusRemoteBorder,
          Icons.public,
        ),
      ExecutionSemantic.danger => (
          t.colors.statusDangerText,
          t.colors.statusDangerFill,
          t.colors.statusDangerBorder,
          Icons.error_outline,
        ),
    };
    return Tooltip(
      message: tooltip ?? label,
      child: Container(
        padding: const EdgeInsets.symmetric(
            horizontal: HarborSpace.s2 + 2, vertical: HarborSpace.s1),
        decoration: ShapeDecoration(
          color: fill,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(HarborRadius.sm),
            side: BorderSide(color: border),
          ),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 13, color: text),
            const SizedBox(width: HarborSpace.s1),
            Flexible(
              child: Text(label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: t.text.smallOf(text)),
            ),
          ],
        ),
      ),
    );
  }
}

/// Trust Pulse: workspace privacy policy + actual execution location as
/// two separate facts (goal §13). Never imply an allowed remote policy
/// means the current request went remote.
class TrustPulse extends StatelessWidget {
  const TrustPulse({
    super.key,
    required this.policyLabel,
    required this.executionLabel,
    required this.executionSemantic,
  });

  final String policyLabel;
  final String executionLabel;
  final ExecutionSemantic executionSemantic;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Container(
      padding: const EdgeInsets.all(HarborSpace.s3),
      decoration: ShapeDecoration(
        color: t.colors.surfaceRaised,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(HarborRadius.md),
          side: BorderSide(color: t.colors.border),
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(policyLabel, style: t.text.captionOf(t.colors.inkMuted)),
          const SizedBox(height: HarborSpace.s1),
          Row(children: [
            Icon(Icons.shield_outlined, size: 14, color: t.colors.inkMuted),
            const SizedBox(width: HarborSpace.s1),
            Expanded(
                child: Text(policyLabel, style: t.text.smallOf(t.colors.ink))),
          ]),
          const SizedBox(height: HarborSpace.s2),
          StatusBadge(semantic: executionSemantic, label: executionLabel),
        ],
      ),
    );
  }
}

/// Harbor Rail: primary desktop navigation.
class HarborRail extends StatelessWidget {
  const HarborRail({
    super.key,
    required this.destinations,
    required this.selectedIndex,
    required this.onSelected,
    this.width = HarborLayout.desktopRail,
  });

  final List<HarborDestination> destinations;
  final int selectedIndex;
  final ValueChanged<int> onSelected;
  final double width;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Container(
      width: width,
      color: t.colors.surface,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.all(HarborSpace.s4),
            child: Text('Harbor', style: t.text.h2Of(t.colors.brand)),
          ),
          for (final (i, d) in destinations.indexed)
            Padding(
              padding: const EdgeInsets.symmetric(
                  horizontal: HarborSpace.s2, vertical: 2),
              child: Material(
                color: i == selectedIndex
                    ? t.colors.brandSoft
                    : Colors.transparent,
                borderRadius: BorderRadius.circular(HarborRadius.sm),
                child: InkWell(
                  borderRadius: BorderRadius.circular(HarborRadius.sm),
                  onTap: () => onSelected(i),
                  child: Container(
                    constraints: const BoxConstraints(minHeight: 44),
                    padding:
                        const EdgeInsets.symmetric(horizontal: HarborSpace.s3),
                    child: Row(children: [
                      Icon(d.icon,
                          size: 18,
                          color: i == selectedIndex
                              ? t.colors.brand
                              : t.colors.inkMuted),
                      if (width > HarborLayout.desktopRailCollapsed) ...[
                        const SizedBox(width: HarborSpace.s3),
                        Expanded(
                          child: Text(d.label,
                              overflow: TextOverflow.ellipsis,
                              style: t.text.smallOf(i == selectedIndex
                                  ? t.colors.brand
                                  : t.colors.ink)),
                        ),
                      ],
                    ]),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class HarborDestination {
  const HarborDestination(this.label, this.icon);
  final String label;
  final IconData icon;
}

/// Run Trail: user-readable durable agent timeline (goal §24).
class RunTrail extends StatelessWidget {
  const RunTrail({super.key, required this.entries});
  final List<RunTrailEntry> entries;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    if (entries.isEmpty) {
      return Text('No activity yet.', style: t.text.smallOf(t.colors.inkMuted));
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final (i, e) in entries.indexed)
          Padding(
            padding: const EdgeInsets.only(bottom: HarborSpace.s3),
            child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Column(children: [
                Icon(e.icon,
                    size: 15,
                    color: e.failed ? t.colors.danger : t.colors.brand),
                if (i != entries.length - 1)
                  Container(
                    width: 1,
                    height: 18,
                    color: t.colors.border,
                  ),
              ]),
              const SizedBox(width: HarborSpace.s3),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(e.text, style: t.text.smallOf(t.colors.ink)),
                    if (e.detail != null)
                      Text(e.detail!,
                          style: t.text.captionOf(t.colors.inkMuted)),
                  ],
                ),
              ),
            ]),
          ),
      ],
    );
  }
}

class RunTrailEntry {
  const RunTrailEntry(this.text,
      {this.detail, this.failed = false, this.icon = Icons.circle_outlined});
  final String text;
  final String? detail;
  final bool failed;
  final IconData icon;
}

/// Harbor Sheet: the approval/review surface (goal §16). Explains target,
/// scope, base version and consequence before a protected action.
class HarborSheet extends StatelessWidget {
  const HarborSheet({
    super.key,
    required this.title,
    required this.explanation,
    required this.diffSummary,
    required this.onApprove,
    required this.onDeny,
    this.approveLabel = 'Approve',
    this.denyLabel = 'Deny',
  });

  final String title;
  final String explanation;
  final Widget diffSummary;
  final VoidCallback onApprove;
  final VoidCallback onDeny;
  final String approveLabel;
  final String denyLabel;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Container(
      padding: const EdgeInsets.all(HarborSpace.s5),
      decoration: ShapeDecoration(
        color: t.colors.surface,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(HarborRadius.lg),
          side: BorderSide(color: t.colors.border),
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(title, style: t.text.h2Of(t.colors.ink)),
          const SizedBox(height: HarborSpace.s2),
          Text(explanation, style: t.text.smallOf(t.colors.inkMuted)),
          const SizedBox(height: HarborSpace.s4),
          diffSummary,
          const SizedBox(height: HarborSpace.s5),
          Row(children: [
            const Spacer(),
            TextButton(onPressed: onDeny, child: Text(denyLabel)),
            const SizedBox(width: HarborSpace.s2),
            FilledButton(onPressed: onApprove, child: Text(approveLabel)),
          ]),
        ],
      ),
    );
  }
}

/// Model Dock: the active model / provider / execution surface.
class ModelDock extends StatelessWidget {
  const ModelDock({
    super.key,
    required this.modelLabel,
    required this.runtimeLabel,
    required this.semantic,
    this.onTap,
  });

  final String modelLabel;
  final String runtimeLabel;
  final ExecutionSemantic semantic;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(HarborRadius.md),
      child: Container(
        padding: const EdgeInsets.symmetric(
            horizontal: HarborSpace.s3, vertical: HarborSpace.s2),
        decoration: ShapeDecoration(
          color: t.colors.surface,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(HarborRadius.md),
            side: BorderSide(color: t.colors.border),
          ),
        ),
        child: Row(children: [
          StatusBadge(semantic: semantic, label: runtimeLabel),
          const SizedBox(width: HarborSpace.s3),
          Expanded(
            child: Text(modelLabel,
                overflow: TextOverflow.ellipsis,
                style: t.text.smallOf(t.colors.ink)),
          ),
        ]),
      ),
    );
  }
}

/// Fit Score band rendering with the five user-language bands (goal §6).
enum FitBand { excellent, good, limited, tooLarge, unsupported }

extension FitBandLabel on FitBand {
  String get label => switch (this) {
        FitBand.excellent => 'Excellent',
        FitBand.good => 'Good',
        FitBand.limited => 'Limited',
        FitBand.tooLarge => 'Too large',
        FitBand.unsupported => 'Unsupported',
      };
}

class FitScoreBadge extends StatelessWidget {
  const FitScoreBadge({super.key, required this.band, this.reasons = const []});
  final FitBand band;
  final List<String> reasons;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final semantic = switch (band) {
      FitBand.excellent || FitBand.good => ExecutionSemantic.local,
      FitBand.limited => ExecutionSemantic.hybrid,
      FitBand.tooLarge || FitBand.unsupported => ExecutionSemantic.danger,
    };
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      StatusBadge(semantic: semantic, label: band.label),
      for (final r in reasons)
        Padding(
          padding: const EdgeInsets.only(top: 2),
          child: Text('• $r', style: t.text.captionOf(t.colors.inkMuted)),
        ),
    ]);
  }
}

/// Exceptional states are first-class (goal §17): every surface renders
/// these instead of blank screens.
class HarborEmptyState extends StatelessWidget {
  const HarborEmptyState({
    super.key,
    required this.title,
    required this.body,
    this.actionLabel,
    this.onAction,
  });
  final String title;
  final String body;
  final String? actionLabel;
  final VoidCallback? onAction;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    // Scrollable so 200% scaled text never clips (accessibility §27).
    return SingleChildScrollView(
      child: ConstrainedBox(
        constraints:
            BoxConstraints(minHeight: MediaQuery.of(context).size.height * 0.4),
        child: Center(
          child: Padding(
            padding: const EdgeInsets.all(HarborSpace.s8),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.inbox_outlined, size: 32, color: t.colors.inkMuted),
                const SizedBox(height: HarborSpace.s3),
                Text(title,
                    style: t.text.h2Of(t.colors.ink),
                    textAlign: TextAlign.center),
                const SizedBox(height: HarborSpace.s2),
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 420),
                  child: Text(body,
                      textAlign: TextAlign.center,
                      style: t.text.smallOf(t.colors.inkMuted)),
                ),
                if (actionLabel != null) ...[
                  const SizedBox(height: HarborSpace.s4),
                  FilledButton(onPressed: onAction, child: Text(actionLabel!)),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class HarborErrorState extends StatelessWidget {
  const HarborErrorState({super.key, required this.message, this.onRetry});
  final String message;
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(HarborSpace.s8),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const StatusBadge(
                semantic: ExecutionSemantic.danger, label: 'Something failed'),
            const SizedBox(height: HarborSpace.s3),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 420),
              child: Text(message,
                  textAlign: TextAlign.center,
                  style: t.text.smallOf(t.colors.ink)),
            ),
            if (onRetry != null) ...[
              const SizedBox(height: HarborSpace.s4),
              OutlinedButton(onPressed: onRetry, child: const Text('Retry')),
            ],
          ],
        ),
      ),
    );
  }
}

/// Artifact Diff: version-bound human + technical change preview.
class ArtifactDiffView extends StatelessWidget {
  const ArtifactDiffView({
    super.key,
    required this.baseVersion,
    required this.proposedHash,
    required this.entries,
  });

  final String baseVersion;
  final String proposedHash;
  final List<DiffEntryVM> entries;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Container(
      decoration: ShapeDecoration(
        color: t.colors.surfaceRaised,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(HarborRadius.md),
          side: BorderSide(color: t.colors.border),
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.all(HarborSpace.s3),
            child: DefaultTextStyle(
              style: t.text.captionOf(t.colors.inkMuted),
              child: Wrap(
                spacing: HarborSpace.s4,
                children: [
                  Text('base: $baseVersion'),
                  Text(
                      'proposed: ${proposedHash.length >= 16 ? proposedHash.substring(0, 16) : proposedHash}…'),
                ],
              ),
            ),
          ),
          const Divider(height: 1),
          for (final e in entries)
            Padding(
              padding: const EdgeInsets.all(HarborSpace.s3),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(children: [
                    Icon(Icons.change_circle_outlined,
                        size: 14, color: t.colors.brand),
                    const SizedBox(width: HarborSpace.s2),
                    Expanded(
                        child: Text(e.summary,
                            style: t.text.smallOf(t.colors.ink))),
                  ]),
                  if (e.before != null)
                    Padding(
                      padding:
                          const EdgeInsets.only(left: HarborSpace.s5, top: 2),
                      child: Text('- ${e.before}',
                          style: t.text
                              .monoOf(t.colors.statusDangerText, size: 12)),
                    ),
                  if (e.after != null)
                    Padding(
                      padding:
                          const EdgeInsets.only(left: HarborSpace.s5, top: 2),
                      child: Text('+ ${e.after}',
                          style: t.text
                              .monoOf(t.colors.statusLocalText, size: 12)),
                    ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

class DiffEntryVM {
  const DiffEntryVM({required this.summary, this.before, this.after});
  final String summary;
  final String? before;
  final String? after;
}
