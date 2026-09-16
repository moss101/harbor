import 'package:flutter/material.dart';

import 'tokens.dart';

/// Artifact Diff: version-bound human + technical change preview.
class ArtifactDiffView extends StatelessWidget {
  const ArtifactDiffView({
    super.key,
    required this.baseVersion,
    required this.proposedHash,
    required this.entries,
    this.baseLabel = 'base',
    this.proposedLabel = 'proposed',
  });

  final String baseVersion;
  final String proposedHash;
  final List<DiffEntryVM> entries;
  final String baseLabel;
  final String proposedLabel;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final shortHash = proposedHash.length >= 16
        ? '${proposedHash.substring(0, 16)}…'
        : proposedHash;
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
            child: Directionality(
              textDirection: TextDirection.ltr,
              child: DefaultTextStyle(
                style: t.text.monoOf(t.colors.inkMuted, size: 11),
                child: Wrap(
                  spacing: HarborSpace.s4,
                  runSpacing: HarborSpace.s1,
                  children: [
                    Text('$baseLabel: $baseVersion'),
                    Text('$proposedLabel: $shortHash'),
                  ],
                ),
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
                    _DiffLine(
                        prefix: '-',
                        text: e.before!,
                        color: t.colors.statusDangerText,
                        fill: t.colors.statusDangerFill),
                  if (e.after != null)
                    _DiffLine(
                        prefix: '+',
                        text: e.after!,
                        color: t.colors.statusLocalText,
                        fill: t.colors.statusLocalFill),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

class _DiffLine extends StatelessWidget {
  const _DiffLine({
    required this.prefix,
    required this.text,
    required this.color,
    required this.fill,
  });
  final String prefix, text;
  final Color color, fill;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Padding(
      padding: const EdgeInsetsDirectional.only(start: HarborSpace.s5, top: 2),
      child: Directionality(
        textDirection: TextDirection.ltr,
        child: Container(
          width: double.infinity,
          padding: const EdgeInsets.symmetric(
              horizontal: HarborSpace.s2, vertical: 2),
          decoration: BoxDecoration(
            color: fill,
            borderRadius: BorderRadius.circular(4),
          ),
          child: Text('$prefix $text', style: t.text.monoOf(color, size: 12)),
        ),
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
