import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../../l10n/app_localizations.dart';
import '../../services/file_passages.dart';
import 'work_focus.dart';

/// PDF/research workspace (UX-011): one card per page with the extracted
/// text (selectable), page badges and a page list on wide canvases.
class PdfView extends StatefulWidget {
  const PdfView({super.key, required this.preview, this.focus});
  final Map preview;

  /// A cited page to scroll to and mark.
  final WorkFocusListenable? focus;

  @override
  State<PdfView> createState() => _PdfViewState();
}

class _PdfViewState extends State<PdfView> {
  final _scroll = ScrollController();
  final Map<int, GlobalKey> _anchors = {};
  int? _marked;

  @override
  void initState() {
    super.initState();
    widget.focus?.addListener(_onFocus);
  }

  @override
  void didUpdateWidget(PdfView old) {
    super.didUpdateWidget(old);
    if (!identical(old.focus, widget.focus)) {
      old.focus?.removeListener(_onFocus);
      widget.focus?.addListener(_onFocus);
    }
  }

  @override
  void dispose() {
    widget.focus?.removeListener(_onFocus);
    _scroll.dispose();
    super.dispose();
  }

  void _onFocus() {
    final target = widget.focus?.value?.target;
    if (target == null || target.kind != PassageKind.page) return;
    setState(() => _marked = target.index);
    _jumpTo(target.index);
  }

  void _jumpTo(int index) {
    final ctx = _anchors[index]?.currentContext;
    final motion = HarborMotion.of(context);
    if (ctx != null) {
      Scrollable.ensureVisible(ctx,
          duration: motion.slow, curve: HarborMotion.easing, alignment: 0.02);
      return;
    }
    // The list builds lazily: a page far off screen has no context yet.
    // Jump to its estimated position, then align once it is built.
    if (!_scroll.hasClients) return;
    final pages = (widget.preview['pages'] as List? ?? const []).cast<Map>();
    final at = pages.indexWhere((p) => (p['index'] as num?)?.toInt() == index);
    if (at < 0 || pages.isEmpty) return;
    final max = _scroll.position.maxScrollExtent;
    _scroll.jumpTo((max * at / pages.length).clamp(0.0, max));
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final later = _anchors[index]?.currentContext;
      if (later == null || !mounted) return;
      Scrollable.ensureVisible(later, alignment: 0.02);
    });
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final pages = (widget.preview['pages'] as List? ?? const []).cast<Map>();
    final wc = HarborBreakpoints.of(context);
    final gutter = HarborBreakpoints.gutter(wc);

    final list = Scrollbar(
      controller: _scroll,
      child: ListView.builder(
        controller: _scroll,
        padding: EdgeInsets.fromLTRB(
            gutter, HarborSpace.s2, gutter, HarborSpace.s10),
        itemCount: pages.length,
        itemBuilder: (context, i) {
          final pg = pages[i];
          final index = (pg['index'] as num).toInt();
          final text = (pg['text'] as String? ?? '').trim();
          return Center(
            child: ConstrainedBox(
              constraints:
                  const BoxConstraints(maxWidth: HarborLayout.readingMax),
              child: Padding(
                key: _anchors.putIfAbsent(index, GlobalKey.new),
                padding: const EdgeInsets.only(bottom: HarborSpace.s4),
                child: HarborCard(
                  key: _marked == index
                      ? const ValueKey('work-marked-page')
                      : null,
                  selected: _marked == index,
                  padding: EdgeInsets.all(HarborBreakpoints.isCompact(wc)
                      ? HarborSpace.s4
                      : HarborSpace.s6),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      HarborPill(l10n.workPage(index),
                          icon: Icons.article_outlined),
                      const SizedBox(height: HarborSpace.s3),
                      if (text.isEmpty)
                        Text(l10n.workEmptyTextPage,
                            style: t.text.smallOf(t.colors.inkMuted))
                      else
                        SelectableText(text,
                            style: t.text
                                .bodyOf(t.colors.ink)
                                .copyWith(fontSize: 15, height: 1.6)),
                    ],
                  ),
                ),
              ),
            ),
          );
        },
      ),
    );

    return LayoutBuilder(builder: (context, constraints) {
      final wide = constraints.maxWidth >= HarborBreakpoints.twoColumnMinCanvas;
      if (!wide || pages.length < 2) return list;
      return Row(children: [
        Expanded(child: list),
        VerticalDivider(width: 1, color: t.colors.border),
        SizedBox(
          width: 200,
          child: Material(
            color: t.colors.surfaceRaised,
            child: ListView(
              padding: const EdgeInsets.all(HarborSpace.s3),
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(HarborSpace.s3,
                      HarborSpace.s2, HarborSpace.s3, HarborSpace.s3),
                  child: Text(l10n.workPages(pages.length).toUpperCase(),
                      style: t.text
                          .captionOf(t.colors.inkMuted)
                          .copyWith(letterSpacing: 0.6)),
                ),
                for (final pg in pages)
                  HarborListRow(
                    dense: true,
                    leading: const Icon(Icons.article_outlined, size: 16),
                    title: Text(l10n.workPage((pg['index'] as num).toInt())),
                    onTap: () => _jumpTo((pg['index'] as num).toInt()),
                  ),
              ],
            ),
          ),
        ),
      ]);
    });
  }
}
