import 'package:flutter/material.dart';

import 'tokens.dart';

/// Execution semantics shared by Trust Pulse and status badges.
enum ExecutionSemantic { local, hybrid, remote, danger }

/// Visual triple (text, fill, border) + icon for a semantic.
({Color text, Color fill, Color border, IconData icon}) semanticVisual(
    HarborColors c, ExecutionSemantic semantic) {
  return switch (semantic) {
    ExecutionSemantic.local => (
        text: c.statusLocalText,
        fill: c.statusLocalFill,
        border: c.statusLocalBorder,
        icon: Icons.shield_outlined,
      ),
    ExecutionSemantic.hybrid => (
        text: c.statusHybridText,
        fill: c.statusHybridFill,
        border: c.statusHybridBorder,
        icon: Icons.sync_alt,
      ),
    ExecutionSemantic.remote => (
        text: c.statusRemoteText,
        fill: c.statusRemoteFill,
        border: c.statusRemoteBorder,
        icon: Icons.public,
      ),
    ExecutionSemantic.danger => (
        text: c.statusDangerText,
        fill: c.statusDangerFill,
        border: c.statusDangerBorder,
        icon: Icons.error_outline,
      ),
  };
}

/// Status meaning with icon + label + color — never color alone
/// (accessibility requirement, goal §17/§27).
class StatusBadge extends StatelessWidget {
  const StatusBadge({
    super.key,
    required this.semantic,
    required this.label,
    this.tooltip,
    this.icon,
    this.large = false,
  });

  final ExecutionSemantic semantic;
  final String label;
  final String? tooltip;

  /// Override the semantic's default icon (e.g. run-state glyphs).
  final IconData? icon;
  final bool large;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final v = semanticVisual(t.colors, semantic);
    final badge = Container(
      padding: EdgeInsets.symmetric(
        horizontal: large ? HarborSpace.s3 : HarborSpace.s2 + 2,
        vertical: large ? HarborSpace.s2 : HarborSpace.s1,
      ),
      decoration: ShapeDecoration(
        color: v.fill,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(HarborRadius.sm),
          side: BorderSide(color: v.border),
        ),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon ?? v.icon, size: large ? 16 : 13, color: v.text),
          const SizedBox(width: HarborSpace.s1 + 1),
          Flexible(
            child: Text(label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style:
                    large ? t.text.labelOf(v.text) : t.text.captionOf(v.text)),
          ),
        ],
      ),
    );
    if (tooltip == null || tooltip!.isEmpty) return badge;
    return Tooltip(message: tooltip!, child: badge);
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
    this.policyHeading,
    this.executionHeading,
    this.policyDetail,
    this.policySemantic = ExecutionSemantic.local,
  });

  final String policyLabel;
  final String executionLabel;
  final ExecutionSemantic executionSemantic;
  final ExecutionSemantic policySemantic;

  /// Fact headings ("Workspace policy" / "Current execution"). Optional so
  /// the pattern stays usable without localization plumbing.
  final String? policyHeading;
  final String? executionHeading;

  /// Small explanatory line under the policy (e.g. policy version).
  final String? policyDetail;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Semantics(
      container: true,
      child: Container(
        padding: const EdgeInsets.all(HarborSpace.s4),
        decoration: ShapeDecoration(
          color: t.colors.surfaceRaised,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(HarborRadius.md),
            side: BorderSide(color: t.colors.border),
          ),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            _Fact(
              heading: policyHeading,
              icon: Icons.policy_outlined,
              detail: policyDetail,
              child: StatusBadge(
                  semantic: policySemantic, label: policyLabel, large: true),
            ),
            const SizedBox(height: HarborSpace.s3),
            Divider(height: 1, color: t.colors.borderSubtle),
            const SizedBox(height: HarborSpace.s3),
            _Fact(
              heading: executionHeading,
              icon: Icons.memory_outlined,
              child: StatusBadge(
                  semantic: executionSemantic,
                  label: executionLabel,
                  large: true),
            ),
          ],
        ),
      ),
    );
  }
}

class _Fact extends StatelessWidget {
  const _Fact({
    required this.heading,
    required this.icon,
    required this.child,
    this.detail,
  });
  final String? heading;
  final IconData icon;
  final Widget child;
  final String? detail;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (heading != null)
          Padding(
            padding: const EdgeInsets.only(bottom: HarborSpace.s2),
            child: Row(children: [
              Icon(icon, size: 14, color: t.colors.inkMuted),
              const SizedBox(width: HarborSpace.s1 + 2),
              Expanded(
                child:
                    Text(heading!, style: t.text.captionOf(t.colors.inkMuted)),
              ),
            ]),
          ),
        Align(alignment: AlignmentDirectional.centerStart, child: child),
        if (detail != null)
          Padding(
            padding: const EdgeInsets.only(top: HarborSpace.s2),
            child: Text(detail!, style: t.text.captionOf(t.colors.inkMuted)),
          ),
      ],
    );
  }
}

/// Compact Trust chip for app bars and rail footers: "● LOCAL". Tapping
/// opens the full Trust Pulse (in the Harbor Lens).
class TrustChip extends StatelessWidget {
  const TrustChip({
    super.key,
    required this.label,
    required this.semantic,
    this.onTap,
    this.tooltip,
    this.iconOnly = false,
  });

  final String label;
  final ExecutionSemantic semantic;
  final VoidCallback? onTap;
  final String? tooltip;
  final bool iconOnly;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final v = semanticVisual(t.colors, semantic);
    final chip = Container(
      constraints: const BoxConstraints(minHeight: 32, minWidth: 32),
      padding: EdgeInsets.symmetric(
          horizontal: iconOnly ? HarborSpace.s2 : HarborSpace.s3,
          vertical: HarborSpace.s1),
      decoration: ShapeDecoration(
        color: v.fill,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(HarborRadius.pill),
          side: BorderSide(color: v.border),
        ),
      ),
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        Icon(v.icon, size: 14, color: v.text),
        if (!iconOnly) ...[
          const SizedBox(width: HarborSpace.s1 + 2),
          Flexible(
            child: Text(label,
                style: t.text.labelOf(v.text),
                maxLines: 1,
                overflow: TextOverflow.ellipsis),
          ),
        ],
      ]),
    );
    final labeled = Semantics(
      button: onTap != null,
      label: iconOnly ? (tooltip ?? label) : null,
      child: Tooltip(
        message: tooltip ?? label,
        child: Material(
          color: Colors.transparent,
          shape: const StadiumBorder(),
          clipBehavior: Clip.antiAlias,
          child: InkWell(
            onTap: onTap,
            customBorder: const StadiumBorder(),
            child: chip,
          ),
        ),
      ),
    );
    return labeled;
  }
}

/// Agent run states (UI authority §11) mapped to icon + semantic. The
/// label stays the core's technical token; callers add the localized
/// human line beside it.
class RunStateBadge extends StatelessWidget {
  const RunStateBadge({super.key, required this.state, this.label});

  final String state;

  /// Override for the visible label (default: the raw state token).
  final String? label;

  static ({ExecutionSemantic semantic, IconData icon}) visual(String state) {
    return switch (state.toUpperCase()) {
      'CREATED' => (
          semantic: ExecutionSemantic.local,
          icon: Icons.add_circle_outline
        ),
      'PLANNING' => (
          semantic: ExecutionSemantic.local,
          icon: Icons.route_outlined
        ),
      'RUNNING' => (
          semantic: ExecutionSemantic.local,
          icon: Icons.play_circle_outline
        ),
      'PAUSED' => (
          semantic: ExecutionSemantic.hybrid,
          icon: Icons.pause_circle_outline
        ),
      'WAITING_APPROVAL' => (
          semantic: ExecutionSemantic.hybrid,
          icon: Icons.approval_outlined
        ),
      'CANCELLING' => (
          semantic: ExecutionSemantic.hybrid,
          icon: Icons.hourglass_top_outlined
        ),
      'COMPLETED' => (
          semantic: ExecutionSemantic.local,
          icon: Icons.check_circle_outline
        ),
      'FAILED' => (
          semantic: ExecutionSemantic.danger,
          icon: Icons.error_outline
        ),
      'CANCELLED' => (
          semantic: ExecutionSemantic.danger,
          icon: Icons.cancel_outlined
        ),
      'OUTCOME_UNKNOWN' => (
          semantic: ExecutionSemantic.danger,
          icon: Icons.help_outline
        ),
      _ => (semantic: ExecutionSemantic.hybrid, icon: Icons.circle_outlined),
    };
  }

  @override
  Widget build(BuildContext context) {
    final v = visual(state);
    return Directionality(
      textDirection: TextDirection.ltr,
      child: StatusBadge(
        semantic: v.semantic,
        icon: v.icon,
        label: label ?? state,
      ),
    );
  }
}
