import 'package:flutter/material.dart';

import '../tokens.dart';
import 'models.dart';

/// Row (or wrapped grid) of palette chips. Colour is never the only cue:
/// every chip shows its name, the selected one a check mark and a heavier
/// outline. Swatch colours are the only literal colours in the package's
/// designer widgets (they ARE the data).
class DeckPaletteStrip extends StatelessWidget {
  const DeckPaletteStrip({
    super.key,
    required this.palettes,
    required this.selected,
    required this.onSelected,
    this.ownColoursLabel,
    this.ownColoursSwatches = const [],
    this.semanticsLabel,
    this.horizontal = false,
  });

  final List<DeckPaletteInfo> palettes;

  /// Selected palette id; null = the design's own colours.
  final String? selected;

  /// Called with the palette id, or null for "the design's own colours".
  final ValueChanged<String?> onSelected;

  /// When set, a first chip returns to the design's own colours (null).
  final String? ownColoursLabel;

  /// Swatches shown on that chip (typically the selected design's).
  final List<String> ownColoursSwatches;

  /// Label for the group (screen readers).
  final String? semanticsLabel;

  /// One scrolling row instead of a wrapped grid.
  final bool horizontal;

  @override
  Widget build(BuildContext context) {
    final chips = <Widget>[
      if (ownColoursLabel != null)
        _PaletteChip(
          key: const ValueKey('deck-palette-own'),
          name: ownColoursLabel!,
          swatches: ownColoursSwatches,
          selected: selected == null || selected!.isEmpty,
          onTap: () => onSelected(null),
        ),
      for (final p in palettes)
        _PaletteChip(
          key: ValueKey('deck-palette-${p.id}'),
          name: p.name,
          semanticName: p.mood.isEmpty ? p.name : '${p.name}, ${p.mood}',
          swatches: p.swatches,
          selected: p.id == selected,
          onTap: () => onSelected(p.id),
        ),
    ];
    return Semantics(
      container: true,
      label: semanticsLabel,
      child: FocusTraversalGroup(
        policy: ReadingOrderTraversalPolicy(),
        child: horizontal
            ? SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      for (var i = 0; i < chips.length; i++) ...[
                        if (i > 0) const SizedBox(width: HarborSpace.s2),
                        chips[i],
                      ],
                    ]),
              )
            : Wrap(
                spacing: HarborSpace.s2,
                runSpacing: HarborSpace.s2,
                children: chips,
              ),
      ),
    );
  }
}

class _PaletteChip extends StatelessWidget {
  const _PaletteChip({
    super.key,
    required this.name,
    required this.swatches,
    required this.selected,
    required this.onTap,
    String? semanticName,
  }) : semanticName = semanticName ?? name;

  final String name, semanticName;
  final List<String> swatches;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    // Accents first, then page and ink: show up to four accents + the page.
    final shown = <Color>[];
    final accents = swatches.length > 2 ? swatches.length - 2 : swatches.length;
    for (var i = 0; i < accents && shown.length < 4; i++) {
      final c = parseSwatch(swatches[i]);
      if (c != null) shown.add(c);
    }
    final page = swatches.length >= 2
        ? parseSwatch(swatches[swatches.length - 2])
        : null;
    return Semantics(
      container: true,
      button: true,
      selected: selected,
      inMutuallyExclusiveGroup: true,
      onTap: onTap,
      label: semanticName,
      child: ExcludeSemantics(
        child: ConstrainedBox(
          constraints: const BoxConstraints(
              minWidth: HarborLayout.touchTarget,
              minHeight: HarborLayout.touchTarget,
              maxWidth: 112),
          child: Material(
            color: selected ? t.colors.brandSoft : t.colors.surface,
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(HarborRadius.sm),
              side: BorderSide(
                color: selected ? t.colors.brand : t.colors.border,
                width: selected ? HarborStroke.focus : HarborStroke.hairline,
              ),
            ),
            clipBehavior: Clip.antiAlias,
            child: InkWell(
              onTap: onTap,
              hoverColor: t.colors.hover,
              child: Padding(
                padding: const EdgeInsets.all(HarborSpace.s2),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    SizedBox(
                      height: 22,
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          color: page ?? t.colors.surfaceRaised,
                          borderRadius: BorderRadius.circular(4),
                          border: Border.all(color: t.colors.borderSubtle),
                        ),
                        child: Row(children: [
                          for (final c in shown)
                            Expanded(child: ColoredBox(color: c)),
                        ]),
                      ),
                    ),
                    const SizedBox(height: HarborSpace.s1),
                    Row(mainAxisSize: MainAxisSize.min, children: [
                      if (selected) ...[
                        Icon(Icons.check, size: 14, color: t.colors.brand),
                        const SizedBox(width: 2),
                      ],
                      Flexible(
                        child: Text(name,
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: t.text.captionOf(t.colors.ink)),
                      ),
                    ]),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
