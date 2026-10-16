import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/file_passages.dart';

/// One question and what came back for it: a grounded answer, retrieval-
/// only evidence, an error or a cancellation. Shared by the Home
/// conversation and the Work command bar so a grounded answer reads the
/// same everywhere (goal §12).
class AnswerTurn {
  AnswerTurn(this.question, {this.runId});
  final String question;
  final String? runId;
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

/// A question bubble followed by its outcome.
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
      padding: const EdgeInsets.symmetric(vertical: HarborSpace.s3),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          QuestionBubble(text: turn.question),
          const SizedBox(height: HarborSpace.s3),
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

/// What the person asked, as a bubble at the end edge.
class QuestionBubble extends StatelessWidget {
  const QuestionBubble({super.key, required this.text});
  final String text;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return Align(
      alignment: AlignmentDirectional.centerEnd,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 560),
        child: Container(
          padding: const EdgeInsets.symmetric(
              horizontal: HarborSpace.s4, vertical: HarborSpace.s3),
          decoration: BoxDecoration(
            color: t.colors.brandSoft,
            borderRadius: const BorderRadiusDirectional.only(
              topStart: Radius.circular(HarborRadius.lg),
              topEnd: Radius.circular(HarborRadius.lg),
              bottomStart: Radius.circular(HarborRadius.lg),
              bottomEnd: Radius.circular(HarborRadius.sm / 2),
            ),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(l10n.askYou, style: t.text.captionOf(t.colors.inkMuted)),
              const SizedBox(height: 2),
              SelectableText(text, style: t.text.bodyOf(t.colors.ink)),
            ],
          ),
        ),
      ),
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

/// A generated answer with its grounding badge, abstention, execution
/// facts and (when the model used them) its citations.
class GroundedAnswerView extends StatelessWidget {
  const GroundedAnswerView({
    super.key,
    required this.answer,
    this.onCitationTap,
    this.citationLabel,
  });
  final Map<String, dynamic> answer;
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
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        HarborCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(children: [
                Icon(Icons.auto_awesome_outlined,
                    size: 16, color: t.colors.brand),
                const SizedBox(width: HarborSpace.s2),
                Expanded(
                  child: Text(l10n.askAnswerHeading,
                      style: t.text.bodyStrongOf(t.colors.ink)),
                ),
                StatusBadge(
                  semantic: grounded
                      ? ExecutionSemantic.local
                      : ExecutionSemantic.hybrid,
                  icon: grounded ? Icons.verified_outlined : Icons.help_outline,
                  label: grounded
                      ? l10n.askGroundedBadge
                      : l10n.askUngroundedBadge,
                ),
              ]),
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
                    style: t.text.bodyOf(t.colors.ink).copyWith(fontSize: 15)),
              const SizedBox(height: HarborSpace.s3),
              Wrap(
                spacing: HarborSpace.s2,
                runSpacing: HarborSpace.s1,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  StatusBadge(
                    semantic: ExecutionSemantic.local,
                    label: ((answer['execution'] as String?) ?? 'ON_DEVICE')
                        .replaceAll('_', ' '),
                  ),
                  Text(
                    l10n.askExecutedOnLine(
                      (answer['executed_on'] as String?) ?? '',
                      ((answer['usage']?['completion_tokens'] as num?) ?? 0)
                          .toInt(),
                    ),
                    style: t.text.captionOf(t.colors.inkMuted),
                  ),
                ],
              ),
            ],
          ),
        ),
        if (answer['used_citations'] == true && citations.isNotEmpty) ...[
          const SizedBox(height: HarborSpace.s3),
          CitationList(
            title: l10n.askCitationsHeading,
            citations: citations,
            onTap: onCitationTap,
            label: citationLabel,
          ),
        ],
      ],
    );
  }
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
