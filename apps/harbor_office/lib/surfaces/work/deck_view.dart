import 'package:flutter/material.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/deck_designer.dart';
import 'package:harbor_ui/harbor_ui.dart';
import 'package:harbor_ui/slides.dart';

import '../../l10n/app_localizations.dart';
import '../../services/harbor_service.dart';
import 'deck_designer_labels.dart';

/// Presentation workspace (UX-010): slide navigator (filmstrip) plus a
/// canvas. With a resolved scene (`preview['scene']`) the canvas and the
/// filmstrip paint the real slides, with a full-screen presenter and an
/// inline motion preview; without one they fall back to the slide titles and
/// bullets as text.
class DeckView extends StatefulWidget {
  const DeckView({super.key, required this.preview, this.designer});
  final Map preview;

  /// The core's deck designer; defaults to the app's [HarborService].
  /// Injectable for tests.
  final DeckDesignerApi? designer;

  @override
  State<DeckView> createState() => _DeckViewState();
}

class _DeckViewState extends State<DeckView> {
  int _selected = 0;
  bool _saving = false;
  bool _restyling = false;
  String? _error;

  // The scene is parsed once per scene map (by identity), not per rebuild.
  Object? _sceneSource;
  DeckScene? _scene;
  bool _previewing = false;
  final GlobalKey _selectedThumbKey = GlobalKey();

  @override
  void initState() {
    super.initState();
    _syncScene();
  }

  @override
  void didUpdateWidget(DeckView old) {
    super.didUpdateWidget(old);
    _syncScene();
  }

  void _syncScene() {
    final raw = widget.preview['scene'];
    if (identical(raw, _sceneSource)) return;
    _sceneSource = raw;
    _previewing = false;
    _scene = _parseScene(raw);
  }

  static DeckScene? _parseScene(Object? raw) {
    if (raw is! Map) return null;
    try {
      final scene = DeckScene.fromJson(raw is Map<String, dynamic>
          ? raw
          : raw.map((k, v) => MapEntry(k.toString(), v)));
      return scene.slides.isEmpty ? null : scene;
    } catch (_) {
      // A scene the painter cannot read: the text view still works.
      return null;
    }
  }

  /// Long-press edit of the selected slide's title or bullets through
  /// the package-preserving core op.
  Future<void> _editSlide(bool isTitle) async {
    if (_saving) return;
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null || sp.failed) return;
    final l10n = AppLocalizations.of(context)!;
    final slides = (widget.preview['slides'] as List? ?? const []).cast<Map>();
    if (slides.isEmpty) return;
    final index = _selected.clamp(0, slides.length - 1);
    final slide = slides[index];
    final slideNo = (slide['index'] as num).toInt();
    final current = isTitle
        ? (slide['title'] as String? ?? '')
        : (slide['bullets'] as List? ?? const []).cast<String>().join('\n');
    // The dialog owns (and disposes) its controller, so it is never torn
    // down while the route's exit animation still paints the TextField.
    final newText = await showDialog<String>(
      context: context,
      builder: (context) => _SlideTextDialog(
        title: isTitle
            ? l10n.workEditSlideTitle(slideNo)
            : l10n.workEditSlideBullets(slideNo),
        initial: current,
        singleLine: isTitle,
      ),
    );
    if (newText == null || !mounted || newText == current) return;
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await service.editDeckSlide(
          slide: slideNo, isTitle: isTitle, text: newText);
    } on ffi.HarborCoreException catch (e) {
      if (mounted) setState(() => _error = e.message);
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  /// How the deck was designed (`preview.design`); null for a deck that
  /// does not carry Harbor's design source.
  Map? get _design {
    final d = widget.preview['design'];
    return d is Map ? d : null;
  }

  bool get _restylable => _design?['restylable'] == true;

  DeckDesignerApi? _designerOf(BuildContext context) {
    if (widget.designer != null) return widget.designer;
    final sp = HarborServiceProvider.of(context);
    return sp.failed ? null : sp.notifier;
  }

  /// Pick a new design / palette / motion and apply it to the open deck.
  /// The result is a new UNSAVED working copy (the source file is never
  /// written); the header's Save a copy is the way to keep it.
  Future<void> _changeDesign() async {
    if (_restyling || _saving || !_restylable) return;
    final designer = _designerOf(context);
    final d = _design;
    if (designer == null || d == null) return;
    final l10n = AppLocalizations.of(context)!;
    final messenger = ScaffoldMessenger.maybeOf(context);
    final palette = d['palette'];
    final current = DeckStyleChoice(
      design: d['theme'] as String? ?? '',
      palette: palette is String && palette.isNotEmpty ? palette : null,
      motion: d['motion'] as String? ?? 'subtle',
      rtl: d['rtl'] == true,
    );
    final chosen = await ChangeDesignDialog.show(
      context,
      designer: designer,
      labels: deckDesignerLabels(l10n),
      current: current,
      currentName: d['theme_name'] as String? ?? current.design,
    );
    if (chosen == null || !mounted) return;
    setState(() {
      _restyling = true;
      _error = null;
    });
    try {
      await designer.restyleOpenDeck(chosen);
      messenger?.showSnackBar(SnackBar(content: Text(l10n.deckDesignApplied)));
    } catch (e) {
      if (mounted) {
        setState(
            () => _error = '${l10n.deckDesignFailed}: ${deckErrorText(e)}');
      }
    } finally {
      if (mounted) setState(() => _restyling = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final scene = _scene;
    return scene == null ? _buildText(context) : _buildScene(context, scene);
  }

  void _select(int i) {
    setState(() {
      _selected = i;
      _previewing = false;
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final ctx = _selectedThumbKey.currentContext;
      if (ctx != null && ctx.mounted) {
        Scrollable.ensureVisible(ctx,
            alignment: 0.5, duration: const Duration(milliseconds: 150));
      }
    });
  }

  PresentStrings _presentStrings(AppLocalizations l10n) => PresentStrings(
        slideOf: (i, n) => l10n.presentSlideOf(i, n),
        exit: l10n.presentExit,
        next: l10n.presentNext,
        previous: l10n.presentPrevious,
        showNotes: l10n.presentShowNotes,
        hideNotes: l10n.presentHideNotes,
        noNotes: l10n.presentNoNotes,
        elapsed: (time) => l10n.presentElapsed(time),
        blackScreen: l10n.presentBlackScreen,
        whiteScreen: l10n.presentWhiteScreen,
        progress: l10n.presentProgress,
      );

  void _present(DeckScene scene, int startIndex) {
    PresentView.show(
      context,
      deck: scene,
      startIndex: startIndex,
      strings: _presentStrings(AppLocalizations.of(context)!),
    );
  }

  /// Real-slide workspace: big canvas + thumbnail filmstrip.
  Widget _buildScene(BuildContext context, DeckScene scene) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final wc = HarborBreakpoints.of(context);
    final gutter = HarborBreakpoints.gutter(wc);
    final slides = scene.slides;
    final index = _selected.clamp(0, slides.length - 1);
    final slide = slides[index];
    final ar = scene.width / scene.height;
    final reduce = MediaQuery.disableAnimationsOf(context);
    final hasMotion = slide.transition != null || slide.animations.isNotEmpty;

    Widget thumb(int i, {required bool strip}) {
      final s = slides[i];
      final selected = i == index;
      final image = ClipRRect(
        borderRadius: BorderRadius.circular(3),
        child: SlideThumbnail(
          scene: s,
          deckWidth: scene.width,
          deckHeight: scene.height,
        ),
      );
      return Semantics(
        button: true,
        selected: selected,
        label: l10n.workSlide(s.index),
        child: ExcludeSemantics(
          child: HarborCard(
            key: selected ? _selectedThumbKey : null,
            selected: selected,
            padding: const EdgeInsets.all(HarborSpace.s2),
            radius: HarborRadius.sm,
            onTap: () => _select(i),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              Text('${s.index}',
                  style: t.text.monoOf(
                      selected ? t.colors.brand : t.colors.inkMuted,
                      size: 11,
                      weight: 600)),
              const SizedBox(width: HarborSpace.s2),
              if (strip)
                SizedBox(height: 64, width: 64 * ar, child: image)
              else
                Expanded(child: AspectRatio(aspectRatio: ar, child: image)),
            ]),
          ),
        ),
      );
    }

    final unsupported = slide.unsupported;
    final canvas = Padding(
      padding: EdgeInsets.all(gutter),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 960),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (_error != null)
                Padding(
                  padding: const EdgeInsets.only(bottom: HarborSpace.s3),
                  child: Text(_error!,
                      style: t.text.captionOf(t.colors.statusDangerText)),
                ),
              Padding(
                padding: const EdgeInsets.only(bottom: HarborSpace.s3),
                child: Wrap(
                  spacing: HarborSpace.s2,
                  runSpacing: HarborSpace.s2,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: [
                    FilledButton.icon(
                      onPressed: () => _present(scene, index),
                      icon: const Icon(Icons.slideshow_outlined),
                      label: Text(l10n.deckPresent),
                    ),
                    if (index != 0)
                      TextButton.icon(
                        onPressed: () => _present(scene, 0),
                        icon: const Icon(Icons.first_page),
                        label: Text(l10n.deckPresentFromStart),
                      ),
                    if (hasMotion)
                      TextButton.icon(
                        onPressed: reduce
                            ? null
                            : () => setState(() => _previewing = !_previewing),
                        icon: Icon(_previewing
                            ? Icons.stop_circle_outlined
                            : Icons.play_circle_outline),
                        label: Text(_previewing
                            ? l10n.deckStopPreview
                            : l10n.deckPreviewMotion),
                      ),
                    Tooltip(
                      message: _restylable
                          ? l10n.deckChangeDesignTitle
                          : l10n.deckChangeDesignDisabled,
                      child: TextButton.icon(
                        key: const ValueKey('deck-change-design'),
                        onPressed: _restylable && !_restyling && !_saving
                            ? _changeDesign
                            : null,
                        icon: _restyling
                            ? const SizedBox(
                                width: 16,
                                height: 16,
                                child:
                                    CircularProgressIndicator(strokeWidth: 2))
                            : const Icon(Icons.palette_outlined),
                        label: Text(l10n.deckChangeDesign),
                      ),
                    ),
                    IconButton(
                      tooltip: l10n.workEditSlideTitle(slide.index),
                      onPressed: _saving ? null : () => _editSlide(true),
                      icon: const Icon(Icons.title),
                    ),
                    IconButton(
                      tooltip: l10n.workEditSlideBullets(slide.index),
                      onPressed: _saving ? null : () => _editSlide(false),
                      icon: const Icon(Icons.format_list_bulleted),
                    ),
                  ],
                ),
              ),
              // Long-press edits the title (top of the slide) or the
              // bullets (the rest), as on the text canvas.
              LayoutBuilder(builder: (context, c) {
                final h = c.maxWidth / ar;
                return GestureDetector(
                  behavior: HitTestBehavior.opaque,
                  onLongPressStart: _saving
                      ? null
                      : (d) => _editSlide(d.localPosition.dy < h * 0.35),
                  child: DecoratedBox(
                    decoration: ShapeDecoration(
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(HarborRadius.md),
                        side: BorderSide(color: t.colors.border),
                      ),
                      shadows: [
                        BoxShadow(
                          color: t.colors.ink
                              .withValues(alpha: t.isDark ? 0 : 0.06),
                          blurRadius: 24,
                          offset: const Offset(0, 8),
                        ),
                      ],
                    ),
                    child: ClipRRect(
                      borderRadius: BorderRadius.circular(HarborRadius.md),
                      child: _previewing
                          ? _MotionPreview(
                              key: ValueKey('motion-$index'),
                              deck: scene,
                              index: index,
                              onDone: () {
                                if (mounted) {
                                  setState(() => _previewing = false);
                                }
                              },
                            )
                          : SlideView(
                              scene: slide,
                              deckWidth: scene.width,
                              deckHeight: scene.height,
                              semanticsLabel: l10n.workSlide(slide.index),
                            ),
                    ),
                  ),
                );
              }),
              if (unsupported.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.only(top: HarborSpace.s3),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Icon(Icons.info_outline,
                          size: 16, color: t.colors.inkMuted),
                      const SizedBox(width: HarborSpace.s2),
                      Expanded(
                        child: Text(
                          l10n.deckUnsupportedNote(unsupported.join(', ')),
                          style: t.text.captionOf(t.colors.inkMuted),
                        ),
                      ),
                    ],
                  ),
                ),
              const SizedBox(height: HarborSpace.s3),
              Row(children: [
                IconButton(
                  tooltip:
                      l10n.workSlide(slides[index == 0 ? 0 : index - 1].index),
                  onPressed: index == 0 ? null : () => _select(index - 1),
                  icon: const Icon(Icons.chevron_left),
                ),
                Expanded(
                  child: Text(
                    '${l10n.workSlide(slide.index)} · ${l10n.workSlides(slides.length)}',
                    textAlign: TextAlign.center,
                    style: t.text.captionOf(t.colors.inkMuted),
                  ),
                ),
                IconButton(
                  tooltip: l10n.workSlide(
                      slides[index >= slides.length - 1 ? index : index + 1]
                          .index),
                  onPressed: index >= slides.length - 1
                      ? null
                      : () => _select(index + 1),
                  icon: const Icon(Icons.chevron_right),
                ),
              ]),
            ],
          ),
        ),
      ),
    );

    return LayoutBuilder(builder: (context, constraints) {
      final wide = constraints.maxWidth >= HarborBreakpoints.twoColumnMinCanvas;
      if (wide) {
        return Row(children: [
          SizedBox(
            width: 220,
            child: Material(
              color: t.colors.surfaceRaised,
              child: Semantics(
                container: true,
                label: l10n.deckFilmstrip,
                child: ListView.separated(
                  padding: const EdgeInsets.all(HarborSpace.s3),
                  itemCount: slides.length,
                  separatorBuilder: (_, __) =>
                      const SizedBox(height: HarborSpace.s2),
                  itemBuilder: (context, i) => thumb(i, strip: false),
                ),
              ),
            ),
          ),
          VerticalDivider(width: 1, color: t.colors.border),
          Expanded(child: SingleChildScrollView(child: canvas)),
        ]);
      }
      return Column(children: [
        Expanded(child: SingleChildScrollView(child: canvas)),
        Container(
          height: 104,
          decoration: BoxDecoration(
            color: t.colors.surfaceRaised,
            border: Border(top: BorderSide(color: t.colors.border)),
          ),
          child: Semantics(
            container: true,
            label: l10n.deckFilmstrip,
            child: ListView.separated(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.all(HarborSpace.s3),
              itemCount: slides.length,
              separatorBuilder: (_, __) =>
                  const SizedBox(width: HarborSpace.s2),
              itemBuilder: (context, i) => thumb(i, strip: true),
            ),
          ),
        ),
      ]);
    });
  }

  Widget _buildText(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final slides = (widget.preview['slides'] as List? ?? const []).cast<Map>();
    if (slides.isEmpty) {
      return HarborEmptyState(
          icon: Icons.slideshow_outlined,
          title: l10n.workKindDeck,
          body: l10n.workSlides(0));
    }
    final index = _selected.clamp(0, slides.length - 1);
    final slide = slides[index];
    final wc = HarborBreakpoints.of(context);
    final gutter = HarborBreakpoints.gutter(wc);

    Widget thumb(int i) {
      final s = slides[i];
      final selected = i == index;
      return Semantics(
        button: true,
        selected: selected,
        label: l10n.workSlide((s['index'] as num).toInt()),
        child: ExcludeSemantics(
          child: HarborCard(
            selected: selected,
            padding: const EdgeInsets.all(HarborSpace.s2),
            radius: HarborRadius.sm,
            onTap: () => setState(() => _selected = i),
            child: Row(children: [
              Text('${s['index']}',
                  style: t.text.monoOf(
                      selected ? t.colors.brand : t.colors.inkMuted,
                      size: 11,
                      weight: 600)),
              const SizedBox(width: HarborSpace.s2),
              Expanded(
                child: AspectRatio(
                  aspectRatio: 16 / 9,
                  child: Container(
                    padding: const EdgeInsets.all(HarborSpace.s1 + 2),
                    decoration: BoxDecoration(
                      color: t.colors.surfaceRaised,
                      borderRadius: BorderRadius.circular(4),
                      border: Border.all(color: t.colors.borderSubtle),
                    ),
                    child: Text(s['title'] as String? ?? '',
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: t.text.captionOf(t.colors.ink)),
                  ),
                ),
              ),
            ]),
          ),
        ),
      );
    }

    final canvas = Padding(
      padding: EdgeInsets.all(gutter),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 960),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (_error != null)
                Padding(
                  padding: const EdgeInsets.only(bottom: HarborSpace.s3),
                  child: Text(_error!,
                      style: t.text.captionOf(t.colors.statusDangerText)),
                ),
              AspectRatio(
                aspectRatio: 16 / 9,
                child: Container(
                  padding: EdgeInsets.all(HarborBreakpoints.isCompact(wc)
                      ? HarborSpace.s5
                      : HarborSpace.s10),
                  decoration: ShapeDecoration(
                    color: t.colors.surface,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(HarborRadius.md),
                      side: BorderSide(color: t.colors.border),
                    ),
                    shadows: [
                      BoxShadow(
                        color:
                            t.colors.ink.withValues(alpha: t.isDark ? 0 : 0.06),
                        blurRadius: 24,
                        offset: const Offset(0, 8),
                      ),
                    ],
                  ),
                  child: SingleChildScrollView(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        GestureDetector(
                          onLongPress: _saving ? null : () => _editSlide(true),
                          child: SelectableText(slide['title'] as String? ?? '',
                              style: t.text.titleOf(t.colors.ink)),
                        ),
                        const SizedBox(height: HarborSpace.s4),
                        GestureDetector(
                          onLongPress: _saving ? null : () => _editSlide(false),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              for (final b
                                  in (slide['bullets'] as List? ?? const [])
                                      .cast<String>())
                                Padding(
                                  padding: const EdgeInsets.only(
                                      bottom: HarborSpace.s2),
                                  child: Row(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      Padding(
                                        padding:
                                            const EdgeInsetsDirectional.only(
                                                end: HarborSpace.s3, top: 9),
                                        child: Container(
                                          width: 6,
                                          height: 6,
                                          decoration: BoxDecoration(
                                              color: t.colors.brand,
                                              shape: BoxShape.circle),
                                        ),
                                      ),
                                      Expanded(
                                        child: SelectableText(b,
                                            style: t.text
                                                .bodyOf(t.colors.ink)
                                                .copyWith(fontSize: 15)),
                                      ),
                                    ],
                                  ),
                                ),
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
              const SizedBox(height: HarborSpace.s3),
              Row(children: [
                IconButton(
                  tooltip: l10n.workSlide(index == 0 ? 1 : index),
                  onPressed: index == 0
                      ? null
                      : () => setState(() => _selected = index - 1),
                  icon: const Icon(Icons.chevron_left),
                ),
                Expanded(
                  child: Text(
                    '${l10n.workSlide((slide['index'] as num).toInt())} · ${l10n.workSlides(slides.length)}',
                    textAlign: TextAlign.center,
                    style: t.text.captionOf(t.colors.inkMuted),
                  ),
                ),
                IconButton(
                  tooltip: l10n.workSlide(
                      index + 2 > slides.length ? slides.length : index + 2),
                  onPressed: index >= slides.length - 1
                      ? null
                      : () => setState(() => _selected = index + 1),
                  icon: const Icon(Icons.chevron_right),
                ),
              ]),
            ],
          ),
        ),
      ),
    );

    return LayoutBuilder(builder: (context, constraints) {
      final wide = constraints.maxWidth >= HarborBreakpoints.twoColumnMinCanvas;
      if (wide) {
        return Row(children: [
          SizedBox(
            width: 220,
            child: Material(
              color: t.colors.surfaceRaised,
              child: ListView.separated(
                padding: const EdgeInsets.all(HarborSpace.s3),
                itemCount: slides.length,
                separatorBuilder: (_, __) =>
                    const SizedBox(height: HarborSpace.s2),
                itemBuilder: (context, i) => thumb(i),
              ),
            ),
          ),
          VerticalDivider(width: 1, color: t.colors.border),
          Expanded(child: SingleChildScrollView(child: canvas)),
        ]);
      }
      return Column(children: [
        Expanded(child: SingleChildScrollView(child: canvas)),
        Container(
          height: 92,
          decoration: BoxDecoration(
            color: t.colors.surfaceRaised,
            border: Border(top: BorderSide(color: t.colors.border)),
          ),
          child: ListView.separated(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.all(HarborSpace.s3),
            itemCount: slides.length,
            separatorBuilder: (_, __) => const SizedBox(width: HarborSpace.s2),
            itemBuilder: (context, i) => SizedBox(width: 168, child: thumb(i)),
          ),
        ),
      ]);
    });
  }
}

/// Text-entry dialog for one slide's title or bullets; pops the entered
/// text, or null on cancel.
class _SlideTextDialog extends StatefulWidget {
  const _SlideTextDialog({
    required this.title,
    required this.initial,
    required this.singleLine,
  });
  final String title;
  final String initial;
  final bool singleLine;

  @override
  State<_SlideTextDialog> createState() => _SlideTextDialogState();
}

class _SlideTextDialogState extends State<_SlideTextDialog> {
  late final TextEditingController _controller =
      TextEditingController(text: widget.initial);

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final material = MaterialLocalizations.of(context);
    return AlertDialog(
      title: Text(widget.title),
      content: TextField(
        controller: _controller,
        autofocus: true,
        maxLines: widget.singleLine ? 1 : 8,
        minLines: 1,
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(material.cancelButtonLabel),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop(_controller.text),
          child: Text(material.okButtonLabel),
        ),
      ],
    );
  }
}

/// Plays the selected slide's motion inline: its transition in (from the
/// previous slide, or from black for the first), then its animations. Click
/// steps are played one after another with a short pause, since nobody is
/// there to click.
class _MotionPreview extends StatefulWidget {
  const _MotionPreview({
    super.key,
    required this.deck,
    required this.index,
    required this.onDone,
  });
  final DeckScene deck;
  final int index;
  final VoidCallback onDone;

  @override
  State<_MotionPreview> createState() => _MotionPreviewState();
}

class _MotionPreviewState extends State<_MotionPreview>
    with SingleTickerProviderStateMixin {
  static const int _gapMs = 350;
  static const int _end = 1 << 30;
  static const SlideScene _black = SlideScene(
    index: 0,
    background: ScenePaint(kind: PaintKind.solid, c: '#000000', a: 1),
  );

  late final AnimationController _ctl;
  late final SlideScene _slide = widget.deck.slides[widget.index];
  late final SlidePlayer _player = SlidePlayer(_slide,
      deckWidth: widget.deck.width, deckHeight: widget.deck.height);
  late final SlideScene _from =
      widget.index > 0 ? widget.deck.slides[widget.index - 1] : _black;
  late final PlayState _fromState = widget.index > 0
      ? SlidePlayer(_from,
              deckWidth: widget.deck.width, deckHeight: widget.deck.height)
          .stateAt(_end, _end)
      : PlayState.empty;
  late final PlayState _toState = _player.stateAt(-1, 0);
  late final int _transitionMs = _slide.transition?.durMs ?? 0;
  late final int _total = _transitionMs + _stepsMs();

  int _stepsMs() {
    var ms = 0;
    for (var s = 0; s <= _player.steps; s++) {
      final d = _player.durationMs(s);
      if (d > 0) ms += d + (s > 0 ? _gapMs : 0);
    }
    return ms;
  }

  @override
  void initState() {
    super.initState();
    _ctl = AnimationController(
      vsync: this,
      duration: Duration(milliseconds: _total < 1 ? 1 : _total),
    )..addStatusListener((s) {
        if (s == AnimationStatus.completed) widget.onDone();
      });
    _ctl.forward();
  }

  @override
  void dispose() {
    _ctl.dispose();
    super.dispose();
  }

  /// (step, ms into the step) for [t] ms after the transition.
  (int, int) _locate(int t) {
    var at = 0;
    for (var s = 0; s <= _player.steps; s++) {
      final d = _player.durationMs(s);
      if (d <= 0) continue;
      final gap = s > 0 ? _gapMs : 0;
      if (t < at + gap) return (s - 1, _end); // pause: previous end state
      at += gap;
      if (t < at + d) return (s, t - at);
      at += d;
    }
    return (_player.steps, _end);
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _ctl,
      builder: (context, _) {
        final ms = (_ctl.value * _total).round();
        if (ms < _transitionMs) {
          return TransitionView(
            from: _from,
            to: _slide,
            deckWidth: widget.deck.width,
            deckHeight: widget.deck.height,
            transition: _slide.transition,
            progress: Curves.easeInOut.transform(ms / _transitionMs),
            fromState: _fromState,
            toState: _toState,
          );
        }
        final (step, t) = _locate(ms - _transitionMs);
        final st = _player.stateAt(step, t);
        return SlideView(
          scene: _slide,
          deckWidth: widget.deck.width,
          deckHeight: widget.deck.height,
          effects: st.effects,
          hidden: st.hidden,
          hiddenParas: st.hiddenParas,
        );
      },
    );
  }
}
