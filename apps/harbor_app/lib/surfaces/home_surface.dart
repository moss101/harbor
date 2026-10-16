import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:harbor_domain/harbor_domain.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

import '../services/file_types.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import '../services/request_router.dart';
import 'capture_flow.dart';
import '../widgets/answer.dart';
import '../widgets/composer.dart';
import '../widgets/ops.dart';
import '../widgets/route_panel.dart';
import '../widgets/trust.dart';
import 'knowledge_ingest.dart';
import 'skill_run.dart';

export '../widgets/composer.dart' show Composer, HarborComposer;

/// Home is one conversation (goal §21): "What do you want to get done?"
/// with a single composer. Sending routes the request ([RequestRouter]):
/// a question is answered inline as a turn — grounded and cited in an
/// attached Office file, else in the Library, retrieval-only when no chat
/// model is installed, abstaining without evidence, cancellable while it
/// generates, and recorded as a durable run; a task opens the recommended
/// skill's run sheet (an attached Office file already attached; nothing
/// runs until the person presses Run and approves) and stays in the
/// conversation as a card linking to that run. Quick actions prefill the
/// composer, never "chat with model X". Attached files ride with the
/// request; adding them to the Library is a separate, explicit action.
///
/// The empty conversation is the dashboard: greeting, the first-run model
/// banner, quick actions, background work, recent durable runs, the active
/// model and the Library index — all live facts from the core. Turns are
/// kept for the session (the surface stack preserves them); the durable
/// truth stays in Activity.
class HomeSurface extends StatefulWidget {
  const HomeSurface({super.key, required this.state});
  final AppState state;

  @override
  State<HomeSurface> createState() => _HomeSurfaceState();
}

/// A file attached to the request in the composer.
class _Attachment {
  _Attachment(this.file, this.bytes);
  final XFile file;
  final List<int> bytes;
  String get name => file.name;
  bool get isOffice => isOfficeName(file.name);
}

/// Why a request got guidance instead of an answer.
enum _Notice { needsModel, needsModelForFile, libraryClosed }

/// One request in the conversation and what came back for it: an answer
/// turn, a routed task (with the run it started, once it has one) or a
/// notice. While [routing], only the request is shown.
class _Entry {
  _Entry(this.request);
  final String request;
  bool routing = true;
  AnswerTurn? turn;
  RouteDecision? route;
  _Notice? notice;

  /// The office file the request was about, kept for re-opening the sheet.
  _Attachment? office;

  /// The latest report of the run the task's sheet started.
  Map<String, dynamic>? report;
}

class _HomeSurfaceState extends State<HomeSurface> {
  final _composer = TextEditingController();
  final _composerFocus = FocusNode();
  final _scroll = ScrollController();
  final _thread = ScrollController();
  final List<_Attachment> _attachments = [];
  final List<_Entry> _entries = [];

  @override
  void initState() {
    super.initState();
    widget.state.addListener(_consumePending);
    _composer.addListener(_onComposerChanged);
  }

  @override
  void dispose() {
    widget.state.removeListener(_consumePending);
    _composer.removeListener(_onComposerChanged);
    _composer.dispose();
    _composerFocus.dispose();
    _scroll.dispose();
    _thread.dispose();
    super.dispose();
  }

  /// "Search the Library only" follows whether there is text to search.
  void _onComposerChanged() {
    if (mounted) setState(() {});
  }

  bool _needsFirstModel(BuildContext context) {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    return !sp.failed && service != null && service.needsFirstModel;
  }

  /// Text handed over by the command palette ("Summarize document").
  void _consumePending() {
    final text = widget.state.pendingComposerText;
    if (text == null) return;
    widget.state.pendingComposerText = null;
    _prefill(text);
  }

  void _prefill(String label) {
    _composer.text = label;
    _composer.selection =
        TextSelection.collapsed(offset: _composer.text.length);
    _composerFocus.requestFocus();
  }

  /// Attach files to the request (chips on the composer). Nothing is
  /// indexed here: "Add to Library" does that, explicitly.
  Future<void> _attachFiles() async {
    final l10n = AppLocalizations.of(context)!;
    final group = knowledgeTypeGroup(l10n.fileGroupDocuments);
    final List<XFile> files;
    try {
      files = await openFiles(acceptedTypeGroups: [group]);
    } on Error {
      // A misconfigured type group throws Error from Dart before any
      // picker exists. Swallowing that as "dismissed" is exactly how
      // every file entry point on iOS stayed dead: silent, and labelled
      // as the user's choice. Let it surface.
      rethrow;
    } catch (_) {
      return; // picker dismissed
    }
    if (files.isEmpty) return;
    final read = [for (final f in files) _Attachment(f, await f.readAsBytes())];
    if (!mounted) return;
    setState(() => _attachments.addAll(read));
  }

  /// The explicit "Add to Library": index the attached files locally.
  Future<void> _addAttachmentsToKnowledge() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null || _attachments.isEmpty) return;
    final l10n = AppLocalizations.of(context)!;
    final outcome = await ingestFiles(
        service, [for (final a in _attachments) a.file], l10n);
    if (!mounted) return;
    showIngestFeedback(context, outcome, l10n);
  }

  /// Run a scan / voice flow and put its reviewed text in the composer (it is
  /// appended to what the person already typed, and never sent).
  Future<void> _capture(
      Future<String?> Function(BuildContext, HarborService) flow) async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final text = await flow(context, service);
    if (text == null || text.isEmpty || !mounted) return;
    final existing = _composer.text.trim();
    _prefill(existing.isEmpty ? text : '$existing\n$text');
  }

  _Attachment? get _officeAttachment {
    for (final a in _attachments) {
      if (a.isOffice) return a;
    }
    return null;
  }

  String? _chatPackage(HarborService service) =>
      chatPackageFor(service, widget.state.chatModel);

  _Entry? get _generating {
    for (final e in _entries) {
      if (e.turn?.busy == true) return e;
    }
    return null;
  }

  bool get _busy => _entries.any((e) => e.routing) || _generating != null;

  void _scrollToEnd() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_thread.hasClients) return;
      final motion = HarborMotion.of(context);
      final target = _thread.position.maxScrollExtent;
      if (motion.reduced) {
        _thread.jumpTo(target);
      } else {
        _thread.animateTo(target,
            duration: motion.slow, curve: HarborMotion.easing);
      }
    });
  }

  _Entry _begin(String text) {
    final entry = _Entry(text)..office = _officeAttachment;
    setState(() => _entries.add(entry));
    _scrollToEnd();
    return entry;
  }

  /// Route the request; show, never run (see [RequestRouter]).
  Future<void> _submit(String text) async {
    final service = HarborServiceProvider.of(context).notifier;
    final l10n = AppLocalizations.of(context)!;
    if (service == null) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(l10n.homeRequestFailed)));
      return;
    }
    if (_busy) return;
    final entry = _begin(text);
    final office = entry.office;
    final decision = await RequestRouter(service).route(
      text,
      file: office == null ? null : RouteFile(name: office.name),
      chatPackage: _chatPackage(service),
    );
    if (!mounted) return;
    setState(() => entry.routing = false);
    switch (decision) {
      case RouteAsk():
        await _ask(service, entry);
      case RouteSkill(:final skill):
        setState(() => entry.route = decision);
        await _openSkill(service, entry, skill);
      case RouteNone():
        setState(() => entry.route = decision);
    }
    _scrollToEnd();
  }

  /// A grounded answer, inline: from the attached Office file when there is
  /// one, else from the Library. Without a chat model, Library evidence
  /// alone (retrieval-only).
  Future<void> _ask(HarborService service, _Entry entry) async {
    final l10n = AppLocalizations.of(context)!;
    final question = entry.request;
    final office = entry.office;
    final chat = _chatPackage(service);
    if (chat == null && (office != null || !service.knowledgeOpen)) {
      setState(() => entry.notice =
          office != null ? _Notice.needsModelForFile : _Notice.needsModel);
      return;
    }
    final runId = chat == null ? null : HarborService.newRunId();
    final turn = AnswerTurn(question, runId: runId);
    setState(() => entry.turn = turn);
    try {
      if (chat == null) {
        final evidence = await service.searchKnowledge(question, topK: 5);
        turn.evidence = evidence ?? const {'citations': []};
      } else if (office != null) {
        final preview = await service.extractPreview(office.bytes);
        final doc = FileDocument.fromPreview(office.name, preview);
        if (doc == null || doc.passages.isEmpty) {
          turn.error = l10n.workNoReadableText;
        } else {
          await service.createRun(runId!);
          turn.answer = await service.askAboutFile(question,
              chatPackage: chat, runId: runId, document: doc);
        }
      } else {
        await service.createRun(runId!);
        turn.answer = await service.generateAnswer(question,
            chatPackage: chat, runId: runId);
      }
    } on ffi.HarborCoreException catch (e) {
      turn.cancelled = e.message.toLowerCase().contains('cancel');
      turn.error = turn.cancelled ? null : e.message;
    }
    if (!mounted) return;
    setState(() => turn.busy = false);
  }

  /// Retrieval-only: Library citations for the composer text, without
  /// generation, even when a chat model is installed.
  Future<void> _searchOnly() async {
    final service = HarborServiceProvider.of(context).notifier;
    final question = _composer.text.trim();
    if (service == null || _busy || question.isEmpty) return;
    _composer.clear();
    final entry = _begin(question);
    if (!service.knowledgeOpen) await service.openKnowledge();
    if (!mounted) return;
    if (!service.knowledgeOpen) {
      setState(() {
        entry.routing = false;
        entry.notice = _Notice.libraryClosed;
      });
      return;
    }
    final turn = AnswerTurn(question);
    setState(() {
      entry.routing = false;
      entry.turn = turn;
    });
    final result = await service.searchKnowledge(question, topK: 5);
    if (!mounted) return;
    setState(() {
      turn.evidence = result ?? const {'citations': []};
      turn.busy = false;
    });
    _scrollToEnd();
  }

  void _cancel() {
    final service = HarborServiceProvider.of(context).notifier;
    final opId = service?.kindProgress['generate']?['op_id'] as String?;
    if (opId != null) service?.cancelOp(opId);
  }

  /// The skill's run sheet with the request prefilled and the request's
  /// Office file already attached. The run it starts is linked from the
  /// task's card.
  Future<void> _openSkill(
      HarborService service, _Entry entry, SkillSummary skill) async {
    if (skill.graph == null) return;
    final office = entry.office;
    await SkillRunSheet.show(
      context,
      skill,
      service,
      initialArtifact: office == null
          ? null
          : AttachedFile(
              name: office.name,
              bytes: office.bytes,
              path: office.file.path.isEmpty ? null : office.file.path),
      initialRequest: entry.request,
      chatPackage: _chatPackage(service),
      onReport: (report) {
        if (mounted) setState(() => entry.report = report);
      },
    );
  }

  void _clearConversation() => setState(_entries.clear);

  String _greeting(AppLocalizations l10n) {
    final hour = DateTime.now().hour;
    if (hour < 12) return l10n.homeGreetingMorning;
    if (hour < 18) return l10n.homeGreetingAfternoon;
    return l10n.homeGreetingEvening;
  }

  Widget _composerBlock(
      AppLocalizations l10n, HarborTheme t, HarborService? service,
      {required bool docked}) {
    final office = _officeAttachment;
    final models = [
      for (final m in service?.installedModels ?? const [])
        if (m['id'] is String) m['id'] as String
    ];
    final generating = _generating;
    final canSearch =
        service != null && !_busy && _composer.text.trim().isNotEmpty;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        // Ask's tools, kept on the one composer: Library-only search and
        // the chat model answers run with.
        Wrap(
          alignment: WrapAlignment.end,
          crossAxisAlignment: WrapCrossAlignment.center,
          spacing: HarborSpace.s1,
          runSpacing: HarborSpace.s1,
          children: [
            IconButton(
              key: const ValueKey('home-search-only'),
              tooltip: l10n.askSearchOnly,
              onPressed: canSearch ? _searchOnly : null,
              icon: const Icon(Icons.manage_search_outlined),
            ),
            if (models.isNotEmpty && service != null)
              _ModelPicker(
                models: models,
                selected: _chatPackage(service),
                runtimeLabel: runtimeLabelFor(service),
                onSelected: widget.state.setChatModel,
              ),
          ],
        ),
        const SizedBox(height: HarborSpace.s1),
        HarborComposer(
          hint: l10n.homeComposerHint,
          controller: _composer,
          focusNode: _composerFocus,
          elevated: !docked,
          onAttach: _attachFiles,
          onScan: () => _capture(showScanFlow),
          onRecord: () => _capture(showVoiceFlow),
          onSubmit: _submit,
          busy: _busy,
          onCancel: generating == null ? null : _cancel,
          scope: office == null
              ? const ComposerScope.everything()
              : ComposerScope.thisFile(office.name),
          attachments: [
            for (final a in _attachments)
              InputChip(
                key: ValueKey('home-attachment-${a.name}'),
                avatar: Icon(
                    a.isOffice
                        ? Icons.description_outlined
                        : Icons.insert_drive_file_outlined,
                    size: 16,
                    color: t.colors.brand),
                // File names are data.
                label: Text(a.name, overflow: TextOverflow.ellipsis),
                deleteButtonTooltipMessage: l10n.homeAttachmentRemove(a.name),
                onDeleted:
                    _busy ? null : () => setState(() => _attachments.remove(a)),
              ),
            if (_attachments.isNotEmpty)
              TextButton.icon(
                key: const ValueKey('home-add-to-knowledge'),
                onPressed: _busy ? null : _addAttachmentsToKnowledge,
                icon: const Icon(Icons.library_add_outlined, size: 16),
                label: Text(l10n.homeAddToKnowledge),
              ),
          ],
        ),
        if (_attachments.isNotEmpty) ...[
          const SizedBox(height: HarborSpace.s1),
          Text(l10n.homeAttachmentsNote,
              style: t.text.captionOf(t.colors.inkMuted)),
        ],
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final service = HarborServiceProvider.of(context).notifier;
    if (_entries.isNotEmpty) return _conversation(l10n, t, service);
    return _dashboard(l10n, t, service);
  }

  /// The conversation: its turns above, the composer docked below.
  Widget _conversation(
      AppLocalizations l10n, HarborTheme t, HarborService? service) {
    final wc = HarborBreakpoints.of(context);
    final gutter = HarborBreakpoints.gutter(wc);
    const width = BoxConstraints(maxWidth: HarborLayout.readingMax + 80);
    return Column(
      key: const ValueKey('home-conversation'),
      children: [
        Padding(
          padding: EdgeInsets.fromLTRB(gutter, HarborSpace.s2, gutter, 0),
          child: Center(
            child: ConstrainedBox(
              constraints: width,
              child: Align(
                alignment: AlignmentDirectional.centerStart,
                child: TextButton.icon(
                  key: const ValueKey('home-clear-conversation'),
                  onPressed: _busy ? null : _clearConversation,
                  icon: const Icon(Icons.delete_sweep_outlined, size: 18),
                  label: Text(l10n.askClearConversation),
                ),
              ),
            ),
          ),
        ),
        Expanded(
          child: Scrollbar(
            controller: _thread,
            child: ListView.builder(
              controller: _thread,
              padding: EdgeInsets.fromLTRB(
                  gutter, HarborSpace.s2, gutter, HarborSpace.s4),
              itemCount: _entries.length,
              itemBuilder: (context, i) => Center(
                child: ConstrainedBox(
                  constraints: width,
                  child: _entryView(l10n, service, _entries[i]),
                ),
              ),
            ),
          ),
        ),
        SafeArea(
          top: false,
          child: Padding(
            padding: EdgeInsets.fromLTRB(gutter, 0, gutter, HarborSpace.s3),
            child: Center(
              child: ConstrainedBox(
                constraints: width,
                child: _composerBlock(l10n, t, service, docked: true),
              ),
            ),
          ),
        ),
      ],
    );
  }

  Widget _entryView(
      AppLocalizations l10n, HarborService? service, _Entry entry) {
    final turn = entry.turn;
    if (turn != null) {
      return AnswerTurnView(
        key: const ValueKey('home-answer'),
        turn: turn,
        progress: turn.busy ? service?.kindProgress['generate'] : null,
        onCancel: _cancel,
        citationLabel: (c) => fileCitationLabel(c, l10n),
      );
    }
    final Widget? outcome;
    if (entry.routing) {
      outcome = const LinearProgressIndicator(
          key: ValueKey('home-routing'), minHeight: 2);
    } else if (entry.notice case final notice?) {
      outcome = _noticeBanner(l10n, notice);
    } else if (entry.route case final route? when service != null) {
      outcome = _TaskCard(
        decision: route,
        report: entry.report,
        onPick: (skill) => _openSkill(service, entry, skill),
        onReopen: route is RouteSkill
            ? () => _openSkill(service, entry, route.skill)
            : null,
        onBrowse: () => widget.state.goTo(HarborSurface.skills),
        onViewRun: () => widget.state.goTo(HarborSurface.activity),
      );
    } else {
      outcome = null;
    }
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: HarborSpace.s3),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          QuestionBubble(text: entry.request),
          if (outcome != null) ...[
            const SizedBox(height: HarborSpace.s3),
            outcome,
          ],
        ],
      ),
    );
  }

  Widget _noticeBanner(AppLocalizations l10n, _Notice notice) {
    return HarborBanner(
      key: const ValueKey('home-needs-model'),
      tone: HarborBannerTone.info,
      dense: true,
      icon: notice == _Notice.libraryClosed
          ? Icons.local_library_outlined
          : Icons.memory_outlined,
      title: notice == _Notice.libraryClosed
          ? l10n.askSearchOnly
          : l10n.askModelPicker,
      body: switch (notice) {
        _Notice.needsModel => l10n.askNoChatModel,
        _Notice.needsModelForFile => l10n.workNeedsChatModel,
        _Notice.libraryClosed => l10n.askKnowledgeNotOpen,
      },
      action: TextButton(
        onPressed: () => widget.state.openModels(notice == _Notice.libraryClosed
            ? ModelsTab.installed
            : ModelsTab.recommended),
        child: Text(l10n.surfaceModels),
      ),
    );
  }

  /// The empty conversation: the work-first dashboard.
  Widget _dashboard(
      AppLocalizations l10n, HarborTheme t, HarborService? service) {
    final wc = HarborBreakpoints.of(context);
    final compact = HarborBreakpoints.isCompact(wc);
    final quick = [
      (l10n.quickSummarize, Icons.description_outlined),
      (l10n.quickAnalyze, Icons.table_chart_outlined),
      (l10n.quickPresent, Icons.slideshow_outlined),
      (l10n.quickCompare, Icons.difference_outlined),
      (l10n.quickOrganize, Icons.folder_open_outlined),
      (l10n.quickResearch, Icons.travel_explore_outlined),
      (l10n.quickTranslate, Icons.translate_outlined),
    ];
    final chips = [
      for (final (label, icon) in quick)
        ActionChip(
          avatar: Icon(icon, size: 16, color: t.colors.brand),
          label: Text(label),
          onPressed: () => _prefill(label),
        ),
    ];

    return HarborPage(
      controller: _scroll,
      children: [
        SizedBox(height: compact ? HarborSpace.s5 : HarborSpace.s10),
        Text(_greeting(l10n), style: t.text.smallOf(t.colors.inkMuted)),
        const SizedBox(height: HarborSpace.s1),
        Text(l10n.homeHeadline,
            style: compact
                ? t.text.titleOf(t.colors.ink)
                : t.text.displayOf(t.colors.ink)),
        const SizedBox(height: HarborSpace.s5),
        // First run (production plan C2): a model install gates every
        // model-backed action; say so once, plainly, with the Local Only
        // fact next to it, and take the user straight to Recommended.
        if (_needsFirstModel(context)) ...[
          HarborBanner(
            key: const ValueKey('home-first-run'),
            tone: HarborBannerTone.info,
            icon: Icons.download_outlined,
            title: l10n.homeFirstRunTitle,
            body: l10n.homeFirstRunBody,
            action: FilledButton.tonalIcon(
              key: const ValueKey('home-first-run-install'),
              onPressed: () => widget.state.openModels(ModelsTab.recommended),
              icon: const Icon(Icons.recommend_outlined, size: 18),
              label: Text(l10n.homeFirstRunAction),
            ),
          ),
          const SizedBox(height: HarborSpace.s4),
        ],
        _composerBlock(l10n, t, service, docked: false),
        const SizedBox(height: HarborSpace.s4),
        if (compact)
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            clipBehavior: Clip.none,
            child: Row(children: [
              for (final chip in chips) ...[
                chip,
                const SizedBox(width: HarborSpace.s2),
              ],
            ]),
          )
        else
          Wrap(
              spacing: HarborSpace.s2,
              runSpacing: HarborSpace.s2,
              children: chips),
        SizedBox(height: compact ? HarborSpace.s6 : HarborSpace.s10),
        HarborTwoColumn(
          asideFirstWhenStacked: true,
          main: _MainColumn(state: widget.state),
          aside: _AsideColumn(state: widget.state),
        ),
      ],
    );
  }
}

/// A routed task in the conversation: which skill's sheet was opened (or
/// why none was picked, with choices), the run it started once it has one,
/// and a way back into the sheet.
class _TaskCard extends StatelessWidget {
  const _TaskCard({
    required this.decision,
    required this.report,
    required this.onPick,
    required this.onReopen,
    required this.onBrowse,
    required this.onViewRun,
  });
  final RouteDecision decision;
  final Map<String, dynamic>? report;
  final ValueChanged<SkillSummary> onPick;
  final VoidCallback? onReopen;
  final VoidCallback onBrowse;
  final VoidCallback onViewRun;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final runId = report?['run_id'] as String?;
    final state = report?['state'] as String?;
    return Column(
      key: const ValueKey('home-task'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        RoutePanel(decision: decision, onPick: onPick, onBrowse: onBrowse),
        if (onReopen != null || runId != null) ...[
          const SizedBox(height: HarborSpace.s2),
          Wrap(
            spacing: HarborSpace.s2,
            runSpacing: HarborSpace.s2,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              if (runId != null) ...[
                if (state != null) RunStateBadge(state: state),
                HarborIdentifier(runId, size: 12),
                TextButton.icon(
                  key: const ValueKey('home-task-view-run'),
                  onPressed: onViewRun,
                  icon: const Icon(Icons.timeline_outlined, size: 16),
                  label: Text(l10n.homeTaskViewRun),
                ),
              ],
              if (onReopen != null)
                TextButton.icon(
                  key: const ValueKey('home-task-reopen'),
                  onPressed: onReopen,
                  icon: const Icon(Icons.open_in_new, size: 16),
                  label: Text(l10n.homeTaskReopen),
                ),
            ],
          ),
        ],
      ],
    );
  }
}

/// The chat model answers run with (the person's choice is a preference).
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
        key: const ValueKey('home-model-picker'),
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

class _MainColumn extends StatelessWidget {
  const _MainColumn({required this.state});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final ops = service?.activeOps ?? const [];
    final runs = service?.runs ?? const [];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (ops.isNotEmpty) ...[
          HarborSectionHeader(title: l10n.homeSectionActive),
          for (final op in ops)
            Padding(
              padding: const EdgeInsets.only(bottom: HarborSpace.s2),
              child: OpProgressCard(progress: op, service: service),
            ),
          const SizedBox(height: HarborSpace.s5),
        ],
        HarborSectionHeader(
          title: l10n.homeSectionRecent,
          trailing: runs.isEmpty
              ? null
              : TextButton(
                  onPressed: () => state.goTo(HarborSurface.activity),
                  child: Text(l10n.viewAllAction),
                ),
        ),
        if (sp.failed)
          HarborBanner(
            tone: HarborBannerTone.danger,
            title: l10n.coreDegradedTitle,
            body: l10n.coreNotLoadedActivity,
            action: sp.failureDetail == null
                ? null
                : HarborIdentifier(sp.failureDetail!, size: 11, maxLines: 6),
          )
        else if (runs.isEmpty)
          HarborCard(
            raised: true,
            child: Row(children: [
              Icon(Icons.timeline_outlined, size: 18, color: t.colors.inkMuted),
              const SizedBox(width: HarborSpace.s3),
              Expanded(
                child: Text(l10n.homeRecentEmpty,
                    style: t.text.smallOf(t.colors.inkMuted)),
              ),
            ]),
          )
        else
          HarborCard(
            padding: const EdgeInsets.symmetric(vertical: HarborSpace.s1),
            child: Column(children: [
              for (final (i, r) in runs.take(5).indexed) ...[
                if (i > 0) Divider(height: 1, color: t.colors.borderSubtle),
                HarborListRow(
                  titleIsIdentifier: true,
                  title: HarborIdentifier(r['run_id'] as String, size: 12),
                  subtitle: Text(l10n.runStateLine(
                    r['state'] as String,
                    r['active_compute_ms_total'] as int,
                  )),
                  trailing: RunStateBadge(state: r['state'] as String),
                  onTap: () => state.goTo(HarborSurface.activity),
                ),
              ],
            ]),
          ),
      ],
    );
  }
}

class _AsideColumn extends StatelessWidget {
  const _AsideColumn({required this.state});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final models = service?.installedModels ?? const [];
    final chosen = state.chatModel;
    final active = models.isEmpty
        ? null
        : models.firstWhere((m) => m['id'] == chosen,
            orElse: () => models.first);
    final Widget dock;
    if (sp.failed || service == null) {
      dock = ModelDock(
        heading: l10n.homeModelHeading,
        modelLabel: l10n.modelDockCoreUnavailable,
        modelIsIdentifier: false,
        runtimeLabel: 'OFFLINE',
        semantic: ExecutionSemantic.danger,
      );
    } else if (active == null) {
      dock = ModelDock(
        heading: l10n.homeModelHeading,
        modelLabel: l10n.modelDockEmpty,
        modelIsIdentifier: false,
        runtimeLabel: runtimeLabelFor(service),
        semantic: ExecutionSemantic.local,
        // The dock is a navigation affordance to Models.
        onTap: () => state.goTo(HarborSurface.models),
      );
    } else {
      dock = ModelDock(
        heading: l10n.homeModelHeading,
        modelLabel: active['id'] as String,
        runtimeLabel: runtimeLabelFor(service),
        semantic: ExecutionSemantic.local,
        detail: active['runtime'] as String?,
        onTap: () => state.goTo(HarborSurface.models),
      );
    }
    final knowledgeOpen = service?.knowledgeOpen == true;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        dock,
        const SizedBox(height: HarborSpace.s3),
        HarborCard(
          onTap: () => state.goTo(HarborSurface.knowledge),
          semanticLabel: l10n.surfaceKnowledge,
          child: Row(children: [
            Container(
              width: 40,
              height: 40,
              decoration: BoxDecoration(
                color: t.colors.brandSoft,
                borderRadius: BorderRadius.circular(HarborRadius.sm),
              ),
              child: Icon(Icons.library_books_outlined,
                  size: 20, color: t.colors.brand),
            ),
            const SizedBox(width: HarborSpace.s3),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(l10n.lensSectionKnowledge,
                      style: t.text.captionOf(t.colors.inkMuted)),
                  Text(
                    knowledgeOpen
                        ? l10n.knowledgeSourcesCount(
                            service!.knowledgeSources.length)
                        : l10n.homeKnowledgeNotOpen,
                    style: t.text.bodyStrongOf(t.colors.ink),
                  ),
                ],
              ),
            ),
            Icon(Icons.chevron_right, color: t.colors.inkMuted),
          ]),
        ),
      ],
    );
  }
}
