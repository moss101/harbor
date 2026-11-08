import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/auth_service.dart';
import '../services/file_passages.dart';
import 'report_sheet.dart';
import 'save_pdf.dart';

/// One question and what came back for it: a grounded answer, retrieval-
/// only evidence, an error or a cancellation. Shared by the Home
/// conversation and the Work command bar so a grounded answer reads the
/// same everywhere (goal §12).
class AnswerTurn {
  AnswerTurn(this.question, {this.runId, this.collection});
  final String question;
  final String? runId;

  /// The Library collection the answer was restricted to, if any.
  final String? collection;
  Map<String, dynamic>? answer;
  Map<String, dynamic>? evidence;
  String? error;
  bool cancelled = false;
  bool busy = true;
}

/// Turns a citation into the short label it is shown under (e.g. "Page 3"
/// for a passage of the open file). Null keeps the citation's own title.
typedef CitationLabeler = String? Function(Map citation);

/// The label of a citation of ONE file (an answer from `askAboutFile`):
/// "Paragraph 12", "Page 3", "Slide 2", "Cell B4" from its `passage_id`;
/// the core's `locator` when the id is not one Harbor issued; null (keep
/// the title) for a Knowledge citation.
String? fileCitationLabel(Map citation, AppLocalizations l10n) {
  final target = PassageTarget.parse(citation['passage_id'] as String?);
  if (target == null) return citation['locator'] as String?;
  return switch (target.kind) {
    PassageKind.paragraph => l10n.workParagraph(target.index),
    PassageKind.page => l10n.workPage(target.index),
    PassageKind.slide => l10n.workSlide(target.index),
    PassageKind.cell => l10n.workCell(target.cell ?? ''),
  };
}

/// One turn of the answer thread: the question as a heading, then what
/// came back for it (sources, the answer and its actions, or a notice).
class AnswerTurnView extends StatelessWidget {
  const AnswerTurnView({
    super.key,
    required this.turn,
    required this.progress,
    required this.onCancel,
    this.onCitationTap,
    this.citationLabel,
  });
  final AnswerTurn turn;
  final Map<String, dynamic>? progress;
  final VoidCallback onCancel;
  final ValueChanged<Map>? onCitationTap;
  final CitationLabeler? citationLabel;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: HarborSpace.s4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          QuestionHeading(text: turn.question),
          if (turn.collection != null) ...[
            const SizedBox(height: HarborSpace.s2),
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: HarborPill(
                // Collection names are data.
                l10n.askGroundedInCollection(turn.collection!),
                key: const ValueKey('answer-collection-scope'),
                icon: Icons.folder_outlined,
              ),
            ),
          ],
          const SizedBox(height: HarborSpace.s4),
          if (turn.busy)
            GeneratingCard(progress: progress, onCancel: onCancel)
          else if (turn.cancelled)
            HarborBanner(
              tone: HarborBannerTone.warning,
              title: l10n.askCancelledTitle,
              body: l10n.askCancelledBody,
            )
          else if (turn.error != null)
            HarborBanner(
              tone: HarborBannerTone.danger,
              title: l10n.askErrorTitle,
              body: turn.error,
            )
          else if (turn.answer != null)
            GroundedAnswerView(
              answer: turn.answer!,
              question: turn.question,
              onCitationTap: onCitationTap,
              citationLabel: citationLabel,
            )
          else if (turn.evidence != null)
            EvidenceView(
                citations: ((turn.evidence!['citations'] as List?) ?? const [])
                    .cast<Map>()),
        ],
      ),
    );
  }
}

/// What the person asked, as the heading of its turn (start-aligned, so
/// it reads like a page title in either direction).
class QuestionHeading extends StatelessWidget {
  const QuestionHeading({super.key, required this.text});
  final String text;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final compact = HarborBreakpoints.isCompact(HarborBreakpoints.of(context));
    final style = compact
        ? t.text.h2Of(t.colors.ink).copyWith(fontSize: 19, height: 1.35)
        : t.text.titleOf(t.colors.ink).copyWith(fontSize: 24, height: 1.3);
    return Semantics(
      header: true,
      child: SelectableText(text,
          key: const ValueKey('answer-question'), style: style),
    );
  }
}

/// Live token progress of a grounded generation, with a real cancel.
class GeneratingCard extends StatelessWidget {
  const GeneratingCard(
      {super.key, required this.progress, required this.onCancel});
  final Map<String, dynamic>? progress;
  final VoidCallback onCancel;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final tokens = (progress?['items_done'] as num?)?.toInt() ?? 0;
    return HarborOpProgress(
      icon: Icons.auto_awesome_outlined,
      title: l10n.askGenerating,
      progressLine: tokens > 0 ? l10n.askGenerationProgress(tokens) : null,
      cancelLabel: l10n.cancelAction,
      onCancel: onCancel,
    );
  }
}

/// A generated answer, read like a page: its sources first as compact
/// chips (the full list with quotes and scores opens from them), then the
/// answer with its grounding badge (or the abstention), then a quiet
/// action row — copy, Save as PDF, Report — and the execution facts.
class GroundedAnswerView extends StatelessWidget {
  const GroundedAnswerView({
    super.key,
    required this.answer,
    this.question,
    this.onCitationTap,
    this.citationLabel,
  });
  final Map<String, dynamic> answer;

  /// What was asked; offered (switchable) in the Report sheet.
  final String? question;
  final ValueChanged<Map>? onCitationTap;
  final CitationLabeler? citationLabel;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final answerText = (answer['answer'] as String?)?.trim() ?? '';
    final insufficient = answerText == 'INSUFFICIENT_EVIDENCE';
    final grounded = answer['used_citations'] == true && !insufficient;
    final citations = ((answer['citations'] as List?) ?? const []).cast<Map>();
    final showSources =
        answer['used_citations'] == true && citations.isNotEmpty;
    final hasText = answerText.isNotEmpty && !insufficient;
    return Column(
      key: const ValueKey('answer-view'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (showSources) ...[
          SourceChips(
            citations: citations,
            label: citationLabel,
            onTap: onCitationTap,
          ),
          const SizedBox(height: HarborSpace.s5),
        ],
        HarborAnswerSectionLabel(
          icon: Icons.auto_awesome_outlined,
          label: l10n.askAnswerHeading,
          trailing: StatusBadge(
            semantic:
                grounded ? ExecutionSemantic.local : ExecutionSemantic.hybrid,
            icon: grounded ? Icons.verified_outlined : Icons.help_outline,
            label: grounded ? l10n.askGroundedBadge : l10n.askUngroundedBadge,
          ),
        ),
        const SizedBox(height: HarborSpace.s3),
        if (insufficient)
          HarborBanner(
            tone: HarborBannerTone.warning,
            title: l10n.askAbstentionHeading,
            body: l10n.askInsufficientNote,
            dense: true,
          )
        else
          SelectableText(answerText,
              style: t.text
                  .bodyOf(t.colors.ink)
                  .copyWith(fontSize: 16, height: 1.6)),
        if (answer['truncated'] == true && !insufficient) ...[
          const SizedBox(height: HarborSpace.s2),
          Text(l10n.askTruncatedNote,
              key: const ValueKey('answer-truncated'),
              style: t.text.captionOf(t.colors.inkMuted)),
        ],
        const SizedBox(height: HarborSpace.s3),
        Wrap(
          spacing: HarborSpace.s1,
          runSpacing: HarborSpace.s1,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            if (hasText)
              IconButton(
                key: const ValueKey('answer-copy'),
                tooltip: l10n.answerCopy,
                iconSize: 18,
                visualDensity: VisualDensity.compact,
                icon: const Icon(Icons.copy_outlined),
                onPressed: () async {
                  await Clipboard.setData(ClipboardData(text: answerText));
                  if (!context.mounted) return;
                  ScaffoldMessenger.maybeOf(context)?.showSnackBar(
                      SnackBar(content: Text(l10n.answerCopied)));
                },
              ),
            if (hasText) SaveAnswerPdfButton(text: answerText),
            if (AuthScope.maybeOf(context) case final auth? when hasText)
              IconButton(
                key: const ValueKey('answer-report'),
                tooltip: l10n.reportAction,
                iconSize: 18,
                visualDensity: VisualDensity.compact,
                icon: const Icon(Icons.flag_outlined),
                onPressed: () => showReportSheet(
                  context,
                  auth: auth,
                  answer: answerText,
                  question: question,
                  modelId: answer['executed_on'] as String?,
                ),
              ),
            const SizedBox(width: HarborSpace.s2),
            StatusBadge(
              semantic: ExecutionSemantic.local,
              label: ((answer['execution'] as String?) ?? 'ON_DEVICE')
                  .replaceAll('_', ' '),
            ),
            Text(
              l10n.askExecutedOnLine(
                (answer['executed_on'] as String?) ?? '',
                ((answer['usage']?['completion_tokens'] as num?) ?? 0).toInt(),
              ),
              style: t.text.captionOf(t.colors.inkMuted),
            ),
          ],
        ),
      ],
    );
  }
}

/// An answer's sources as a row of numbered chips plus "All sources",
/// which opens the full list (quotes, scores). A chip jumps to its
/// passage when [onTap] is set (an answer about the open file), and
/// otherwise opens the full list too.
class SourceChips extends StatelessWidget {
  const SourceChips(
      {super.key, required this.citations, this.label, this.onTap});
  final List<Map> citations;
  final CitationLabeler? label;
  final ValueChanged<Map>? onTap;

  static String titleOf(Map c, String? label) =>
      label ??
      (c['title'] as String?) ??
      (c['locator'] as String?) ??
      (c['source_id'] as String?) ??
      '';

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    void showAll() => showSourcesSheet(context,
        citations: citations, label: label, onTap: onTap);
    return Column(
      key: const ValueKey('answer-sources'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        HarborAnswerSectionLabel(
          icon: Icons.format_list_bulleted,
          label: l10n.answerSources,
        ),
        const SizedBox(height: HarborSpace.s2),
        Wrap(
          spacing: HarborSpace.s2,
          runSpacing: HarborSpace.s2,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            for (final (i, c) in citations.indexed)
              HarborSourceChip(
                index: i + 1,
                label: titleOf(c, label?.call(c)),
                semanticLabel: onTap == null
                    ? l10n.answerSourceOpen(titleOf(c, label?.call(c)))
                    : l10n.workCitationJump(titleOf(c, label?.call(c))),
                onTap: onTap == null ? showAll : () => onTap!(c),
              ),
            TextButton.icon(
              key: const ValueKey('answer-sources-all'),
              onPressed: showAll,
              style: TextButton.styleFrom(
                  foregroundColor: t.colors.brand,
                  visualDensity: VisualDensity.compact),
              icon: const Icon(Icons.unfold_more, size: 16),
              label: Text(l10n.answerSourcesAll),
            ),
          ],
        ),
      ],
    );
  }
}

/// The full list of an answer's sources, as a sheet.
Future<void> showSourcesSheet(
  BuildContext context, {
  required List<Map> citations,
  CitationLabeler? label,
  ValueChanged<Map>? onTap,
}) {
  final l10n = AppLocalizations.of(context)!;
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    constraints:
        BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.85),
    builder: (sheetContext) => SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(
          HarborSpace.s4, HarborSpace.s2, HarborSpace.s4, HarborSpace.s5),
      child: CitationList(
        key: const ValueKey('answer-sources-sheet'),
        title: l10n.askCitationsHeading,
        citations: citations,
        label: label,
        onTap: onTap == null
            ? null
            : (c) {
                Navigator.of(sheetContext).pop();
                onTap(c);
              },
      ),
    ),
  );
}

/// Retrieval-only evidence (no generation), or an honest "nothing found".
class EvidenceView extends StatelessWidget {
  const EvidenceView({super.key, required this.citations});
  final List<Map> citations;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    if (citations.isEmpty) {
      return HarborBanner(
        tone: HarborBannerTone.info,
        icon: Icons.search_off_outlined,
        title: l10n.askNoEvidenceTitle,
        body: l10n.askNoEvidenceBody,
      );
    }
    return CitationList(
      title: l10n.askEvidenceHeading,
      note: l10n.askRetrievalOnlyNote,
      citations: citations,
    );
  }
}

class CitationList extends StatelessWidget {
  const CitationList({
    super.key,
    required this.title,
    required this.citations,
    this.note,
    this.onTap,
    this.label,
  });
  final String title;
  final String? note;
  final List<Map> citations;
  final ValueChanged<Map>? onTap;
  final CitationLabeler? label;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return HarborCard(
      padding: const EdgeInsets.all(HarborSpace.s3),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(
                HarborSpace.s1, 0, HarborSpace.s1, HarborSpace.s2),
            child: Row(children: [
              Icon(Icons.format_quote_outlined,
                  size: 16, color: t.colors.brand),
              const SizedBox(width: HarborSpace.s2),
              Expanded(
                  child: Text(title, style: t.text.bodyStrongOf(t.colors.ink))),
              if (note != null)
                Flexible(
                  child: Text(note!,
                      style: t.text.captionOf(t.colors.inkMuted),
                      textAlign: TextAlign.end),
                ),
            ]),
          ),
          for (final (i, c) in citations.indexed) ...[
            if (i > 0) Divider(height: 1, color: t.colors.borderSubtle),
            CitationRow(
              citation: c,
              l10n: l10n,
              label: label?.call(c),
              onTap: onTap == null ? null : () => onTap!(c),
            ),
          ],
        ],
      ),
    );
  }
}

/// One citation. A Knowledge citation has a title, a score and a state; a
/// citation of the open file has a passage locator (shown via [label]) and
/// may carry the quoted text. Missing fields are omitted, never invented.
class CitationRow extends StatelessWidget {
  const CitationRow({
    super.key,
    required this.citation,
    required this.l10n,
    this.label,
    this.onTap,
  });
  final Map citation;
  final AppLocalizations l10n;
  final String? label;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final rawScore = citation['score'];
    final score =
        rawScore is num ? rawScore.toDouble().clamp(0, 1).toDouble() : null;
    final state = citation['state'] as String?;
    final title = label ??
        (citation['title'] as String?) ??
        (citation['locator'] as String?) ??
        '';
    final quote = (citation['text'] as String?) ??
        (citation['snippet'] as String?) ??
        (citation['quote'] as String?);
    final row = Padding(
      padding: const EdgeInsets.symmetric(
          horizontal: HarborSpace.s1, vertical: HarborSpace.s2 + 2),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(children: [
            Expanded(
              child: Text(title, style: t.text.bodyStrongOf(t.colors.ink)),
            ),
            if (onTap != null)
              Icon(Icons.my_location_outlined, size: 16, color: t.colors.brand),
          ]),
          if (quote != null && quote.trim().isNotEmpty) ...[
            const SizedBox(height: HarborSpace.s1),
            Text(quote.trim(),
                maxLines: 3,
                overflow: TextOverflow.ellipsis,
                style: t.text.smallOf(t.colors.inkMuted)),
          ],
          if (score != null) ...[
            const SizedBox(height: HarborSpace.s1),
            Row(children: [
              Expanded(
                child: ClipRRect(
                  borderRadius: BorderRadius.circular(HarborRadius.pill),
                  child: LinearProgressIndicator(value: score, minHeight: 4),
                ),
              ),
              const SizedBox(width: HarborSpace.s3),
              Text(
                  l10n.scoreLine((score * 100).toStringAsFixed(1), state ?? ''),
                  style: t.text.captionOf(t.colors.inkMuted)),
            ]),
          ],
        ],
      ),
    );
    if (onTap == null) return row;
    return Semantics(
      button: true,
      label: l10n.workCitationJump(title),
      child: InkWell(
        borderRadius: BorderRadius.circular(HarborRadius.sm),
        onTap: onTap,
        child: ExcludeSemantics(child: row),
      ),
    );
  }
}
