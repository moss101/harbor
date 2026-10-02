import 'package:flutter/material.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

import '../../l10n/app_localizations.dart';
import '../../services/harbor_service.dart';

/// Presentation workspace (UX-010): slide navigator (filmstrip) plus a
/// 16:9 canvas showing the selected slide's title and bullets.
class DeckView extends StatefulWidget {
  const DeckView({super.key, required this.preview});
  final Map preview;

  @override
  State<DeckView> createState() => _DeckViewState();
}

class _DeckViewState extends State<DeckView> {
  int _selected = 0;
  bool _saving = false;
  String? _error;

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

  @override
  Widget build(BuildContext context) {
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
                              for (final b in (slide['bullets'] as List? ?? const [])
                                  .cast<String>())
                                Padding(
                                  padding: const EdgeInsets.only(
                                      bottom: HarborSpace.s2),
                                  child: Row(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      Padding(
                                        padding: const EdgeInsetsDirectional.only(
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
