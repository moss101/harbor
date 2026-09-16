import 'package:flutter/material.dart';

import 'tokens.dart';

/// Background-operation card: phase label, detail, determinate bar when
/// totals are known, bytes/items line and a labeled cancel action.
/// Progress is real (fed from the core's op snapshots) — never a fake
/// indeterminate spinner that pretends to finish.
class HarborOpProgress extends StatelessWidget {
  const HarborOpProgress({
    super.key,
    required this.title,
    this.detail,
    this.progressLine,
    this.fraction,
    this.onCancel,
    this.cancelLabel = 'Cancel',
    this.icon = Icons.downloading_outlined,
    this.identifier,
  });

  final String title;
  final String? detail;
  final String? progressLine;

  /// 0..1 when known; null renders an indeterminate bar.
  final double? fraction;
  final VoidCallback? onCancel;
  final String cancelLabel;
  final IconData icon;

  /// Technical id (package id, repo id) shown LTR.
  final String? identifier;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final motion = HarborMotion.of(context);
    return Semantics(
      container: true,
      liveRegion: true,
      child: Container(
        padding: const EdgeInsets.all(HarborSpace.s4),
        decoration: ShapeDecoration(
          color: t.colors.surface,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(HarborRadius.md),
            side: BorderSide(color: t.colors.brand.withValues(alpha: 0.4)),
          ),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Icon(icon, size: 18, color: t.colors.brand),
              const SizedBox(width: HarborSpace.s2 + 2),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(title, style: t.text.bodyStrongOf(t.colors.ink)),
                    if (identifier != null)
                      Directionality(
                        textDirection: TextDirection.ltr,
                        child: Text(identifier!,
                            style: t.text.monoOf(t.colors.inkMuted, size: 11),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis),
                      ),
                    if (detail != null && detail!.isNotEmpty)
                      Text(detail!,
                          style: t.text.smallOf(t.colors.inkMuted),
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis),
                  ],
                ),
              ),
              if (onCancel != null)
                TextButton.icon(
                  onPressed: onCancel,
                  icon: const Icon(Icons.close, size: 16),
                  label: Text(cancelLabel),
                ),
            ]),
            const SizedBox(height: HarborSpace.s3),
            TweenAnimationBuilder<double>(
              tween: Tween(end: fraction ?? 0),
              duration: motion.normal,
              curve: HarborMotion.easing,
              builder: (context, value, _) => LinearProgressIndicator(
                value: fraction == null ? null : value,
                semanticsLabel: title,
              ),
            ),
            if (progressLine != null) ...[
              const SizedBox(height: HarborSpace.s2),
              Text(progressLine!, style: t.text.captionOf(t.colors.inkMuted)),
            ],
          ],
        ),
      ),
    );
  }
}
