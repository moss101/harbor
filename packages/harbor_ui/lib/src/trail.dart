import 'package:flutter/material.dart';

import 'tokens.dart';

/// Marker state for a Run Trail step (§11): done, active, pending, failed.
enum RunTrailMarker { done, active, pending, failed }

class RunTrailEntry {
  const RunTrailEntry(
    this.text, {
    this.detail,
    this.failed = false,
    this.icon = Icons.circle_outlined,
    this.marker,
    this.technical,
  });
  final String text;
  final String? detail;
  final bool failed;
  final IconData icon;

  /// Explicit marker; defaults to failed → failed, else done.
  final RunTrailMarker? marker;

  /// Technical line rendered in LTR monospace (event ids, hashes).
  final String? technical;

  RunTrailMarker get effectiveMarker =>
      marker ?? (failed ? RunTrailMarker.failed : RunTrailMarker.done);
}

/// Run Trail: user-readable durable agent timeline (goal §24). Each step
/// shows its readable summary; the technical line (UX-016/ACC-035) is
/// expandable per step, collapsed by default.
class RunTrail extends StatelessWidget {
  const RunTrail({
    super.key,
    required this.entries,
    this.emptyLabel = 'No activity yet.',
  });
  final List<RunTrailEntry> entries;
  final String emptyLabel;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    if (entries.isEmpty) {
      return Text(emptyLabel, style: t.text.smallOf(t.colors.inkMuted));
    }
    return Semantics(
      container: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final (i, e) in entries.indexed)
            _TrailRow(entry: e, last: i == entries.length - 1),
        ],
      ),
    );
  }
}

class _TrailRow extends StatefulWidget {
  const _TrailRow({required this.entry, required this.last});
  final RunTrailEntry entry;
  final bool last;

  @override
  State<_TrailRow> createState() => _TrailRowState();
}

class _TrailRowState extends State<_TrailRow> {
  bool _technicalExpanded = false;

  @override
  Widget build(BuildContext context) {
    final entry = widget.entry;
    final last = widget.last;
    final t = HarborTheme.of(context);
    final c = t.colors;
    final (Color color, IconData icon) = switch (entry.effectiveMarker) {
      RunTrailMarker.done => (c.brand, entry.icon),
      RunTrailMarker.active => (c.brand, Icons.radio_button_checked),
      RunTrailMarker.pending => (c.inkMuted, Icons.circle_outlined),
      RunTrailMarker.failed => (c.danger, Icons.error_outline),
    };
    return IntrinsicHeight(
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SizedBox(
            width: 20,
            child: Column(children: [
              Padding(
                padding: const EdgeInsets.only(top: 3),
                child: Icon(icon, size: 15, color: color),
              ),
              if (!last)
                Expanded(
                  child: Container(
                    width: 1,
                    margin: const EdgeInsets.symmetric(vertical: 3),
                    color: c.border,
                  ),
                ),
            ]),
          ),
          const SizedBox(width: HarborSpace.s3),
          Expanded(
            child: Padding(
              padding: EdgeInsets.only(bottom: last ? 0 : HarborSpace.s3),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(entry.text,
                      style: t.text.smallOf(
                          entry.effectiveMarker == RunTrailMarker.pending
                              ? c.inkMuted
                              : c.ink)),
                  if (entry.detail != null)
                    Text(entry.detail!, style: t.text.captionOf(c.inkMuted)),
                  if (entry.technical != null) ...[
                    Align(
                      alignment: AlignmentDirectional.centerStart,
                      child: InkWell(
                        borderRadius: BorderRadius.circular(8),
                        onTap: () => setState(
                            () => _technicalExpanded = !_technicalExpanded),
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                              horizontal: 4, vertical: 2),
                          child: Row(mainAxisSize: MainAxisSize.min, children: [
                            Icon(
                                _technicalExpanded
                                    ? Icons.expand_less
                                    : Icons.expand_more,
                                size: 14,
                                color: c.inkMuted),
                            const SizedBox(width: 2),
                          ]),
                        ),
                      ),
                    ),
                    if (_technicalExpanded)
                      Directionality(
                        textDirection: TextDirection.ltr,
                        child: Text(entry.technical!,
                            style: t.text.monoOf(c.inkMuted, size: 11)),
                      ),
                  ],
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
