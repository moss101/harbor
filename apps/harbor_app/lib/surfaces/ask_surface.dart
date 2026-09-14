import 'package:flutter/material.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';

/// Ask: grounded Q&A over the local Knowledge index. The full journey —
/// question → on-device generation grounded in real citations → durable
/// run in Activity — runs as a cancellable background op with live token
/// progress. Without a chat model the surface falls back honestly to
/// retrieval-only citations; without evidence it abstains (goal §12).
class AskSurface extends StatefulWidget {
  const AskSurface({super.key});

  @override
  State<AskSurface> createState() => _AskSurfaceState();
}

class _AskSurfaceState extends State<AskSurface> {
  final _controller = TextEditingController();
  Map<String, dynamic>? _result;
  Map<String, dynamic>? _answer;
  bool _busy = false;
  bool _cancelled = false;
  String? _error;
  String? _chatPackage;
  String? _activeOpId;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _ensureKnowledgeOpen();
      _pickDefaultChatModel();
    });
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _ensureKnowledgeOpen() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null || service.knowledgeOpen) return;
    await service.openKnowledge();
    if (mounted) setState(() {});
  }

  void _pickDefaultChatModel() {
    if (_chatPackage != null) return;
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final models = service.installedModels;
    if (models.isNotEmpty) {
      setState(() => _chatPackage = models.first['id'] as String);
    }
  }

  /// Retrieval-only: citations without generation (always available when
  /// the knowledge index is open).
  Future<void> _searchOnly() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null || _busy) return;
    setState(() {
      _busy = true;
      _error = null;
      _cancelled = false;
    });
    final result = await service.searchKnowledge(_controller.text, topK: 5);
    if (!mounted) return;
    setState(() {
      _result = result;
      _answer = null;
      _busy = false;
    });
  }

  /// The complete grounded journey: create a durable run, generate
  /// on-device against the cited evidence, log both as run events.
  Future<void> _ask() async {
    final service = HarborServiceProvider.of(context).notifier;
    final question = _controller.text.trim();
    if (service == null || _busy || question.isEmpty) return;
    final chatPackage = _chatPackage;
    if (chatPackage == null) return;
    setState(() {
      _busy = true;
      _error = null;
      _cancelled = false;
      _answer = null;
      _result = null;
    });
    final runId = HarborService.newRunId();
    try {
      await service.createRun(runId);
      final answer = await service.generateAnswer(
        question,
        chatPackage: chatPackage,
        runId: runId,
      );
      if (!mounted) return;
      setState(() {
        _answer = answer;
        _result = null;
        _busy = false;
        _activeOpId = null;
      });
    } on ffi.HarborCoreException catch (e) {
      if (!mounted) return;
      setState(() {
        _cancelled = e.message.toLowerCase().contains('cancel');
        _error = _cancelled ? null : e.message;
        _busy = false;
        _activeOpId = null;
      });
    }
  }

  void _cancel() {
    final opId = _activeOpId ??
        (HarborServiceProvider.of(context).notifier?.kindProgress['generate']
            ?['op_id'] as String?);
    if (opId != null) {
      HarborServiceProvider.of(context).notifier?.cancelOp(opId);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final knowledgeReady = service != null && service.knowledgeOpen;
    final models = service?.installedModels ?? const [];
    final generateProgress = _busy ? service?.kindProgress['generate'] : null;
    return Column(children: [
      Expanded(
        child: _buildBody(context, l10n, t, knowledgeReady, generateProgress),
      ),
      if (models.isNotEmpty)
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: HarborSpace.s4),
          child: Row(children: [
            Text(l10n.surfaceModels,
                style: t.text.captionOf(t.colors.inkMuted)),
            const SizedBox(width: HarborSpace.s2),
            DropdownButton<String>(
              value: _chatPackage,
              underline: const SizedBox.shrink(),
              items: [
                for (final m in models)
                  DropdownMenuItem(
                      value: m['id'] as String, child: Text(m['id'] as String)),
              ],
              onChanged: (v) => setState(() => _chatPackage = v),
            ),
          ]),
        )
      else
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: HarborSpace.s4),
          child: Align(
            alignment: AlignmentDirectional.centerStart,
            child: Text(l10n.askNoChatModel,
                style: t.text.captionOf(t.colors.inkMuted)),
          ),
        ),
      SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(HarborSpace.s4),
          child: Row(children: [
            Expanded(
              child: TextField(
                controller: _controller,
                onSubmitted: (_) =>
                    _chatPackage == null ? _searchOnly() : _ask(),
                decoration: InputDecoration(hintText: l10n.homeComposerHint),
              ),
            ),
            const SizedBox(width: HarborSpace.s2),
            IconButton(
                tooltip: l10n.askSearchOnly,
                onPressed: knowledgeReady && !_busy ? _searchOnly : null,
                icon: const Icon(Icons.search)),
            const SizedBox(width: HarborSpace.s2),
            if (_busy)
              IconButton.filled(
                tooltip: l10n.cancelAction,
                onPressed: _cancel,
                icon: const Icon(Icons.stop),
              )
            else
              IconButton.filled(
                tooltip: l10n.askGenerateTooltip,
                onPressed: knowledgeReady && _chatPackage != null ? _ask : null,
                icon: const Icon(Icons.auto_awesome_outlined),
              ),
          ]),
        ),
      ),
    ]);
  }

  Widget _buildBody(
    BuildContext context,
    AppLocalizations l10n,
    HarborTheme t,
    bool knowledgeReady,
    Map<String, dynamic>? generateProgress,
  ) {
    final tokensDone = (generateProgress?['items_done'] as num?)?.toInt() ?? 0;
    if (_busy) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const CircularProgressIndicator(),
            const SizedBox(height: HarborSpace.s3),
            Text(l10n.askGenerating),
            if (tokensDone > 0) ...[
              const SizedBox(height: HarborSpace.s2),
              Text(l10n.askGenerationProgress(tokensDone),
                  style: t.text.captionOf(t.colors.inkMuted)),
            ],
          ],
        ),
      );
    }
    if (_cancelled) {
      return HarborEmptyState(
          title: l10n.cancelAction, body: l10n.askGenerating);
    }
    if (_error != null) {
      return HarborErrorState(message: _error!);
    }
    final answer = _answer;
    if (answer != null) {
      final answerText = (answer['answer'] as String?)?.trim() ?? '';
      final insufficient = answerText == 'INSUFFICIENT_EVIDENCE';
      final citations =
          ((answer['citations'] as List?) ?? const []).cast<Map>();
      return ListView(
        padding: const EdgeInsets.all(HarborSpace.s4),
        children: [
          Text(l10n.askAnswerHeading, style: t.text.h2Of(t.colors.ink)),
          const SizedBox(height: HarborSpace.s2),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(HarborSpace.s3),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  SelectableText(
                      insufficient ? l10n.askAbstentionHeading : answerText),
                  const SizedBox(height: HarborSpace.s2),
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
            ),
          ),
          if (insufficient)
            Padding(
              padding: const EdgeInsets.only(top: HarborSpace.s2),
              child: Text(l10n.askInsufficientNote,
                  style: t.text.smallOf(t.colors.inkMuted)),
            ),
          if (answer['used_citations'] == true) ...[
            const SizedBox(height: HarborSpace.s4),
            Text(l10n.askCitationsHeading, style: t.text.h2Of(t.colors.ink)),
            const SizedBox(height: HarborSpace.s2),
            for (final c in citations)
              Builder(builder: (context) {
                final scorePct =
                    ((c['score'] as num).toDouble() * 100).toStringAsFixed(1);
                final state = c['state'] as String;
                return Card(
                  margin: const EdgeInsets.only(bottom: HarborSpace.s2),
                  child: ListTile(
                    leading: Icon(Icons.format_quote_outlined,
                        color: t.colors.brand),
                    title: Text(c['title'] as String),
                    subtitle: Text(l10n.scoreLine(scorePct, state)),
                  ),
                );
              }),
          ],
        ],
      );
    }
    final result = _result;
    if (result != null) {
      final citations = (result['citations'] as List).cast<Map>();
      return citations.isEmpty
          ? HarborEmptyState(
              title: l10n.askNoEvidenceTitle,
              body: l10n.askNoEvidenceBody,
            )
          : ListView(
              padding: const EdgeInsets.all(HarborSpace.s4),
              children: [
                for (final c in citations)
                  Builder(builder: (context) {
                    final scorePct = ((c['score'] as num).toDouble() * 100)
                        .toStringAsFixed(1);
                    final state = c['state'] as String;
                    return Card(
                      margin: const EdgeInsets.only(bottom: HarborSpace.s2),
                      child: ListTile(
                        leading: Icon(Icons.format_quote_outlined,
                            color: t.colors.brand),
                        title: Text(c['title'] as String),
                        subtitle: Text(l10n.scoreLine(scorePct, state)),
                      ),
                    );
                  }),
              ],
            );
    }
    return HarborEmptyState(
      title: l10n.askEmptyTitle,
      body: knowledgeReady ? l10n.askEmptyBody : l10n.askKnowledgeNotOpen,
    );
  }
}
