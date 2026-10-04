import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../breakpoints.dart';
import '../foundation.dart';
import '../slides/slide_scene.dart';
import '../slides/slide_view.dart';
import '../states.dart';
import '../tokens.dart';
import 'api.dart';
import 'deck_designer_labels.dart';
import 'design_gallery.dart';
import 'models.dart';
import 'motion_picker.dart';
import 'palette_strip.dart';
import 'purpose_picker.dart';

/// Rendered sample decks, cached per (design, palette, motion, rtl).
/// In-flight requests are shared and a failed one is forgotten, so a retry
/// asks the core again.
class DeckSampleCache {
  DeckSampleCache(this.designer);
  final DeckDesignerApi designer;
  final Map<String, Future<DeckScene>> _futures = {};

  Future<DeckScene> sample(DeckStyleChoice style) {
    final key = style.cacheKey;
    return _futures.putIfAbsent(key, () {
      final f = designer.deckSample(style);
      f.then((_) {}, onError: (Object _) {
        _futures.remove(key);
      });
      return f;
    });
  }

  void forget(DeckStyleChoice style) => _futures.remove(style.cacheKey);
}

/// Full-screen "New presentation" flow: purpose, then design (live
/// thumbnails, palette), then motion and language. Two panes (steps on one
/// side, a live preview on the other) from 840 px wide; a stepper below.
///
/// Pops with the [DeckStarterResult] when the user creates the deck.
class NewPresentationSheet extends StatefulWidget {
  const NewPresentationSheet({
    super.key,
    required this.designer,
    required this.labels,
    this.initial = const DeckDesignPrefs(),
    this.onRemember,
  });

  final DeckDesignerApi designer;

  /// Every user-visible string (the app localises them).
  final DeckDesignerLabels labels;

  /// The user's last choices (applied when the catalogue still knows them).
  final DeckDesignPrefs initial;

  /// Called with the choices when a deck is created.
  final ValueChanged<DeckDesignPrefs>? onRemember;

  static Future<DeckStarterResult?> show(
    BuildContext context, {
    required DeckDesignerApi designer,
    required DeckDesignerLabels labels,
    DeckDesignPrefs initial = const DeckDesignPrefs(),
    ValueChanged<DeckDesignPrefs>? onRemember,
  }) {
    return Navigator.of(context).push<DeckStarterResult>(MaterialPageRoute(
      fullscreenDialog: true,
      builder: (_) => NewPresentationSheet(
          designer: designer,
          labels: labels,
          initial: initial,
          onRemember: onRemember),
    ));
  }

  /// Width from which the sheet shows steps and preview side by side.
  static const double twoPaneMinWidth = 840;

  @override
  State<NewPresentationSheet> createState() => _NewPresentationSheetState();
}

class _NewPresentationSheetState extends State<NewPresentationSheet> {
  late final DeckSampleCache _cache = DeckSampleCache(widget.designer);

  DeckCatalog? _catalog;
  Object? _catalogError;
  bool _loadingCatalog = true;

  int _step = 0;
  String? _purposeId;
  String _designId = '';
  String? _paletteId;
  String _motionId = 'subtle';
  bool _rtl = false;

  bool _creating = false;
  String? _createError;

  // Slide size of the samples (defaults to 16:9 until one arrives).
  double _deckW = 12192000, _deckH = 6858000;

  @override
  void initState() {
    super.initState();
    _loadCatalog();
  }

  Future<void> _loadCatalog() async {
    setState(() {
      _loadingCatalog = true;
      _catalogError = null;
    });
    try {
      final c = await widget.designer.deckCatalog();
      if (!mounted) return;
      _applyCatalog(c);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _catalogError = e;
        _loadingCatalog = false;
      });
    }
  }

  void _applyCatalog(DeckCatalog c) {
    final prefs = widget.initial;
    final purpose = c.purpose(prefs.purpose ?? '') ??
        (c.purposes.isEmpty ? null : c.purposes.first);
    String design = purpose?.theme ?? '';
    if (c.design(prefs.design ?? '') != null) {
      design = prefs.design!;
    } else if (c.design(design) == null && c.designs.isNotEmpty) {
      design = c.designs.first.id;
    }
    final motionIds = {for (final m in c.motions) m.id};
    var motion = motionIds.contains(prefs.motion)
        ? prefs.motion!
        : (motionIds.contains(purpose?.motion) ? purpose!.motion : 'subtle');
    if (motionIds.isNotEmpty && !motionIds.contains(motion)) {
      motion = c.motions.first.id;
    }
    setState(() {
      _catalog = c;
      _loadingCatalog = false;
      _purposeId = purpose?.id;
      _designId = design;
      _paletteId =
          c.palette(prefs.palette ?? '') == null ? null : prefs.palette;
      _motionId = motion;
      _rtl = prefs.rtl;
    });
  }

  DeckStyleChoice get _style => DeckStyleChoice(
      design: _designId, palette: _paletteId, motion: _motionId, rtl: _rtl);

  void _pickPurpose(String id) {
    final p = _catalog?.purpose(id);
    setState(() {
      _purposeId = id;
      // A purpose brings its own suggested design and motion.
      if (p != null) {
        if (_catalog!.design(p.theme) != null) {
          _designId = p.theme;
        }
        if (_catalog!.motions.any((m) => m.id == p.motion)) {
          _motionId = p.motion;
        }
      }
    });
  }

  Future<void> _create() async {
    final purpose = _purposeId;
    if (purpose == null || _creating) return;
    setState(() {
      _creating = true;
      _createError = null;
    });
    try {
      final result = await widget.designer.deckStarter(
        purpose: purpose,
        style: _designId.isEmpty ? null : _style,
      );
      widget.onRemember?.call(DeckDesignPrefs(
        purpose: purpose,
        design: _designId.isEmpty ? null : _designId,
        palette: _paletteId,
        motion: _motionId,
        rtl: _rtl,
      ));
      if (mounted) Navigator.of(context).pop(result);
    } catch (e) {
      if (mounted) {
        setState(() {
          _creating = false;
          _createError = widget.labels.errorText(e);
        });
      }
    }
  }

  static const int _steps = 3;

  bool get _canAdvance => switch (_step) {
        0 => _purposeId != null,
        1 => _designId.isNotEmpty,
        _ => true,
      };

  @override
  Widget build(BuildContext context) {
    final labels = widget.labels;
    final t = HarborTheme.of(context);
    final material = MaterialLocalizations.of(context);
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): () {
          if (!_creating) Navigator.of(context).maybePop();
        },
      },
      child: Scaffold(
        backgroundColor: t.colors.canvas,
        appBar: AppBar(
          backgroundColor: t.colors.surface,
          scrolledUnderElevation: 0,
          title: Text(labels.newTitle),
          leading: IconButton(
            tooltip: material.closeButtonLabel,
            onPressed:
                _creating ? null : () => Navigator.of(context).maybePop(),
            icon: const Icon(Icons.close),
          ),
        ),
        body: SafeArea(
          child: Builder(builder: (context) {
            if (_loadingCatalog) {
              return HarborLoadingState(label: labels.catalogLoading);
            }
            if (_catalogError != null || _catalog == null) {
              return HarborErrorState(
                title: labels.catalogFailed,
                message: labels.catalogFailed,
                technical: _catalogError == null
                    ? null
                    : labels.errorText(_catalogError!),
                retryLabel: labels.retry,
                onRetry: _loadCatalog,
              );
            }
            return LayoutBuilder(builder: (context, c) {
              final wide = c.maxWidth >= NewPresentationSheet.twoPaneMinWidth;
              final steps = _buildSteps(context, wide: wide);
              if (wide) {
                return Row(children: [
                  SizedBox(width: 500, child: steps),
                  VerticalDivider(width: 1, color: t.colors.border),
                  Expanded(
                    child: Padding(
                      padding: const EdgeInsets.all(HarborSpace.s6),
                      child: _PreviewPane(
                        labels: widget.labels,
                        cache: _cache,
                        catalog: _catalog!,
                        style: _style,
                        onSceneSize: _onSceneSize,
                      ),
                    ),
                  ),
                ]);
              }
              return steps;
            });
          }),
        ),
      ),
    );
  }

  void _onSceneSize(double w, double h) {
    if (w == _deckW && h == _deckH) return;
    // Defer: this is called while a preview builds.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        setState(() {
          _deckW = w;
          _deckH = h;
        });
      }
    });
  }

  Widget _buildSteps(BuildContext context, {required bool wide}) {
    final labels = widget.labels;
    final t = HarborTheme.of(context);
    final catalog = _catalog!;
    final stepLabels = [
      labels.stepPurpose,
      labels.stepDesign,
      labels.stepStyle,
    ];
    final gutter = HarborBreakpoints.gutter(
        wide ? HarborWindowClass.expanded : HarborWindowClass.compact);

    Widget heading(String text) => Padding(
          padding: const EdgeInsets.only(bottom: HarborSpace.s3),
          child: Semantics(
              header: true,
              child: Text(text, style: t.text.h2Of(t.colors.ink))),
        );

    final Widget body = switch (_step) {
      0 => Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            heading(labels.purposeHeading),
            Padding(
              padding: const EdgeInsets.only(bottom: HarborSpace.s3),
              child: Text(labels.purposeHelp,
                  style: t.text.captionOf(t.colors.inkMuted)),
            ),
            Expanded(
              child: DeckPurposePicker(
                purposes: catalog.purposes,
                selected: _purposeId,
                onSelected: _pickPurpose,
                semanticsLabel: labels.purposeLabel,
              ),
            ),
          ],
        ),
      1 => _designStep(context, heading, wide: wide),
      _ => _styleStep(context, heading, wide: wide),
    };

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: EdgeInsets.fromLTRB(gutter, HarborSpace.s3, gutter, 0),
          child: _StepHeader(
            labels: stepLabels,
            current: _step,
            ofLabel: (i) => labels.stepOf(i + 1, _steps),
            onTap: (i) {
              if (_creating) return;
              // Forward jumps need the earlier choices to be made.
              if (i > _step && (_purposeId == null)) return;
              setState(() => _step = i);
            },
          ),
        ),
        Expanded(
          child: Padding(
            padding: EdgeInsets.fromLTRB(gutter, HarborSpace.s4, gutter, 0),
            child: body,
          ),
        ),
        if (_createError != null)
          Padding(
            padding: EdgeInsets.fromLTRB(gutter, HarborSpace.s2, gutter, 0),
            child: HarborBanner(
              tone: HarborBannerTone.danger,
              dense: true,
              title: labels.createFailed,
              body: _createError,
              action: TextButton(
                onPressed: _creating ? null : _create,
                child: Text(labels.retry),
              ),
            ),
          ),
        Padding(
          padding: EdgeInsets.all(gutter),
          // A Wrap: with large text the buttons stack instead of squeezing
          // each other's labels.
          child: Wrap(
              alignment:
                  _step > 0 ? WrapAlignment.spaceBetween : WrapAlignment.end,
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: HarborSpace.s3,
              runSpacing: HarborSpace.s2,
              children: [
                if (_step > 0)
                  OutlinedButton(
                    onPressed: _creating ? null : () => setState(() => _step--),
                    style: OutlinedButton.styleFrom(
                        minimumSize: const Size(0, HarborLayout.touchTarget)),
                    child: Text(labels.back),
                  ),
                if (_creating)
                  Padding(
                    padding: const EdgeInsets.all(HarborSpace.s1),
                    child: Semantics(
                      liveRegion: true,
                      label: labels.creating,
                      child: const SizedBox(
                          width: 20,
                          height: 20,
                          child: CircularProgressIndicator(strokeWidth: 2)),
                    ),
                  ),
                if (_step < _steps - 1)
                  FilledButton(
                    onPressed:
                        _canAdvance ? () => setState(() => _step++) : null,
                    style: FilledButton.styleFrom(
                        minimumSize: const Size(0, HarborLayout.touchTarget)),
                    child: Text(labels.next),
                  )
                else
                  FilledButton.icon(
                    onPressed: _purposeId == null || _creating ? null : _create,
                    style: FilledButton.styleFrom(
                        minimumSize: const Size(0, HarborLayout.touchTarget)),
                    icon: const Icon(Icons.slideshow_outlined, size: 18),
                    label: Text(labels.create),
                  ),
              ]),
        ),
      ],
    );
  }

  Widget _designStep(BuildContext context, Widget Function(String) heading,
      {required bool wide}) {
    final labels = widget.labels;
    final catalog = _catalog!;
    final design = catalog.design(_designId);
    final purpose = catalog.purpose(_purposeId ?? '');
    final suggested = {
      if (purpose != null) purpose.theme,
      if (purpose != null) ...purpose.altThemes,
    };
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        heading(labels.designHeading),
        Padding(
          padding: const EdgeInsets.only(bottom: HarborSpace.s2),
          child: DeckPaletteStrip(
            palettes: catalog.palettes,
            selected: _paletteId,
            onSelected: (p) => setState(() => _paletteId = p),
            ownColoursLabel: labels.paletteOwn,
            ownColoursSwatches: design?.swatches ?? const [],
            semanticsLabel: labels.paletteLabel,
            horizontal: true,
          ),
        ),
        Expanded(
          child: DeckDesignGallery(
            designs: catalog.designs,
            selected: _designId,
            onSelected: (id) => setState(() => _designId = id),
            thumbnailBuilder: (id) async {
              final scene = await _cache.sample(_style.copyWith(design: id));
              return scene.slides.isEmpty ? null : scene.slides.first;
            },
            thumbnailKey: '${_paletteId ?? ''}|$_motionId|$_rtl',
            deckWidth: _deckW,
            deckHeight: _deckH,
            minCardWidth: wide ? 200 : 150,
            darkLabel: labels.darkLabel,
            lightLabel: labels.lightLabel,
            recommended: suggested,
            recommendedLabel: labels.suggested,
            thumbnailUnavailableLabel: labels.thumbUnavailable,
            semanticsLabel: labels.galleryLabel,
          ),
        ),
      ],
    );
  }

  Widget _styleStep(BuildContext context, Widget Function(String) heading,
      {required bool wide}) {
    final labels = widget.labels;
    final t = HarborTheme.of(context);
    final catalog = _catalog!;

    return ListView(
      children: [
        heading(labels.styleHeading),
        if (!wide)
          Padding(
            padding: const EdgeInsets.only(bottom: HarborSpace.s4),
            child: SizedBox(
              height: 170,
              child: _PreviewPane(
                labels: labels,
                cache: _cache,
                catalog: catalog,
                style: _style,
                compact: true,
                onSceneSize: _onSceneSize,
              ),
            ),
          ),
        Padding(
          padding: const EdgeInsets.only(bottom: HarborSpace.s2),
          child: Text(labels.motionHeading,
              style: t.text.bodyStrongOf(t.colors.ink)),
        ),
        DeckMotionPicker(
          motions: catalog.motions,
          selected: _motionId,
          onSelected: (m) => setState(() => _motionId = m),
          names: labels.motionNames,
          descriptions: labels.motionDescriptions,
          semanticsLabel: labels.motionLabel,
        ),
        const SizedBox(height: HarborSpace.s4),
        Padding(
          padding: const EdgeInsets.only(bottom: HarborSpace.s2),
          child: Text(labels.languageHeading,
              style: t.text.bodyStrongOf(t.colors.ink)),
        ),
        Semantics(
          container: true,
          label: labels.languageHeading,
          child: Wrap(
            spacing: HarborSpace.s2,
            runSpacing: HarborSpace.s2,
            children: [
              for (final rtl in [false, true])
                ChoiceChip(
                  key: ValueKey('deck-language-${rtl ? 'rtl' : 'ltr'}'),
                  materialTapTargetSize: MaterialTapTargetSize.padded,
                  showCheckmark: true,
                  selected: _rtl == rtl,
                  onSelected: (_) => setState(() => _rtl = rtl),
                  label: Text(rtl ? labels.languageRtl : labels.languageLtr),
                ),
            ],
          ),
        ),
      ],
    );
  }
}

/// Numbered step chips. Each is a labelled, selectable button.
class _StepHeader extends StatelessWidget {
  const _StepHeader({
    required this.labels,
    required this.current,
    required this.ofLabel,
    required this.onTap,
  });
  final List<String> labels;
  final int current;
  final String Function(int) ofLabel;
  final ValueChanged<int> onTap;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Wrap(
      spacing: HarborSpace.s2,
      runSpacing: HarborSpace.s2,
      children: [
        for (var i = 0; i < labels.length; i++)
          Semantics(
            key: ValueKey('deck-step-$i'),
            button: true,
            selected: i == current,
            inMutuallyExclusiveGroup: true,
            label: '${ofLabel(i)}: ${labels[i]}',
            onTap: () => onTap(i),
            child: ExcludeSemantics(
              child: Material(
                color: i == current ? t.colors.brandSoft : t.colors.surface,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(HarborRadius.pill),
                  side: BorderSide(
                    color: i == current ? t.colors.brand : t.colors.border,
                    width: i == current
                        ? HarborStroke.focus
                        : HarborStroke.hairline,
                  ),
                ),
                child: InkWell(
                  customBorder: const StadiumBorder(),
                  onTap: () => onTap(i),
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(
                        minHeight: HarborLayout.touchTarget),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(
                          horizontal: HarborSpace.s3),
                      child: Row(mainAxisSize: MainAxisSize.min, children: [
                        Text('${i + 1}',
                            style: t.text.monoOf(
                                i == current
                                    ? t.colors.brand
                                    : t.colors.inkMuted,
                                size: 12,
                                weight: 600)),
                        const SizedBox(width: HarborSpace.s2),
                        Flexible(
                          child: Text(labels[i],
                              style: t.text.labelOf(t.colors.ink)),
                        ),
                      ]),
                    ),
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

/// The live preview of the current choices: the sample deck's slides, one
/// at a time. [compact] shows just the slide (phone step layout).
class _PreviewPane extends StatefulWidget {
  const _PreviewPane({
    required this.labels,
    required this.cache,
    required this.catalog,
    required this.style,
    required this.onSceneSize,
    this.compact = false,
  });
  final DeckDesignerLabels labels;
  final DeckSampleCache cache;
  final DeckCatalog catalog;
  final DeckStyleChoice style;
  final bool compact;
  final void Function(double w, double h) onSceneSize;

  @override
  State<_PreviewPane> createState() => _PreviewPaneState();
}

class _PreviewPaneState extends State<_PreviewPane> {
  int _slide = 0;

  @override
  Widget build(BuildContext context) {
    final labels = widget.labels;
    final t = HarborTheme.of(context);
    final style = widget.style;
    final info = widget.catalog.design(style.design);
    final name = info?.name ?? style.design;
    return FutureBuilder<DeckScene>(
      future: widget.cache.sample(style),
      builder: (context, snap) {
        final scene = snap.data;
        if (scene != null) widget.onSceneSize(scene.width, scene.height);
        final Widget stage;
        if (snap.hasError) {
          stage = Center(
            child: Column(mainAxisSize: MainAxisSize.min, children: [
              Text(labels.previewUnavailable,
                  style: t.text.bodyStrongOf(t.colors.ink)),
              Text(labels.errorText(snap.error!),
                  textAlign: TextAlign.center,
                  style: t.text.captionOf(t.colors.statusDangerText)),
              TextButton(
                onPressed: () {
                  widget.cache.forget(style);
                  setState(() {});
                },
                child: Text(labels.retry),
              ),
            ]),
          );
        } else if (scene == null) {
          stage = Center(
              child: Semantics(
            liveRegion: true,
            label: labels.catalogLoading,
            child: const SizedBox(
                width: 24,
                height: 24,
                child: CircularProgressIndicator(strokeWidth: 2)),
          ));
        } else {
          final i = _slide.clamp(0, scene.slides.length - 1);
          final slide = scene.slides[i];
          final view = DecoratedBox(
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(HarborRadius.md),
              border: Border.all(color: t.colors.border),
            ),
            child: ClipRRect(
              borderRadius: BorderRadius.circular(HarborRadius.md),
              child: SlideView(
                key: const ValueKey('deck-preview-slide'),
                scene: slide,
                deckWidth: scene.width,
                deckHeight: scene.height,
                semanticsLabel: labels.previewOf(name),
              ),
            ),
          );
          stage = widget.compact
              ? Center(child: view)
              : Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    view,
                    const SizedBox(height: HarborSpace.s3),
                    Row(children: [
                      IconButton(
                        tooltip: labels.previousSlide,
                        onPressed: i == 0
                            ? null
                            : () => setState(() => _slide = i - 1),
                        icon: const Icon(Icons.chevron_left),
                      ),
                      Expanded(
                        child: Text(
                          labels.slideOf(i + 1, scene.slides.length),
                          textAlign: TextAlign.center,
                          style: t.text.captionOf(t.colors.inkMuted),
                        ),
                      ),
                      IconButton(
                        tooltip: labels.nextSlide,
                        onPressed: i >= scene.slides.length - 1
                            ? null
                            : () => setState(() => _slide = i + 1),
                        icon: const Icon(Icons.chevron_right),
                      ),
                    ]),
                  ],
                );
        }
        if (widget.compact) return stage;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(labels.previewHeading, style: t.text.h2Of(t.colors.ink)),
            if (info != null)
              Padding(
                padding: const EdgeInsets.only(top: 2, bottom: HarborSpace.s3),
                child: Text(
                    info.tagline.isEmpty
                        ? info.name
                        : '${info.name} · ${info.tagline}',
                    style: t.text.captionOf(t.colors.inkMuted)),
              ),
            Expanded(child: Center(child: SingleChildScrollView(child: stage))),
          ],
        );
      },
    );
  }
}

// ---------------------------------------------------------------------------
// Change design (open deck)

/// The Change design dialog for an open deck. Pops with the new
/// [DeckStyleChoice] when the user applies it.
class ChangeDesignDialog extends StatefulWidget {
  const ChangeDesignDialog({
    super.key,
    required this.designer,
    required this.labels,
    required this.current,
    required this.currentName,
  });

  final DeckDesignerApi designer;
  final DeckDesignerLabels labels;

  /// The deck's current design, palette, motion and direction.
  final DeckStyleChoice current;
  final String currentName;

  static Future<DeckStyleChoice?> show(
    BuildContext context, {
    required DeckDesignerApi designer,
    required DeckDesignerLabels labels,
    required DeckStyleChoice current,
    required String currentName,
  }) {
    return showDialog<DeckStyleChoice>(
      context: context,
      builder: (_) => ChangeDesignDialog(
          designer: designer,
          labels: labels,
          current: current,
          currentName: currentName),
    );
  }

  @override
  State<ChangeDesignDialog> createState() => _ChangeDesignDialogState();
}

class _ChangeDesignDialogState extends State<ChangeDesignDialog> {
  late final DeckSampleCache _cache = DeckSampleCache(widget.designer);
  DeckCatalog? _catalog;
  Object? _error;
  late DeckStyleChoice _style = widget.current;
  double _deckW = 12192000, _deckH = 6858000;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() => _error = null);
    try {
      final c = await widget.designer.deckCatalog();
      if (mounted) setState(() => _catalog = c);
    } catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  @override
  Widget build(BuildContext context) {
    final labels = widget.labels;
    final t = HarborTheme.of(context);
    final material = MaterialLocalizations.of(context);
    final catalog = _catalog;

    final changed = _style != widget.current;

    Widget content;
    if (_error != null) {
      content = HarborErrorState(
        title: labels.catalogFailed,
        message: labels.catalogFailed,
        technical: labels.errorText(_error!),
        retryLabel: labels.retry,
        onRetry: _load,
      );
    } else if (catalog == null) {
      content = HarborLoadingState(label: labels.catalogLoading);
    } else {
      final own = catalog.design(_style.design);
      content = Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(labels.currentDesign(widget.currentName),
              style: t.text.captionOf(t.colors.inkMuted)),
          const SizedBox(height: HarborSpace.s3),
          DeckPaletteStrip(
            palettes: catalog.palettes,
            selected: _style.palette,
            onSelected: (p) => setState(() => _style = p == null
                ? _style.copyWith(clearPalette: true)
                : _style.copyWith(palette: p)),
            ownColoursLabel: labels.paletteOwn,
            ownColoursSwatches: own?.swatches ?? const [],
            semanticsLabel: labels.paletteLabel,
            horizontal: true,
          ),
          const SizedBox(height: HarborSpace.s3),
          Expanded(
            child: DeckDesignGallery(
              designs: catalog.designs,
              selected: _style.design,
              onSelected: (id) =>
                  setState(() => _style = _style.copyWith(design: id)),
              thumbnailBuilder: (id) async {
                final scene = await _cache.sample(_style.copyWith(design: id));
                if (scene.width != _deckW || scene.height != _deckH) {
                  WidgetsBinding.instance.addPostFrameCallback((_) {
                    if (mounted) {
                      setState(() {
                        _deckW = scene.width;
                        _deckH = scene.height;
                      });
                    }
                  });
                }
                return scene.slides.isEmpty ? null : scene.slides.first;
              },
              thumbnailKey:
                  '${_style.palette ?? ''}|${_style.motion}|${_style.rtl}',
              deckWidth: _deckW,
              deckHeight: _deckH,
              minCardWidth: 150,
              darkLabel: labels.darkLabel,
              lightLabel: labels.lightLabel,
              thumbnailUnavailableLabel: labels.thumbUnavailable,
              semanticsLabel: labels.galleryLabel,
            ),
          ),
          Theme(
            data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
            child: ExpansionTile(
              key: const ValueKey('deck-motion-section'),
              tilePadding: EdgeInsets.zero,
              title: Text(
                  '${labels.motionHeading}: ${labels.motionNames[_style.motion] ?? _style.motion}',
                  style: t.text.bodyStrongOf(t.colors.ink)),
              children: [
                DeckMotionPicker(
                  motions: catalog.motions,
                  selected: _style.motion,
                  onSelected: (m) =>
                      setState(() => _style = _style.copyWith(motion: m)),
                  names: labels.motionNames,
                  descriptions: labels.motionDescriptions,
                  semanticsLabel: labels.motionLabel,
                ),
              ],
            ),
          ),
        ],
      );
    }

    final size = MediaQuery.sizeOf(context);
    final compact = size.width < 600;
    return Dialog(
      insetPadding: compact
          ? EdgeInsets.zero
          : const EdgeInsets.symmetric(
              horizontal: HarborSpace.s6, vertical: HarborSpace.s6),
      shape: compact
          ? const RoundedRectangleBorder()
          : RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(HarborRadius.lg)),
      child: ConstrainedBox(
        constraints: BoxConstraints(
          maxWidth: compact ? size.width : 760,
          maxHeight: compact ? size.height : 720,
          minHeight: compact ? size.height : 520,
        ),
        child: Padding(
          padding: const EdgeInsets.all(HarborSpace.s5),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Semantics(
                header: true,
                child: Text(labels.changeDesignTitle,
                    style: t.text.h2Of(t.colors.ink)),
              ),
              const SizedBox(height: HarborSpace.s3),
              Expanded(child: content),
              const SizedBox(height: HarborSpace.s3),
              Wrap(
                  alignment: WrapAlignment.end,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  spacing: HarborSpace.s2,
                  runSpacing: HarborSpace.s2,
                  children: [
                    TextButton(
                      onPressed: () => Navigator.of(context).pop(),
                      style: TextButton.styleFrom(
                          minimumSize: const Size(0, HarborLayout.touchTarget)),
                      child: Text(material.cancelButtonLabel),
                    ),
                    FilledButton(
                      onPressed: catalog != null && changed
                          ? () => Navigator.of(context).pop(_style)
                          : null,
                      style: FilledButton.styleFrom(
                          minimumSize: const Size(0, HarborLayout.touchTarget)),
                      child: Text(labels.applyDesign),
                    ),
                  ]),
            ],
          ),
        ),
      ),
    );
  }
}
