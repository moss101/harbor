import 'package:flutter/material.dart';

import '../tokens.dart';
import 'models.dart';

/// The motion presets (`none`, `subtle`, `dynamic`, `cinematic`) as radio-like
/// rows with a one-line description each.
class DeckMotionPicker extends StatelessWidget {
  const DeckMotionPicker({
    super.key,
    required this.motions,
    required this.selected,
    required this.onSelected,
    this.names = const {},
    this.descriptions = const {},
    this.semanticsLabel,
  });

  final List<DeckMotionInfo> motions;
  final String selected;
  final ValueChanged<String> onSelected;

  /// Localised overrides by motion id (fallback: the id / the catalogue's
  /// own description).
  final Map<String, String> names, descriptions;
  final String? semanticsLabel;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Semantics(
      container: true,
      label: semanticsLabel,
      child: FocusTraversalGroup(
        policy: ReadingOrderTraversalPolicy(),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            for (final m in motions)
              Padding(
                padding: const EdgeInsets.only(bottom: HarborSpace.s2),
                child: _row(context, t, m),
              ),
          ],
        ),
      ),
    );
  }

  Widget _row(BuildContext context, HarborTheme t, DeckMotionInfo m) {
    final sel = m.id == selected;
    final name = names[m.id] ?? m.id;
    final desc = descriptions[m.id] ?? m.description;
    return Semantics(
      key: ValueKey('deck-motion-${m.id}'),
      container: true,
      button: true,
      selected: sel,
      inMutuallyExclusiveGroup: true,
      onTap: () => onSelected(m.id),
      label: desc.isEmpty ? name : '$name, $desc',
      child: ExcludeSemantics(
        child: Material(
          color: sel ? t.colors.brandSoft : t.colors.surface,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(HarborRadius.sm),
            side: BorderSide(
              color: sel ? t.colors.brand : t.colors.border,
              width: sel ? HarborStroke.focus : HarborStroke.hairline,
            ),
          ),
          clipBehavior: Clip.antiAlias,
          child: InkWell(
            onTap: () => onSelected(m.id),
            hoverColor: t.colors.hover,
            child: ConstrainedBox(
              constraints:
                  const BoxConstraints(minHeight: HarborLayout.touchTarget),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                    horizontal: HarborSpace.s3, vertical: HarborSpace.s2),
                child: Row(children: [
                  Icon(
                      sel ? Icons.radio_button_checked : Icons.radio_button_off,
                      size: 20,
                      color: sel ? t.colors.brand : t.colors.inkMuted),
                  const SizedBox(width: HarborSpace.s3),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(name, style: t.text.bodyStrongOf(t.colors.ink)),
                        if (desc.isNotEmpty)
                          Text(desc,
                              style: t.text.captionOf(t.colors.inkMuted)),
                      ],
                    ),
                  ),
                ]),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
