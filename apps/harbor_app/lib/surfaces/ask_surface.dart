import 'package:flutter/material.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import '../widgets/trust.dart';

/// Ask: grounded Q&A over the local Knowledge index. The full journey —
/// question → on-device generation grounded in real citations → durable
/// run in Activity — runs as a cancellable background op with live token
/// progress. Without a chat model the surface falls back honestly to
/// retrieval-only citations; without evidence it abstains (goal §12).
///
/// Turns are kept for the session (the surface stack preserves them);
/// the durable truth stays in Activity.
class AskSurface extends StatefulWidget {
  const AskSurface({super.key});

  @override
  State<AskSurface> createState() => _AskSurfaceState();
}

class _Turn {
  _Turn(this.question, {this.runId});
  final String question;
  final String? runId;
  Map<String, dynamic>? answer;
  Map<String, dynamic>? evidence;
  String? error;
  bool cancelled = false;
  bool busy = true;
}

class _AskSurfaceState extends State<AskSurface> {
  final _controller = TextEditingController();
  final _focus = FocusNode();
  final _scroll = ScrollController();
  final List<_Turn> _turns = [];
  bool _busy = false;
  String? _chatPackage;

  @override
  void initState() {
    super.initState();
    _controller.addListener(() => setState(() {}));
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _ensureKnowledgeOpen();
      _pickDefaultChatModel();
    });
  }

  @override
  void dispose() {
    _controller.dispose();
    _focus.dispose();
    _scroll.dispose();
    super.dispose();
  }

  Future<void> _ensureKnowledgeOpen() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null || service.knowledgeOpen) return;
    await service.openKnowledge();
    if (mounted) setState(() {});
  }

  void _pickDefaultChatModel() {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final models = service.installedModels;
    if (models.isEmpty) return;
    final preferred = AppStateScope.maybeOf(context)?.chatModel;
    final installed = models.map((m) => m['id'] as String).toList();
    setState(() {
      _chatPackage = preferred != null && installed.contains(preferred)
          ? preferred
          : (_chatPackage != null && installed.contains(_chatPackage)
              ? _chatPackage
              : installed.first);
    });
  }

  void _scrollToEnd() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_scroll.hasClients) return;
      final motion = HarborMotion.of(context);
      final target = _scroll.position.maxScrollExtent;
      if (motion.reduced) {
        _scroll.jumpTo(target);
      } else {
        _scroll.animateTo(target,
            duration: motion.slow, curve: HarborMotion.easing);
      }
    });
  }

  /// Retrieval-only: citations without generation (always available when
  /// the knowledge index is open).
  Future<void> _searchOnly() async {
    final service = HarborServiceProvider.of(context).notifier;
    final question = _controller.text.trim();
    if (service == null || _busy || question.isEmpty) return;
    final turn = _Turn(question);
    setState(() {
      _busy = true;
      _turns.add(turn);
      _controller.clear();
    });
    _scrollToEnd();
    final result = await service.searchKnowledge(question, topK: 5);
    if (!mounted) return;
    setState(() {
      turn.evidence = result ?? const {'citations': []};
      turn.busy = false;
      _busy = false;
    });
    _scrollToEnd();
  }

  /// The complete grounded journey: create a durable run, generate
  /// on-device against the cited evidence, log both as run events.
  Future<void> _ask() async {
    final service = HarborServiceProvider.of(context).notifier;
    final question = _controller.text.trim();
    final chatPackage = _chatPackage;
    if (service == null || _busy || question.isEmpty || chatPackage == null) {
      return;
    }
    final runId = HarborService.newRunId();
    final turn = _Turn(question, runId: runId);
    setState(() {
      _busy = true;
      _turns.add(turn);
      _controller.clear();
    });
    _scrollToEnd();
    try {
      await service.createRun(runId);
      final answer = await service.generateAnswer(
        question,
        chatPackage: chatPackage,
        runId: runId,
      );
      if (!mounted) return;
      setState(() {
        turn.answer = answer;
        turn.busy = false;
        _busy = false;
      });
    } on ffi.HarborCoreException catch (e) {
      if (!mounted) return;
      setState(() {
        turn.cancelled = e.message.toLowerCase().contains('cancel');
        turn.error = turn.cancelled ? null : e.message;
        turn.busy = false;
        _busy = false;
      });
    }
    _scrollToEnd();
  }

  void _cancel() {
    final service = HarborServiceProvider.of(context).notifier;
    final opId = service?.kindProgress['generate']?['op_id'] as String?;
    if (opId != null) service?.cancelOp(opId);
  }

  void _clear() => setState(_turns.clear);

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final state = AppStateScope.maybeOf(context);
    final knowledgeReady = service != null && service.knowledgeOpen;
    final models = service?.installedModels ?? const [];
    final generateProgress = _busy ? service?.kindProgress['generate'] : null;
    final wc = HarborBreakpoints.of(context);
    final gutter = HarborBreakpoints.gutter(wc);

    return Column(children: [
      HarborSurfaceHeader(
        title: l10n.surfaceAsk,
        subtitle: l10n.askSubtitle,
        actions: [
          if (models.isNotEmpty)
            _ModelPicker(
              models: [for (final m in models) m['id'] as String],
              selected: _chatPackage,
              runtimeLabel: runtimeLabelFor(service),
              onSelected: (id) {
                setState(() => _chatPackage = id);
                state?.setChatModel(id);
              },
            ),
          if (_turns.isNotEmpty)
            IconButton(
              tooltip: l10n.askClearConversation,
              onPressed: _busy ? null : _clear,
              icon: const Icon(Icons.delete_sweep_outlined),
            ),
        ],
      ),
      Expanded(
        child: Builder(builder: (context) {
          final noModelBanner = models.isEmpty && !sp.failed
              ? Padding(
                  padding: EdgeInsets.fromLTRB(
                      gutter, HarborSpace.s2, gutter, HarborSpace.s3),
                  child: Center(
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(
                          maxWidth: HarborLayout.readingMax + 80),
                      child: HarborBanner(
                        tone: HarborBannerTone.info,
                        dense: true,
                        icon: Icons.memory_outlined,
                        title: l10n.askModelPicker,
                        body: l10n.askNoChatModel,
                        action: state == null
                            ? null
                            : TextButton(
                                onPressed: () =>
                                    state.goTo(HarborSurface.models),
                                child: Text(l10n.surfaceModels),
                              ),
                      ),
                    ),
                  ),
                )
              : null;
          if (sp.failed) {
            return HarborErrorState(
              title: l10n.coreDegradedTitle,
              message: l10n.coreStartFailed,
            );
          }
          if (_turns.isEmpty) {
            // Centered on tall canvases, scrollable on short ones / 200%.
            return LayoutBuilder(
              builder: (context, constraints) => SingleChildScrollView(
                controller: _scroll,
                child: ConstrainedBox(
                  constraints: BoxConstraints(minHeight: constraints.maxHeight),
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      _EmptyConversation(
                        knowledgeReady: knowledgeReady,
                        hasChatModel: models.isNotEmpty,
                      ),
                      if (noModelBanner != null) noModelBanner,
                    ],
                  ),
                ),
              ),
            );
          }
          return Scrollbar(
            controller: _scroll,
            child: ListView.builder(
              controller: _scroll,
              padding: EdgeInsets.fromLTRB(
                  gutter, HarborSpace.s2, gutter, HarborSpace.s6),
              itemCount: _turns.length + (noModelBanner == null ? 0 : 1),
              itemBuilder: (context, i) {
                if (i == _turns.length) return noModelBanner!;
                return Center(
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(
                        maxWidth: HarborLayout.readingMax + 80),
                    child: _TurnView(
                      turn: _turns[i],
                      progress:
                          i == _turns.length - 1 ? generateProgress : null,
                      onCancel: _cancel,
                    ),
                  ),
                );
              },
            ),
          );
        }),
      ),
      SafeArea(
        top: false,
        child: Padding(
          padding: EdgeInsets.fromLTRB(gutter, 0, gutter, HarborSpace.s3),
          child: Center(
            child: ConstrainedBox(
              constraints:
                  const BoxConstraints(maxWidth: HarborLayout.readingMax + 80),
              child: Container(
                padding: const EdgeInsets.fromLTRB(HarborSpace.s2,
                    HarborSpace.s1, HarborSpace.s2, HarborSpace.s1),
                decoration: ShapeDecoration(
                  color: t.colors.surface,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(HarborRadius.lg),
                    side: BorderSide(color: t.colors.border),
                  ),
                ),
                child:
                    Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
                  Expanded(
                    child: TextField(
                      controller: _controller,
                      focusNode: _focus,
                      minLines: 1,
                      maxLines: 4,
                      enabled: !sp.failed,
                      textInputAction: TextInputAction.send,
                      onSubmitted: (_) =>
                          _chatPackage == null ? _searchOnly() : _ask(),
                      decoration: InputDecoration(
                        hintText: l10n.askComposerHint,
                        border: InputBorder.none,
                        enabledBorder: InputBorder.none,
                        focusedBorder: InputBorder.none,
                        disabledBorder: InputBorder.none,
                        filled: false,
                        contentPadding: const EdgeInsets.symmetric(
                            horizontal: HarborSpace.s2,
                            vertical: HarborSpace.s3),
                      ),
                    ),
                  ),
                  IconButton(
                    tooltip: l10n.askSearchOnly,
                    onPressed: knowledgeReady &&
                            !_busy &&
                            _controller.text.trim().isNotEmpty
                        ? _searchOnly
                        : null,
                    icon: const Icon(Icons.manage_search_outlined),
                  ),
                  const SizedBox(width: HarborSpace.s1),
                  if (_busy)
                    IconButton.filled(
                      tooltip: l10n.cancelAction,
                      onPressed: _cancel,
                      icon: const Icon(Icons.stop),
                    )
                  else
                    IconButton.filled(
                      tooltip: l10n.askGenerateTooltip,
                      onPressed: knowledgeReady &&
                              _chatPackage != null &&
                              _controller.text.trim().isNotEmpty
                          ? _ask
                          : null,
                      icon: const Icon(Icons.auto_awesome_outlined),
                    ),
                ]),
              ),
            ),
          ),
        ),
      ),
    ]);
  }
}

class _ModelPicker extends StatelessWidget {
  const _ModelPicker({
    required this.models,
    required this.selected,
    required this.runtimeLabel,
    required this.onSelected,
  });
  final List<String> models;
  final String? selected;
  final String runtimeLabel;
  final ValueChanged<String> onSelected;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return MenuAnchor(
      builder: (context, controller, _) => ModelDock.chip(
        modelLabel: selected ?? l10n.askModelPicker,
        runtimeLabel: runtimeLabel,
        semantic: ExecutionSemantic.local,
        onTap: () => controller.isOpen ? controller.close() : controller.open(),
      ),
      menuChildren: [
        Padding(
          padding: const EdgeInsets.fromLTRB(
              HarborSpace.s3, HarborSpace.s2, HarborSpace.s3, HarborSpace.s1),
          child: Text(l10n.askModelPicker,
              style: t.text.captionOf(t.colors.inkMuted)),
        ),
        for (final id in models)
          MenuItemButton(
            leadingIcon: Icon(
              id == selected ? Icons.check : Icons.memory_outlined,
              size: 18,
              color: id == selected ? t.colors.brand : t.colors.inkMuted,
            ),
            onPressed: () => onSelected(id),
            child: HarborIdentifier(id, size: 12),
          ),
      ],
    );
  }
}

class _EmptyConversation extends StatelessWidget {
  const _EmptyConversation(
      {required this.knowledgeReady, required this.hasChatModel});
  final bool knowledgeReady;
  final bool hasChatModel;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final state = AppStateScope.maybeOf(context);
    return HarborEmptyState(
      scrollable: false,
      icon: Icons.question_answer_outlined,
      title: l10n.askEmptyTitle,
      body: knowledgeReady ? l10n.askEmptyBody : l10n.askKnowledgeNotOpen,
      // The "no chat model" banner already links to Models; only offer
      // the button here when that banner is absent.
      actionLabel: knowledgeReady || state == null || !hasChatModel
          ? null
          : l10n.surfaceModels,
      onAction: state == null ? null : () => state.goTo(HarborSurface.models),
    );
  }
}

class _TurnView extends StatelessWidget {
  const _TurnView({
    required this.turn,
    required this.progress,
    required this.onCancel,
  });
  final _Turn turn;
  final Map<String, dynamic>? progress;
  final VoidCallback onCancel;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: HarborSpace.s3),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Align(
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
                    Text(l10n.askYou,
                        style: t.text.captionOf(t.colors.inkMuted)),
                    const SizedBox(height: 2),
                    SelectableText(turn.question,
                        style: t.text.bodyOf(t.colors.ink)),
                  ],
                ),
              ),
            ),
          ),
          const SizedBox(height: HarborSpace.s3),
          if (turn.busy)
            _Generating(progress: progress, onCancel: onCancel)
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
            _AnswerView(answer: turn.answer!)
          else if (turn.evidence != null)
            _EvidenceView(
                citations: ((turn.evidence!['citations'] as List?) ?? const [])
                    .cast<Map>()),
        ],
      ),
    );
  }
}

class _Generating extends StatelessWidget {
  const _Generating({required this.progress, required this.onCancel});
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

class _AnswerView extends StatelessWidget {
  const _AnswerView({required this.answer});
  final Map<String, dynamic> answer;

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
          _CitationList(title: l10n.askCitationsHeading, citations: citations),
        ],
      ],
    );
  }
}

class _EvidenceView extends StatelessWidget {
  const _EvidenceView({required this.citations});
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
    return _CitationList(
      title: l10n.askEvidenceHeading,
      note: l10n.askRetrievalOnlyNote,
      citations: citations,
    );
  }
}

class _CitationList extends StatelessWidget {
  const _CitationList(
      {required this.title, required this.citations, this.note});
  final String title;
  final String? note;
  final List<Map> citations;

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
            _CitationRow(citation: c, l10n: l10n),
          ],
        ],
      ),
    );
  }
}

class _CitationRow extends StatelessWidget {
  const _CitationRow({required this.citation, required this.l10n});
  final Map citation;
  final AppLocalizations l10n;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final score = (citation['score'] as num).toDouble().clamp(0, 1).toDouble();
    final scorePct = (score * 100).toStringAsFixed(1);
    final state = citation['state'] as String;
    return Padding(
      padding: const EdgeInsets.symmetric(
          horizontal: HarborSpace.s1, vertical: HarborSpace.s2 + 2),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(citation['title'] as String,
              style: t.text.bodyStrongOf(t.colors.ink)),
          const SizedBox(height: HarborSpace.s1),
          Row(children: [
            Expanded(
              child: ClipRRect(
                borderRadius: BorderRadius.circular(HarborRadius.pill),
                child: LinearProgressIndicator(value: score, minHeight: 4),
              ),
            ),
            const SizedBox(width: HarborSpace.s3),
            Text(l10n.scoreLine(scorePct, state),
                style: t.text.captionOf(t.colors.inkMuted)),
          ]),
        ],
      ),
    );
  }
}
