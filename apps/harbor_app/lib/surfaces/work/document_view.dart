import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../../l10n/app_localizations.dart';

/// Document workspace (UX-008): paragraphs rendered with typographic
/// hierarchy from their DOCX styles, a page-like reading column and an
/// outline of headings (side panel on wide canvases, sheet on compact).
class DocumentView extends StatefulWidget {
  const DocumentView({super.key, required this.preview});
  final Map preview;

  @override
  State<DocumentView> createState() => _DocumentViewState();
}

class _DocumentViewState extends State<DocumentView> {
  final _scroll = ScrollController();
  final Map<int, GlobalKey> _anchors = {};

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  static int? headingLevel(String? style) {
    if (style == null) return null;
    final s = style.toLowerCase().replaceAll(' ', '');
    if (s == 'title') return 0;
    final m = RegExp(r'^heading(\d)$').firstMatch(s);
    if (m != null) return int.parse(m.group(1)!);
    return null;
  }

  static bool isList(String? style) {
    if (style == null) return false;
    final s = style.toLowerCase();
    return s.contains('list') || s.contains('bullet');
  }

  void _jumpTo(int index) {
    final key = _anchors[index];
    final ctx = key?.currentContext;
    if (ctx == null) return;
    final motion = HarborMotion.of(context);
    Scrollable.ensureVisible(ctx,
        duration: motion.slow, curve: HarborMotion.easing, alignment: 0.05);
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final paras =
        (widget.preview['paragraphs'] as List? ?? const []).cast<Map>();
    final headings = [
      for (final p in paras)
        if (headingLevel(p['style'] as String?) != null &&
            (p['text'] as String? ?? '').trim().isNotEmpty)
          p,
    ];
    final wc = HarborBreakpoints.of(context);
    final gutter = HarborBreakpoints.gutter(wc);

    final page = Scrollbar(
      controller: _scroll,
      child: SingleChildScrollView(
        controller: _scroll,
        padding: EdgeInsets.fromLTRB(
            gutter, HarborSpace.s2, gutter, HarborSpace.s10),
        child: Center(
          child: ConstrainedBox(
            constraints:
                const BoxConstraints(maxWidth: HarborLayout.readingMax),
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
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final p in paras)
                    _Paragraph(
                      key: _anchors.putIfAbsent(
                          (p['index'] as num).toInt(), GlobalKey.new),
                      paragraph: p,
                    ),
                ],
              ),
            ),
          ),
        ),
      ),
    );

    return LayoutBuilder(builder: (context, constraints) {
      final wide = constraints.maxWidth >= HarborBreakpoints.twoColumnMinCanvas;
      if (!wide || headings.isEmpty) return page;
      return Row(children: [
        Expanded(child: page),
        VerticalDivider(width: 1, color: t.colors.border),
        SizedBox(
          width: 260,
          child: _Outline(
            headings: headings,
            onTap: (p) => _jumpTo((p['index'] as num).toInt()),
            title: l10n.workOutline,
          ),
        ),
      ]);
    });
  }
}

class _Outline extends StatelessWidget {
  const _Outline(
      {required this.headings, required this.onTap, required this.title});
  final List<Map> headings;
  final ValueChanged<Map> onTap;
  final String title;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Material(
      color: t.colors.surfaceRaised,
      child: ListView(
        padding: const EdgeInsets.all(HarborSpace.s3),
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(
                HarborSpace.s3, HarborSpace.s2, HarborSpace.s3, HarborSpace.s3),
            child: Text(title.toUpperCase(),
                style: t.text
                    .captionOf(t.colors.inkMuted)
                    .copyWith(letterSpacing: 0.6)),
          ),
          for (final h in headings)
            HarborListRow(
              dense: true,
              title: Padding(
                padding: EdgeInsetsDirectional.only(
                    start: HarborSpace.s3 *
                        ((_DocumentViewState.headingLevel(
                                        h['style'] as String?) ??
                                    1)
                                .clamp(1, 4) -
                            1)),
                child: Text(h['text'] as String,
                    maxLines: 2, overflow: TextOverflow.ellipsis),
              ),
              onTap: () => onTap(h),
            ),
        ],
      ),
    );
  }
}

class _Paragraph extends StatelessWidget {
  const _Paragraph({super.key, required this.paragraph});
  final Map paragraph;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final text = (paragraph['text'] as String? ?? '');
    final style = paragraph['style'] as String?;
    final level = _DocumentViewState.headingLevel(style);
    final TextStyle ts;
    EdgeInsets padding;
    if (level == 0) {
      ts = t.text
          .displayOf(t.colors.ink)
          .copyWith(fontSize: 28, height: 34 / 28);
      padding = const EdgeInsets.only(bottom: HarborSpace.s5);
    } else if (level == 1) {
      ts = t.text.titleOf(t.colors.ink);
      padding =
          const EdgeInsets.only(top: HarborSpace.s5, bottom: HarborSpace.s3);
    } else if (level == 2) {
      ts = t.text.h2Of(t.colors.ink);
      padding =
          const EdgeInsets.only(top: HarborSpace.s4, bottom: HarborSpace.s2);
    } else if (level != null) {
      ts = t.text.bodyStrongOf(t.colors.ink);
      padding =
          const EdgeInsets.only(top: HarborSpace.s3, bottom: HarborSpace.s1);
    } else {
      ts = t.text.bodyOf(t.colors.ink).copyWith(fontSize: 15, height: 1.6);
      padding = const EdgeInsets.only(bottom: HarborSpace.s3);
    }
    if (text.trim().isEmpty) return const SizedBox(height: HarborSpace.s3);
    final body = SelectableText(text, style: ts);
    if (_DocumentViewState.isList(style) && level == null) {
      return Padding(
        padding: padding,
        child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Padding(
            padding: const EdgeInsetsDirectional.only(
                start: HarborSpace.s2, end: HarborSpace.s3, top: 9),
            child: Container(
              width: 5,
              height: 5,
              decoration: BoxDecoration(
                  color: t.colors.inkMuted, shape: BoxShape.circle),
            ),
          ),
          Expanded(child: body),
        ]),
      );
    }
    return Padding(padding: padding, child: body);
  }
}
