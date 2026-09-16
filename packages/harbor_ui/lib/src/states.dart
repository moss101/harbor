import 'package:flutter/material.dart';

import 'status.dart';
import 'tokens.dart';

/// Exceptional states are first-class (goal §17): every surface renders
/// these instead of blank screens.
class HarborEmptyState extends StatelessWidget {
  const HarborEmptyState({
    super.key,
    required this.title,
    required this.body,
    this.actionLabel,
    this.onAction,
    this.icon = Icons.inbox_outlined,
    this.secondaryActionLabel,
    this.onSecondaryAction,
    this.footer,
    this.compact = false,
    this.scrollable = true,
  });
  final String title;
  final String body;
  final String? actionLabel;
  final VoidCallback? onAction;
  final IconData icon;
  final String? secondaryActionLabel;
  final VoidCallback? onSecondaryAction;

  /// Extra content under the actions (e.g. supported-type chips).
  final Widget? footer;

  /// Inline variant for cards (no vertical centering / min height).
  final bool compact;

  /// When false the state renders as a plain centered block for hosts
  /// that already scroll (and need intrinsic sizing).
  final bool scrollable;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final content = Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: compact ? 40 : 56,
          height: compact ? 40 : 56,
          decoration: BoxDecoration(
            color: t.colors.brandSoft,
            borderRadius: BorderRadius.circular(HarborRadius.lg),
          ),
          child: Icon(icon, size: compact ? 20 : 28, color: t.colors.brand),
        ),
        SizedBox(height: compact ? HarborSpace.s3 : HarborSpace.s4),
        Text(title,
            style: compact
                ? t.text.bodyStrongOf(t.colors.ink)
                : t.text.h2Of(t.colors.ink),
            textAlign: TextAlign.center),
        const SizedBox(height: HarborSpace.s2),
        ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 440),
          child: Text(body,
              textAlign: TextAlign.center,
              style: t.text.smallOf(t.colors.inkMuted)),
        ),
        if (actionLabel != null || secondaryActionLabel != null) ...[
          const SizedBox(height: HarborSpace.s4),
          Wrap(
            alignment: WrapAlignment.center,
            spacing: HarborSpace.s2,
            runSpacing: HarborSpace.s2,
            children: [
              if (actionLabel != null)
                FilledButton(onPressed: onAction, child: Text(actionLabel!)),
              if (secondaryActionLabel != null)
                OutlinedButton(
                    onPressed: onSecondaryAction,
                    child: Text(secondaryActionLabel!)),
            ],
          ),
        ],
        if (footer != null) ...[
          const SizedBox(height: HarborSpace.s4),
          footer!,
        ],
      ],
    );
    if (compact) {
      return Padding(
        padding: const EdgeInsets.all(HarborSpace.s4),
        child: Center(child: content),
      );
    }
    if (!scrollable) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(HarborSpace.s8),
          child: content,
        ),
      );
    }
    // Scrollable so 200% scaled text never clips (accessibility §27).
    return LayoutBuilder(builder: (context, constraints) {
      return SingleChildScrollView(
        child: ConstrainedBox(
          constraints: BoxConstraints(
              minHeight:
                  constraints.hasBoundedHeight ? constraints.maxHeight : 0),
          child: Center(
            child: Padding(
              padding: const EdgeInsets.all(HarborSpace.s8),
              child: content,
            ),
          ),
        ),
      );
    });
  }
}

class HarborErrorState extends StatelessWidget {
  const HarborErrorState({
    super.key,
    required this.message,
    this.onRetry,
    this.title = 'Something failed',
    this.retryLabel = 'Retry',
    this.technical,
  });
  final String message;
  final VoidCallback? onRetry;
  final String title;
  final String retryLabel;

  /// Raw error text from the core (LTR monospace, selectable).
  final String? technical;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return LayoutBuilder(builder: (context, constraints) {
      return SingleChildScrollView(
        child: ConstrainedBox(
          constraints: BoxConstraints(
              minHeight:
                  constraints.hasBoundedHeight ? constraints.maxHeight : 0),
          child: Center(
            child: Padding(
              padding: const EdgeInsets.all(HarborSpace.s8),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  StatusBadge(
                      semantic: ExecutionSemantic.danger,
                      label: title,
                      large: true),
                  const SizedBox(height: HarborSpace.s3),
                  ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 440),
                    child: Text(message,
                        textAlign: TextAlign.center,
                        style: t.text.smallOf(t.colors.ink)),
                  ),
                  if (technical != null) ...[
                    const SizedBox(height: HarborSpace.s3),
                    ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 440),
                      child: Directionality(
                        textDirection: TextDirection.ltr,
                        child: SelectableText(technical!,
                            textAlign: TextAlign.center,
                            style: t.text.monoOf(t.colors.inkMuted, size: 11)),
                      ),
                    ),
                  ],
                  if (onRetry != null) ...[
                    const SizedBox(height: HarborSpace.s4),
                    OutlinedButton.icon(
                        onPressed: onRetry,
                        icon: const Icon(Icons.refresh, size: 16),
                        label: Text(retryLabel)),
                  ],
                ],
              ),
            ),
          ),
        ),
      );
    });
  }
}

/// Loading state with a label (never an unexplained spinner).
class HarborLoadingState extends StatelessWidget {
  const HarborLoadingState({super.key, required this.label, this.detail});
  final String label;
  final String? detail;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(HarborSpace.s8),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const SizedBox(
                width: 28,
                height: 28,
                child: CircularProgressIndicator(strokeWidth: 3)),
            const SizedBox(height: HarborSpace.s4),
            Text(label,
                style: t.text.bodyStrongOf(t.colors.ink),
                textAlign: TextAlign.center),
            if (detail != null)
              Padding(
                padding: const EdgeInsets.only(top: HarborSpace.s1),
                child: Text(detail!,
                    style: t.text.smallOf(t.colors.inkMuted),
                    textAlign: TextAlign.center),
              ),
          ],
        ),
      ),
    );
  }
}

/// Skeleton block for lists that are still loading.
class HarborSkeleton extends StatelessWidget {
  const HarborSkeleton({super.key, this.height = 14, this.width});
  final double height;
  final double? width;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Container(
      height: height,
      width: width,
      decoration: BoxDecoration(
        color: t.colors.skeleton,
        borderRadius: BorderRadius.circular(HarborRadius.sm / 2),
      ),
    );
  }
}
