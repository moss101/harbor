import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../foundation.dart';
import '../slides/slide_scene.dart';
import '../slides/slide_view.dart';
import '../tokens.dart';
import 'models.dart';

/// Supplies the scene painted in a design's thumbnail (slide 1 of a sample
/// deck in that design). Null = nothing to draw (the card shows a neutral
/// placeholder); a thrown error is shown as an unavailable thumbnail.
typedef DeckThumbnailBuilder = Future<SlideScene?> Function(String designId);

/// Grid of design cards, each with a LIVE thumbnail painted by [SlideView].
///
/// The gallery is lazy: a thumbnail is requested only when its row is
/// built. Changing [thumbnailKey] (e.g. a new palette) discards the
/// thumbnails and requests them again, so the same layouts visibly
/// recolour. All text comes from the caller (this package has no l10n).
class DeckDesignGallery extends StatelessWidget {
  const DeckDesignGallery({
    super.key,
    required this.designs,
    required this.selected,
    required this.onSelected,
    required this.thumbnailBuilder,
    required this.darkLabel,
    required this.lightLabel,
    this.thumbnailKey,
    this.deckWidth = 12192000,
    this.deckHeight = 6858000,
    this.minCardWidth = 200,
    this.recommended = const {},
    this.recommendedLabel,
    this.thumbnailUnavailableLabel,
    this.semanticsLabel,
    this.shrinkWrap = false,
    this.controller,
    this.padding = EdgeInsets.zero,
  });

  final List<DeckDesignInfo> designs;

  /// Selected design id, or null for none.
  final String? selected;
  final ValueChanged<String> onSelected;
  final DeckThumbnailBuilder thumbnailBuilder;

  /// Chip labels ("Dark" / "Light"); shown with an icon, never colour alone.
  final String darkLabel, lightLabel;

  /// Identity of everything the thumbnails depend on besides the design.
  final Object? thumbnailKey;

  /// Slide size (EMU) of the scenes the builder returns.
  final double deckWidth, deckHeight;

  /// Cards are at least this wide; columns follow the available width.
  final double minCardWidth;

  /// Design ids to flag with [recommendedLabel].
  final Set<String> recommended;
  final String? recommendedLabel;
  final String? thumbnailUnavailableLabel;

  /// Label for the whole group (screen readers).
  final String? semanticsLabel;

  /// Size to the content instead of filling a bounded height (the parent
  /// scrolls). Every thumbnail is then requested up front.
  final bool shrinkWrap;
  final ScrollController? controller;
  final EdgeInsetsGeometry padding;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      label: semanticsLabel,
      child: LayoutBuilder(builder: (context, c) {
        const gap = HarborSpace.s3;
        final width = c.maxWidth.isFinite ? c.maxWidth : 640.0;
        final cols =
            math.max(1, ((width + gap) / (minCardWidth + gap)).floor());
        final rows = (designs.length / cols).ceil();
        return FocusTraversalGroup(
          policy: ReadingOrderTraversalPolicy(),
          child: ListView.separated(
            controller: controller,
            shrinkWrap: shrinkWrap,
            physics: shrinkWrap ? const NeverScrollableScrollPhysics() : null,
            padding: padding,
            itemCount: rows,
            separatorBuilder: (_, __) => const SizedBox(height: gap),
            itemBuilder: (context, r) {
              final slice = designs.skip(r * cols).take(cols).toList();
              return IntrinsicHeight(
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    for (var i = 0; i < cols; i++) ...[
                      if (i > 0) const SizedBox(width: gap),
                      Expanded(
                        child: i < slice.length
                            ? _DesignCard(
                                key: ValueKey('deck-design-${slice[i].id}'),
                                info: slice[i],
                                selected: slice[i].id == selected,
                                recommended: recommended.contains(slice[i].id),
                                recommendedLabel: recommendedLabel,
                                onTap: () => onSelected(slice[i].id),
                                gallery: this,
                              )
                            : const SizedBox.shrink(),
                      ),
                    ],
                  ],
                ),
              );
            },
          ),
        );
      }),
    );
  }
}

class _DesignCard extends StatelessWidget {
  const _DesignCard({
    super.key,
    required this.info,
    required this.selected,
    required this.recommended,
    required this.recommendedLabel,
    required this.onTap,
    required this.gallery,
  });

  final DeckDesignInfo info;
  final bool selected, recommended;
  final String? recommendedLabel;
  final VoidCallback onTap;
  final DeckDesignGallery gallery;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final mode = info.dark ? gallery.darkLabel : gallery.lightLabel;
    final imageLabel =
        info.tagline.isEmpty ? info.name : '${info.name}: ${info.tagline}';
    return Semantics(
      container: true,
      button: true,
      selected: selected,
      inMutuallyExclusiveGroup: true,
      label: info.name,
      child: HarborCard(
        selected: selected,
        padding: const EdgeInsets.all(HarborSpace.s2),
        radius: HarborRadius.md,
        onTap: onTap,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            Semantics(
              container: true,
              image: true,
              label: imageLabel,
              child: ExcludeSemantics(
                child: ClipRRect(
                  borderRadius: BorderRadius.circular(HarborRadius.sm / 2),
                  child: _DesignThumbnail(
                    designId: info.id,
                    gallery: gallery,
                    // Swatches end with page then ink: the page colour is
                    // the placeholder until the scene arrives.
                    fallback: info.swatches.length >= 2
                        ? parseSwatch(info.swatches[info.swatches.length - 2])
                        : null,
                  ),
                ),
              ),
            ),
            const SizedBox(height: HarborSpace.s2),
            Padding(
              padding: const EdgeInsetsDirectional.only(
                  start: HarborSpace.s1, end: HarborSpace.s1),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(children: [
                    Expanded(
                      child: Text(info.name,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: t.text.bodyStrongOf(t.colors.ink)),
                    ),
                    if (selected) ...[
                      const SizedBox(width: HarborSpace.s1),
                      Icon(Icons.check_circle, size: 18, color: t.colors.brand),
                    ],
                  ]),
                  if (info.tagline.isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.only(top: 2),
                      child: Text(info.tagline,
                          style: t.text.captionOf(t.colors.inkMuted)),
                    ),
                  const SizedBox(height: HarborSpace.s2),
                  Wrap(
                      spacing: HarborSpace.s1,
                      runSpacing: HarborSpace.s1,
                      children: [
                        HarborPill(mode,
                            icon: info.dark
                                ? Icons.dark_mode_outlined
                                : Icons.light_mode_outlined),
                        if (recommended && recommendedLabel != null)
                          HarborPill(recommendedLabel!,
                              icon: Icons.star_outline, brand: true),
                      ]),
                  const SizedBox(height: HarborSpace.s1),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// One live thumbnail. Requests its scene when first built and again when
/// the gallery's [DeckDesignGallery.thumbnailKey] or the design changes.
class _DesignThumbnail extends StatefulWidget {
  const _DesignThumbnail({
    required this.designId,
    required this.gallery,
    required this.fallback,
  });
  final String designId;
  final DeckDesignGallery gallery;
  final Color? fallback;

  @override
  State<_DesignThumbnail> createState() => _DesignThumbnailState();
}

class _DesignThumbnailState extends State<_DesignThumbnail> {
  SlideScene? _scene;
  bool _failed = false;
  int _generation = 0;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(_DesignThumbnail old) {
    super.didUpdateWidget(old);
    if (old.designId != widget.designId ||
        old.gallery.thumbnailKey != widget.gallery.thumbnailKey) {
      _load();
    }
  }

  void _load() {
    final gen = ++_generation;
    Future<SlideScene?> f;
    try {
      f = widget.gallery.thumbnailBuilder(widget.designId);
    } catch (_) {
      f = Future.error(StateError('thumbnail'));
    }
    f.then((scene) {
      if (!mounted || gen != _generation) return;
      setState(() {
        _scene = scene;
        _failed = false;
      });
    }, onError: (Object _) {
      if (!mounted || gen != _generation) return;
      setState(() {
        _scene = null;
        _failed = true;
      });
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final g = widget.gallery;
    final scene = _scene;
    final ratio = g.deckWidth / g.deckHeight;
    // While a recolour is in flight the previous scene stays on screen
    // (no flash to a spinner), so the change reads as a recolour.
    if (scene != null) {
      return RepaintBoundary(
        child: SlideView(
          scene: scene,
          deckWidth: g.deckWidth,
          deckHeight: g.deckHeight,
        ),
      );
    }
    return AspectRatio(
      aspectRatio: ratio,
      child: ColoredBox(
        color: widget.fallback ?? t.colors.surfaceRaised,
        child: Center(
          child: _failed
              ? Padding(
                  padding: const EdgeInsets.all(HarborSpace.s2),
                  child: Row(mainAxisSize: MainAxisSize.min, children: [
                    Icon(Icons.broken_image_outlined,
                        size: 16, color: t.colors.inkMuted),
                    if (g.thumbnailUnavailableLabel != null) ...[
                      const SizedBox(width: HarborSpace.s1),
                      Flexible(
                        child: Text(g.thumbnailUnavailableLabel!,
                            style: t.text.captionOf(t.colors.inkMuted)),
                      ),
                    ],
                  ]),
                )
              : const SizedBox(
                  width: 18,
                  height: 18,
                  child: CircularProgressIndicator(strokeWidth: 2)),
        ),
      ),
    );
  }
}
