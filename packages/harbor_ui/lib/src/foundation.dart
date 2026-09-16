import 'package:flutter/material.dart';

import 'breakpoints.dart';
import 'tokens.dart';

/// Bordered surface container — the base for every card-like block.
class HarborCard extends StatelessWidget {
  const HarborCard({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(HarborSpace.s4),
    this.raised = false,
    this.radius = HarborRadius.md,
    this.onTap,
    this.selected = false,
    this.semanticLabel,
    this.clip = Clip.antiAlias,
  });

  final Widget child;
  final EdgeInsetsGeometry padding;
  final bool raised;
  final double radius;
  final VoidCallback? onTap;
  final bool selected;
  final String? semanticLabel;
  final Clip clip;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final shape = RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(radius),
      side: BorderSide(
        color: selected ? t.colors.brand : t.colors.border,
        width: selected ? HarborStroke.focus : HarborStroke.hairline,
      ),
    );
    final body = Padding(padding: padding, child: child);
    final material = Material(
      color: selected
          ? t.colors.brandSoft
          : raised
              ? t.colors.surfaceRaised
              : t.colors.surface,
      shape: shape,
      clipBehavior: clip,
      child: onTap == null
          ? body
          : InkWell(
              onTap: onTap,
              hoverColor: t.colors.hover,
              child: body,
            ),
    );
    if (semanticLabel == null) return material;
    return Semantics(
      label: semanticLabel,
      button: onTap != null,
      selected: selected,
      child: material,
    );
  }
}

/// Section heading with optional trailing action (h2 + caption).
class HarborSectionHeader extends StatelessWidget {
  const HarborSectionHeader({
    super.key,
    required this.title,
    this.subtitle,
    this.trailing,
    this.dense = false,
  });

  final String title;
  final String? subtitle;
  final Widget? trailing;
  final bool dense;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Padding(
      padding: EdgeInsets.only(bottom: dense ? HarborSpace.s2 : HarborSpace.s3),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title,
                    style: dense
                        ? t.text.bodyStrongOf(t.colors.ink)
                        : t.text.h2Of(t.colors.ink)),
                if (subtitle != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 2),
                    child: Text(subtitle!,
                        style: t.text.smallOf(t.colors.inkMuted)),
                  ),
              ],
            ),
          ),
          if (trailing != null) ...[
            const SizedBox(width: HarborSpace.s3),
            trailing!,
          ],
        ],
      ),
    );
  }
}

/// Surface header: title, subtitle and actions. Scales from a compact
/// stacked layout to the desktop title row.
class HarborSurfaceHeader extends StatelessWidget {
  const HarborSurfaceHeader({
    super.key,
    required this.title,
    this.subtitle,
    this.actions = const [],
    this.leading,
    this.titleIsIdentifier = false,
    this.showTitleOnCompact = false,
  });

  final String title;
  final String? subtitle;
  final List<Widget> actions;
  final Widget? leading;

  /// File names and ids keep intrinsic LTR direction inside RTL layouts.
  final bool titleIsIdentifier;

  /// On compact widths the shell's top bar already names the surface, so
  /// the title is omitted unless it carries something else (a file name).
  final bool showTitleOnCompact;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final wc = HarborBreakpoints.of(context);
    final compact = HarborBreakpoints.isCompact(wc);
    // At large text scales on a phone the secondary subtitle would eat
    // the viewport; the title and actions carry the surface.
    final scale = MediaQuery.textScalerOf(context).scale(1);
    final showSubtitle = subtitle != null && !(compact && scale > 1.5);
    final showTitle = !compact || showTitleOnCompact;
    final titleText = Text(
      title,
      style: compact ? t.text.h2Of(t.colors.ink) : t.text.titleOf(t.colors.ink),
      maxLines: 2,
      overflow: TextOverflow.ellipsis,
    );
    final titleBlock = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (showTitle && titleIsIdentifier)
          Directionality(textDirection: TextDirection.ltr, child: titleText)
        else if (showTitle)
          titleText,
        if (showSubtitle)
          Padding(
            padding: EdgeInsets.only(top: showTitle ? HarborSpace.s1 : 0),
            child: Text(subtitle!,
                style: showTitle
                    ? t.text.smallOf(t.colors.inkMuted)
                    : t.text.bodyOf(t.colors.inkMuted),
                maxLines: compact ? 2 : 3,
                overflow: TextOverflow.ellipsis),
          ),
      ],
    );
    final titleEmpty = !showTitle && !showSubtitle && leading == null;
    if (titleEmpty && actionRowOrNull(actions) == null) {
      return const SizedBox(height: HarborSpace.s2);
    }
    // Compact: one horizontally scrolling row (never stacks); wider:
    // a wrapping row with room to spare.
    final actionRow = actions.isEmpty
        ? null
        : compact
            ? SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                clipBehavior: Clip.none,
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    for (final (i, a) in actions.indexed) ...[
                      if (i > 0) const SizedBox(width: HarborSpace.s2),
                      a,
                    ],
                  ],
                ),
              )
            : Wrap(
                spacing: HarborSpace.s2,
                runSpacing: HarborSpace.s2,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: actions,
              );
    return Padding(
      padding: EdgeInsets.fromLTRB(
        HarborBreakpoints.gutter(wc),
        compact ? HarborSpace.s3 : HarborSpace.s6,
        HarborBreakpoints.gutter(wc),
        compact ? HarborSpace.s3 : HarborSpace.s4,
      ),
      child: LayoutBuilder(builder: (context, constraints) {
        final stacked = constraints.maxWidth < 520 || actionRow == null;
        if (stacked) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              if (!titleEmpty)
                Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  if (leading != null) ...[
                    leading!,
                    const SizedBox(width: HarborSpace.s3),
                  ],
                  Expanded(child: titleBlock),
                ]),
              if (actionRow != null) ...[
                if (!titleEmpty) const SizedBox(height: HarborSpace.s3),
                actionRow,
              ],
            ],
          );
        }
        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (leading != null) ...[
              leading!,
              const SizedBox(width: HarborSpace.s3),
            ],
            Expanded(child: titleBlock),
            const SizedBox(width: HarborSpace.s4),
            actionRow,
          ],
        );
      }),
    );
  }
}

/// Null when there are no actions (keeps the header builder readable).
List<Widget>? actionRowOrNull(List<Widget> actions) =>
    actions.isEmpty ? null : actions;

/// Standard scrolling page body: gutters per window class, content capped
/// at the reading or content max width, always scrollable so 200% text
/// never clips (accessibility §15).
class HarborPage extends StatelessWidget {
  const HarborPage({
    super.key,
    required this.children,
    this.maxWidth = HarborLayout.contentMax,
    this.controller,
    this.bottomInset = HarborSpace.s8,
    this.padding,
  });

  final List<Widget> children;
  final double maxWidth;
  final ScrollController? controller;
  final double bottomInset;
  final EdgeInsetsGeometry? padding;

  @override
  Widget build(BuildContext context) {
    final wc = HarborBreakpoints.of(context);
    final gutter = HarborBreakpoints.gutter(wc);
    return Scrollbar(
      controller: controller,
      child: SingleChildScrollView(
        controller: controller,
        padding: padding ?? EdgeInsets.fromLTRB(gutter, 0, gutter, bottomInset),
        child: Center(
          child: ConstrainedBox(
            constraints: BoxConstraints(maxWidth: maxWidth),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: children,
            ),
          ),
        ),
      ),
    );
  }
}

/// Two-column layout that collapses to a stack below
/// [HarborBreakpoints.twoColumnMinCanvas].
class HarborTwoColumn extends StatelessWidget {
  const HarborTwoColumn({
    super.key,
    required this.main,
    required this.aside,
    this.asideWidth = 340,
    this.gap = HarborSpace.s6,
    this.asideFirstWhenStacked = false,
  });

  final Widget main;
  final Widget aside;
  final double asideWidth;
  final double gap;

  /// When stacked, render the aside above the main column.
  final bool asideFirstWhenStacked;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(builder: (context, constraints) {
      if (constraints.maxWidth < HarborBreakpoints.twoColumnMinCanvas) {
        final first = asideFirstWhenStacked ? aside : main;
        final second = asideFirstWhenStacked ? main : aside;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [first, SizedBox(height: gap), second],
        );
      }
      return Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(child: main),
          SizedBox(width: gap),
          SizedBox(width: asideWidth, child: aside),
        ],
      );
    });
  }
}

/// Key/value row for metadata blocks (label muted, value ink; values that
/// are identifiers keep LTR).
class HarborKeyValue extends StatelessWidget {
  const HarborKeyValue({
    super.key,
    required this.label,
    required this.value,
    this.identifier = false,
    this.trailing,
  });

  final String label;
  final String value;
  final bool identifier;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final labelText = Text(label, style: t.text.smallOf(t.colors.inkMuted));
    final valueText = identifier
        ? HarborIdentifier(value, size: 12)
        : Text(value, style: t.text.smallOf(t.colors.ink));
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: HarborSpace.s1 + 2),
      child: LayoutBuilder(builder: (context, constraints) {
        // Narrow cards (phones, 200% text): label and trailing on one
        // line, value on the next — nothing competes for width.
        if (constraints.maxWidth < 380) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(children: [
                Expanded(child: labelText),
                if (trailing != null) ...[
                  const SizedBox(width: HarborSpace.s2),
                  trailing!,
                ],
              ]),
              const SizedBox(height: 2),
              valueText,
            ],
          );
        }
        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(width: 120, child: labelText),
            const SizedBox(width: HarborSpace.s3),
            Expanded(child: valueText),
            if (trailing != null) ...[
              const SizedBox(width: HarborSpace.s2),
              trailing!,
            ],
          ],
        );
      }),
    );
  }
}

/// Technical string (id, hash, filename, formula, path) rendered in
/// monospace with intrinsic LTR direction inside RTL layouts (§6).
class HarborIdentifier extends StatelessWidget {
  const HarborIdentifier(
    this.value, {
    super.key,
    this.size = 13,
    this.color,
    this.maxLines = 1,
    this.selectable = false,
    this.truncateMiddle = false,
  });

  final String value;
  final double size;
  final Color? color;
  final int? maxLines;
  final bool selectable;

  /// Show `abcdef…123456` for long hashes.
  final bool truncateMiddle;

  static String shorten(String v, {int head = 8, int tail = 6}) {
    if (v.length <= head + tail + 1) return v;
    return '${v.substring(0, head)}…${v.substring(v.length - tail)}';
  }

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final style = t.text.monoOf(color ?? t.colors.ink, size: size);
    final shown = truncateMiddle ? shorten(value) : value;
    final text = selectable
        ? SelectableText(shown, style: style, maxLines: maxLines)
        : Text(shown,
            style: style,
            maxLines: maxLines,
            overflow: maxLines == null ? null : TextOverflow.ellipsis,
            softWrap: maxLines != 1);
    return Directionality(
      textDirection: TextDirection.ltr,
      child: Tooltip(
        message: truncateMiddle ? value : '',
        child: text,
      ),
    );
  }
}

/// Inline banner: icon + label + color (never color alone). Used for
/// compatibility warnings, abstentions, degraded core, revoked access.
enum HarborBannerTone { info, success, warning, danger }

class HarborBanner extends StatelessWidget {
  const HarborBanner({
    super.key,
    required this.tone,
    required this.title,
    this.body,
    this.action,
    this.icon,
    this.dense = false,
  });

  final HarborBannerTone tone;
  final String title;
  final String? body;
  final Widget? action;
  final IconData? icon;
  final bool dense;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final c = t.colors;
    final (Color text, Color fill, Color border, IconData defaultIcon) =
        switch (tone) {
      HarborBannerTone.info => (
          c.ink,
          c.brandSoft,
          c.brand.withValues(alpha: 0.35),
          Icons.info_outline,
        ),
      HarborBannerTone.success => (
          c.statusLocalText,
          c.statusLocalFill,
          c.statusLocalBorder,
          Icons.check_circle_outline,
        ),
      HarborBannerTone.warning => (
          c.statusHybridText,
          c.statusHybridFill,
          c.statusHybridBorder,
          Icons.warning_amber_outlined,
        ),
      HarborBannerTone.danger => (
          c.statusDangerText,
          c.statusDangerFill,
          c.statusDangerBorder,
          Icons.error_outline,
        ),
    };
    final bodyColor = tone == HarborBannerTone.info ? c.inkMuted : text;
    return Semantics(
      container: true,
      liveRegion: tone != HarborBannerTone.info,
      child: Container(
        padding: EdgeInsets.all(dense ? HarborSpace.s3 : HarborSpace.s4),
        decoration: ShapeDecoration(
          color: fill,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(HarborRadius.md),
            side: BorderSide(color: border),
          ),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.only(top: 1),
              child: Icon(icon ?? defaultIcon, size: 18, color: text),
            ),
            const SizedBox(width: HarborSpace.s3),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(title, style: t.text.bodyStrongOf(text)),
                  if (body != null)
                    Padding(
                      padding: const EdgeInsets.only(top: 2),
                      child: Text(body!, style: t.text.smallOf(bodyColor)),
                    ),
                  if (action != null)
                    Padding(
                      padding: const EdgeInsets.only(top: HarborSpace.s2),
                      child: action,
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Small metric tile (number + label) for status blocks.
class HarborMetric extends StatelessWidget {
  const HarborMetric({
    super.key,
    required this.value,
    required this.label,
    this.icon,
  });

  final String value;
  final String label;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Container(
      padding: const EdgeInsets.all(HarborSpace.s3),
      decoration: ShapeDecoration(
        color: t.colors.surfaceRaised,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(HarborRadius.sm),
          side: BorderSide(color: t.colors.borderSubtle),
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(children: [
            if (icon != null) ...[
              Icon(icon, size: 14, color: t.colors.inkMuted),
              const SizedBox(width: HarborSpace.s1),
            ],
            Flexible(
              child: Text(label,
                  style: t.text.captionOf(t.colors.inkMuted),
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis),
            ),
          ]),
          const SizedBox(height: 2),
          Text(value,
              style: t.text.h2Of(t.colors.ink),
              maxLines: 1,
              overflow: TextOverflow.ellipsis),
        ],
      ),
    );
  }
}

/// A tappable row with leading icon, title, subtitle and trailing widget
/// that always meets the 44px touch target.
class HarborListRow extends StatelessWidget {
  const HarborListRow({
    super.key,
    required this.title,
    this.subtitle,
    this.leading,
    this.trailing,
    this.onTap,
    this.selected = false,
    this.titleIsIdentifier = false,
    this.dense = false,
  });

  final Widget title;
  final Widget? subtitle;
  final Widget? leading;
  final Widget? trailing;
  final VoidCallback? onTap;
  final bool selected;
  final bool titleIsIdentifier;
  final bool dense;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final content = ConstrainedBox(
      constraints: const BoxConstraints(minHeight: HarborLayout.touchTarget),
      child: Padding(
        padding: EdgeInsets.symmetric(
            horizontal: HarborSpace.s3,
            vertical: dense ? HarborSpace.s2 : HarborSpace.s3),
        child: Row(
          children: [
            if (leading != null) ...[
              IconTheme(
                data: IconThemeData(
                    color: selected ? t.colors.brand : t.colors.inkMuted,
                    size: 20),
                child: leading!,
              ),
              const SizedBox(width: HarborSpace.s3),
            ],
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  DefaultTextStyle(
                    style: t.text.bodyStrongOf(t.colors.ink),
                    child: titleIsIdentifier
                        ? Directionality(
                            textDirection: TextDirection.ltr, child: title)
                        : title,
                  ),
                  if (subtitle != null)
                    Padding(
                      padding: const EdgeInsets.only(top: 2),
                      child: DefaultTextStyle(
                        style: t.text.smallOf(t.colors.inkMuted),
                        child: subtitle!,
                      ),
                    ),
                ],
              ),
            ),
            if (trailing != null) ...[
              const SizedBox(width: HarborSpace.s3),
              trailing!,
            ],
          ],
        ),
      ),
    );
    return Material(
      color: selected ? t.colors.brandSoft : Colors.transparent,
      borderRadius: BorderRadius.circular(HarborRadius.sm),
      child: onTap == null
          ? content
          : InkWell(
              onTap: onTap,
              borderRadius: BorderRadius.circular(HarborRadius.sm),
              hoverColor: t.colors.hover,
              child: content,
            ),
    );
  }
}

/// Neutral pill for counts and short facts.
class HarborPill extends StatelessWidget {
  const HarborPill(this.label, {super.key, this.icon, this.brand = false});
  final String label;
  final IconData? icon;
  final bool brand;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final fg = brand ? t.colors.brand : t.colors.inkMuted;
    return Container(
      padding: const EdgeInsets.symmetric(
          horizontal: HarborSpace.s2 + 2, vertical: 3),
      decoration: ShapeDecoration(
        color: brand ? t.colors.brandSoft : t.colors.surfaceRaised,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(HarborRadius.pill),
          side: BorderSide(
              color: brand
                  ? t.colors.brand.withValues(alpha: 0.3)
                  : t.colors.border),
        ),
      ),
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        if (icon != null) ...[
          Icon(icon, size: 12, color: fg),
          const SizedBox(width: HarborSpace.s1),
        ],
        Flexible(
          child: Text(label,
              style: t.text.captionOf(brand ? t.colors.ink : fg),
              maxLines: 1,
              overflow: TextOverflow.ellipsis),
        ),
      ]),
    );
  }
}

/// Brand wordmark: an anchor glyph in brand blue + "Harbor".
class HarborWordmark extends StatelessWidget {
  const HarborWordmark({super.key, this.compact = false, this.size = 18});
  final bool compact;
  final double size;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final glyph = Container(
      width: size + 10,
      height: size + 10,
      decoration: BoxDecoration(
        color: t.colors.brand,
        borderRadius: BorderRadius.circular(HarborRadius.sm),
      ),
      child: Icon(Icons.anchor, size: size, color: t.colors.onBrand),
    );
    if (compact) {
      return Semantics(label: 'Harbor', child: ExcludeSemantics(child: glyph));
    }
    // A brand mark is not body copy: like navigation labels it scales to
    // at most 1.3x and yields (ellipsis) instead of overflowing the bar.
    return MediaQuery.withClampedTextScaling(
      maxScaleFactor: 1.3,
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        ExcludeSemantics(child: glyph),
        const SizedBox(width: HarborSpace.s2 + 2),
        Flexible(
          child: Text('Harbor',
              maxLines: 1,
              softWrap: false,
              overflow: TextOverflow.ellipsis,
              style: t.text
                  .h2Of(t.colors.ink)
                  .copyWith(letterSpacing: -0.3, fontSize: size)),
        ),
      ]),
    );
  }
}
