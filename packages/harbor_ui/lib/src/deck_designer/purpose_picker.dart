import 'package:flutter/material.dart';

import '../tokens.dart';
import 'models.dart';

/// List of deck purposes (pitch deck, project report, …): name plus a
/// one-line description each; one is selected.
class DeckPurposePicker extends StatelessWidget {
  const DeckPurposePicker({
    super.key,
    required this.purposes,
    required this.selected,
    required this.onSelected,
    this.semanticsLabel,
    this.shrinkWrap = false,
    this.controller,
    this.padding = EdgeInsets.zero,
  });

  final List<DeckPurposeInfo> purposes;

  /// Selected purpose id, or null for none.
  final String? selected;
  final ValueChanged<String> onSelected;
  final String? semanticsLabel;

  /// Size to the content (the parent scrolls) instead of scrolling itself.
  final bool shrinkWrap;
  final ScrollController? controller;
  final EdgeInsetsGeometry padding;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Semantics(
      container: true,
      label: semanticsLabel,
      child: FocusTraversalGroup(
        policy: ReadingOrderTraversalPolicy(),
        child: ListView.separated(
          controller: controller,
          shrinkWrap: shrinkWrap,
          physics: shrinkWrap ? const NeverScrollableScrollPhysics() : null,
          padding: padding,
          itemCount: purposes.length,
          separatorBuilder: (_, __) => const SizedBox(height: HarborSpace.s2),
          itemBuilder: (context, i) {
            final p = purposes[i];
            final sel = p.id == selected;
            return Semantics(
              key: ValueKey('deck-purpose-${p.id}'),
              container: true,
              button: true,
              selected: sel,
              inMutuallyExclusiveGroup: true,
              onTap: () => onSelected(p.id),
              label: p.summary.isEmpty ? p.name : '${p.name}, ${p.summary}',
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
                    onTap: () => onSelected(p.id),
                    hoverColor: t.colors.hover,
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(
                          minHeight: HarborLayout.touchTarget),
                      child: Padding(
                        padding: const EdgeInsets.symmetric(
                            horizontal: HarborSpace.s3,
                            vertical: HarborSpace.s2),
                        child: Row(children: [
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(p.name,
                                    style: t.text.bodyStrongOf(t.colors.ink)),
                                if (p.summary.isNotEmpty)
                                  Text(p.summary,
                                      style:
                                          t.text.captionOf(t.colors.inkMuted)),
                              ],
                            ),
                          ),
                          if (sel) ...[
                            const SizedBox(width: HarborSpace.s2),
                            Icon(Icons.check_circle,
                                size: 20, color: t.colors.brand),
                          ],
                        ]),
                      ),
                    ),
                  ),
                ),
              ),
            );
          },
        ),
      ),
    );
  }
}
