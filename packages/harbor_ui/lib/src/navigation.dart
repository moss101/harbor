import 'package:flutter/material.dart';

import 'foundation.dart';
import 'tokens.dart';

class HarborDestination {
  const HarborDestination(this.label, this.icon,
      {this.selectedIcon, this.badge});
  final String label;
  final IconData icon;
  final IconData? selectedIcon;

  /// Optional short badge text (e.g. active operation count).
  final String? badge;
}

/// Harbor Rail: primary desktop/tablet navigation.
///
/// - `extended` (220px): icon + label rows, wordmark header.
/// - collapsed (72px): icon-only rows with tooltips, glyph header.
/// - `medium` (icon above caption): tablet-class side pattern.
///
/// The destination list scrolls when the window is short (or text is
/// scaled to 200%), so it never overflows. Every row is a focusable
/// semantic button with `selected` state.
class HarborRail extends StatelessWidget {
  const HarborRail({
    super.key,
    required this.destinations,
    required this.selectedIndex,
    required this.onSelected,
    this.width = HarborLayout.desktopRail,
    this.mode,
    this.footer,
    this.header,
    this.shortcutHint,
  });

  final List<HarborDestination> destinations;
  final int selectedIndex;
  final ValueChanged<int> onSelected;
  final double width;

  /// Explicit layout mode; defaults from [width].
  final HarborRailMode? mode;

  /// Global controls under the destinations (Trust chip, Lens toggle…).
  final Widget? footer;

  /// Replaces the default wordmark header.
  final Widget? header;

  /// Builds a shortcut label per index for tooltips (e.g. "⌘1").
  final String Function(int index)? shortcutHint;

  HarborRailMode get _mode =>
      mode ??
      (width >= HarborLayout.desktopRail
          ? HarborRailMode.extended
          : HarborRailMode.collapsed);

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final m = _mode;
    final extended = m == HarborRailMode.extended;
    return Semantics(
      container: true,
      explicitChildNodes: true,
      child: Container(
        width: width,
        decoration: BoxDecoration(
          color: t.colors.surface,
          border: BorderDirectional(end: BorderSide(color: t.colors.border)),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: EdgeInsets.fromLTRB(
                extended ? HarborSpace.s5 : 0,
                HarborSpace.s5,
                extended ? HarborSpace.s4 : 0,
                HarborSpace.s4,
              ),
              child: header ??
                  (extended
                      ? const HarborWordmark()
                      : const Center(child: HarborWordmark(compact: true))),
            ),
            Expanded(
              child: ScrollConfiguration(
                behavior:
                    ScrollConfiguration.of(context).copyWith(scrollbars: false),
                child: ListView(
                  padding: EdgeInsets.symmetric(
                      horizontal: extended ? HarborSpace.s3 : HarborSpace.s2),
                  children: [
                    for (final (i, d) in destinations.indexed)
                      _RailItem(
                        destination: d,
                        selected: i == selectedIndex,
                        mode: m,
                        onTap: () => onSelected(i),
                        shortcut: shortcutHint?.call(i),
                      ),
                  ],
                ),
              ),
            ),
            if (footer != null)
              Padding(
                padding: EdgeInsets.fromLTRB(
                  extended ? HarborSpace.s3 : HarborSpace.s2,
                  HarborSpace.s2,
                  extended ? HarborSpace.s3 : HarborSpace.s2,
                  HarborSpace.s3,
                ),
                child: footer,
              ),
          ],
        ),
      ),
    );
  }
}

enum HarborRailMode { extended, collapsed, medium }

class _RailItem extends StatelessWidget {
  const _RailItem({
    required this.destination,
    required this.selected,
    required this.mode,
    required this.onTap,
    this.shortcut,
  });

  final HarborDestination destination;
  final bool selected;
  final HarborRailMode mode;
  final VoidCallback onTap;
  final String? shortcut;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final c = t.colors;
    final motion = HarborMotion.of(context);
    final icon = Icon(
      selected
          ? (destination.selectedIcon ?? destination.icon)
          : destination.icon,
      size: 20,
      color: selected ? c.brand : c.inkMuted,
    );
    final badge = destination.badge;
    Widget iconWithBadge = icon;
    if (badge != null) {
      iconWithBadge = Badge(
        label: Text(badge),
        backgroundColor: c.brand,
        textColor: c.onBrand,
        child: icon,
      );
    }

    final Widget content = switch (mode) {
      HarborRailMode.extended => Row(children: [
          iconWithBadge,
          const SizedBox(width: HarborSpace.s3),
          Expanded(
            child: Text(destination.label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: selected
                    ? t.text.bodyStrongOf(c.ink)
                    : t.text.bodyOf(c.ink)),
          ),
          if (shortcut != null)
            Text(shortcut!, style: t.text.captionOf(c.inkMuted)),
        ]),
      HarborRailMode.collapsed => Center(child: iconWithBadge),
      HarborRailMode.medium => Column(
          mainAxisSize: MainAxisSize.min,
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            iconWithBadge,
            const SizedBox(height: HarborSpace.s1),
            Text(destination.label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.center,
                style: t.text.captionOf(selected ? c.brand : c.inkMuted)),
          ],
        ),
    };

    final row = AnimatedContainer(
      duration: motion.fast,
      curve: HarborMotion.easing,
      constraints: BoxConstraints(
        minHeight:
            mode == HarborRailMode.medium ? 56 : HarborLayout.touchTarget,
      ),
      padding: EdgeInsets.symmetric(
        horizontal: mode == HarborRailMode.extended ? HarborSpace.s3 : 0,
        vertical: mode == HarborRailMode.medium ? HarborSpace.s2 : 0,
      ),
      decoration: BoxDecoration(
        color: selected ? c.brandSoft : Colors.transparent,
        borderRadius: BorderRadius.circular(HarborRadius.sm),
      ),
      child: content,
    );

    final tooltipMessage = mode == HarborRailMode.extended
        ? ''
        : shortcut == null
            ? destination.label
            : '${destination.label} · $shortcut';

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Semantics(
        button: true,
        selected: selected,
        label: destination.label,
        child: ExcludeSemantics(
          child: Tooltip(
            message: tooltipMessage,
            waitDuration: const Duration(milliseconds: 500),
            child: Material(
              color: Colors.transparent,
              borderRadius: BorderRadius.circular(HarborRadius.sm),
              child: InkWell(
                onTap: onTap,
                borderRadius: BorderRadius.circular(HarborRadius.sm),
                hoverColor: c.hover,
                focusColor: c.focusRing.withValues(alpha: 0.18),
                child: row,
              ),
            ),
          ),
        ),
      ),
    );
  }
}
