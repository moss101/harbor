import 'dart:math' as math;

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
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
import '../widgets/skill_output.dart';
import '../widgets/trust.dart';
import 'knowledge_ingest.dart';
import 'skill_run.dart';
import 'threads_dialog.dart';

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
/// Answer-first: the empty conversation is one centred ask box under a
/// single headline, with the first-run model banner when no model is
/// installed, a few suggestions and any background work — nothing else.
/// Asking turns the page into a thread: each question is a heading over
/// its sources (compact chips) and answer, and the follow-up box stays
/// pinned at the bottom. Models, recent runs and the Library index live
/// in their own surfaces, out of the main path.
///
/// Library conversations persist as threads in the core (sealed at rest):
/// the latest is restored on entry, follow-ups are answered with the
/// thread's earlier turns as conversation memory. The shell's sidebar
/// lists them ([AppState.threads], published from here) and hands back
/// open / new / manage requests, which this surface consumes. Only
/// completed turns are recorded.
/// Questions about an attached file and routed tasks are not part of a
/// thread; the durable truth for every run stays in Activity.
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

  /// The Library collection the question was scoped to (never set with
  /// [office]); the answer is grounded only on its sources.
  String? collection;

  /// The latest report of the run the task's sheet started.
  Map<String, dynamic>? report;

  /// The skill whose sheet was opened for this request (the routed one,
  /// or the one the person picked from the choices).
  SkillSummary? skill;
}

class _HomeSurfaceState extends State<HomeSurface> {
  final _composer = TextEditingController();
  final _composerFocus = FocusNode();
  final _scroll = ScrollController();
  final _thread = ScrollController();
  final List<_Attachment> _attachments = [];
  final List<_Entry> _entries = [];

  /// The persisted thread the visible Library turns belong to (null until
  /// the first completed question creates one).
  String? _threadId;
  List<Map<String, dynamic>> _threads = const [];
  bool _restoreTried = false;

  /// What the next question is about (Everything or one collection). A
  /// file attached to the request overrides it ("This file").
  ComposerScope _scope = const ComposerScope.everything();

  /// The scope in effect: a collection that no longer has sources falls
  /// back to Everything, so a stale choice can never silently empty an
  /// answer.
  ComposerScope _effectiveScope(HarborService? service) {
    final office = _officeAttachment;
    if (office != null) return ComposerScope.thisFile(office.name);
    if (_scope.kind == ComposerScopeKind.collection &&
        (service?.knowledgeCollections ?? const [])
            .any((c) => c['name'] == _scope.name)) {
      return _scope;
    }
    return const ComposerScope.everything();
  }

  @override
  void initState() {
    super.initState();
    widget.state.addListener(_consumePending);
    _composer.addListener(_onComposerChanged);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _consumePending();
    });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // The core may open after this surface first builds: restore the
    // latest conversation once it is reachable.
    if (!_restoreTried && HarborServiceProvider.of(context).notifier != null) {
      _restoreTried = true;
      WidgetsBinding.instance.addPostFrameCallback((_) => _restoreLatest());
    }
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

  /// Requests handed over by the shell: text from the command palette
  /// ("Summarize document") and the sidebar's open / new / manage thread.
  void _consumePending() {
    final state = widget.state;
    final text = state.pendingComposerText;
    if (text != null) {
      state.pendingComposerText = null;
      _prefill(text);
    }
    final open = state.pendingThreadId;
    if (open != null) {
      state.pendingThreadId = null;
      if (!_busy && open != _threadId) _openThread(open);
    }
    if (state.pendingNewThread) {
      state.pendingNewThread = false;
      if (!_busy) _newConversation();
    }
    if (state.pendingManageThreads) {
      state.pendingManageThreads = false;
      if (!_busy) _showThreads();
    }
  }

  /// Mirror the thread list and the current thread for the sidebar.
  void _publishThreads() => widget.state.publishThreads(_threads, _threadId);

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
    if (entry.office == null) {
      final scope = _effectiveScope(HarborServiceProvider.of(context).notifier);
      if (scope.kind == ComposerScopeKind.collection) {
        entry.collection = scope.name;
      }
    }
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
      case RouteAsk(:final general):
        await _ask(service, entry, general: general);
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
  Future<void> _ask(HarborService service, _Entry entry,
      {bool general = false}) async {
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
    String? created;
    final turn =
        AnswerTurn(question, runId: runId, collection: entry.collection);
    setState(() => entry.turn = turn);
    try {
      if (chat == null) {
        final evidence = await service.searchKnowledge(question,
            topK: 5, collection: entry.collection);
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
        var threadId = _threadId;
        if (threadId == null) {
          // A thread the core cannot create costs persistence, not the
          // answer: the turn still runs, unrecorded.
          created = threadId =
              (await service.createThread())?['thread_id'] as String?;
          if (mounted) _threadId = threadId;
        }
        turn.answer = await service.generateAnswer(question,
            chatPackage: chat,
            runId: runId,
            collection: entry.collection,
            threadId: threadId,
            general: general);
      }
    } on ffi.HarborCoreException catch (e) {
      turn.cancelled = e.message.toLowerCase().contains('cancel');
      turn.error = turn.cancelled ? null : e.message;
    }
    // The core records only completed turns: a thread created for this
    // attempt is still empty, so it is dropped rather than left behind.
    if (turn.answer == null && created != null) {
      await service.deleteThread(created);
      if (mounted && _threadId == created) _threadId = null;
    }
    if (!mounted) return;
    setState(() => turn.busy = false);
    if (turn.answer != null && _threadId != null) await _refreshThreads();
    if (turn.evidence != null && office == null) {
      await _recordSearchTurn(service, question,
          (turn.evidence!['citations'] as List?) ?? const []);
    }
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
    final turn = AnswerTurn(question, collection: entry.collection);
    setState(() {
      entry.routing = false;
      entry.turn = turn;
    });
    final result = await service.searchKnowledge(question,
        topK: 5, collection: entry.collection);
    if (!mounted) return;
    setState(() {
      turn.evidence = result ?? const {'citations': []};
      turn.busy = false;
    });
    await _recordSearchTurn(
        service, question, (turn.evidence!['citations'] as List?) ?? const []);
    _scrollToEnd();
  }

  /// A retrieval-only turn belongs to the thread too, so a restored
  /// conversation shows what the person saw.
  Future<void> _recordSearchTurn(
      HarborService service, String question, List<dynamic> citations) async {
    var threadId = _threadId;
    String? created;
    if (threadId == null) {
      created =
          threadId = (await service.createThread())?['thread_id'] as String?;
      if (threadId == null) return;
      if (mounted) _threadId = threadId;
    }
    // Source chunk text is evidence, not conversation: never sent on.
    final slim = [
      for (final c in citations.cast<Map>())
        {...c.cast<String, dynamic>()}..remove('_text'),
    ];
    final ok = await service.appendSearchTurn(threadId, question, slim);
    if (!ok && created != null) {
      await service.deleteThread(created);
      if (mounted && _threadId == created) _threadId = null;
    }
    if (mounted) await _refreshThreads();
  }

  Future<void> _refreshThreads() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final all = await service.listThreads();
    if (!mounted) return;
    setState(() {
      // A thread with no completed turn is not a conversation yet.
      _threads = [
        for (final t in all)
          if (((t['turns'] as num?) ?? 0) > 0) t
      ];
    });
    _publishThreads();
  }

  Future<void> _restoreLatest() async {
    await _refreshThreads();
    if (!mounted || _threads.isEmpty) return;
    await _openThread(_threads.first['thread_id'] as String, onlyIfEmpty: true);
  }

  /// Load a persisted thread into the view. [onlyIfEmpty] keeps an
  /// automatic restore from clobbering a question the person already began.
  Future<void> _openThread(String id, {bool onlyIfEmpty = false}) async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final got = await service.getThread(id);
    if (!mounted) return;
    if (onlyIfEmpty && (_entries.isNotEmpty || _threadId != null)) return;
    if (got == null) {
      await _refreshThreads();
      return;
    }
    setState(() {
      _threadId = id;
      _entries
        ..clear()
        ..addAll(_entriesFrom((got['messages'] as List).cast<Map>()));
    });
    _publishThreads();
    _scrollToEnd();
  }

  /// Rebuild the visible entries from persisted messages: each question is
  /// followed by its answer (or its retrieval-only evidence).
  List<_Entry> _entriesFrom(List<Map> messages) {
    final entries = <_Entry>[];
    for (var i = 0; i < messages.length; i++) {
      final m = messages[i];
      if (m['role'] != 'user') continue;
      final reply = i + 1 < messages.length ? messages[i + 1] : null;
      final meta =
          ((reply?['meta'] as Map?) ?? const {}).cast<String, dynamic>();
      final turn =
          AnswerTurn(m['text'] as String, runId: meta['run_id'] as String?)
            ..busy = false;
      final citations = (meta['citations'] as List?) ?? const [];
      if (reply?['kind'] == 'answer') {
        turn.answer = {
          'answer': reply!['text'],
          'used_citations': meta['used_citations'] == true,
          'executed_on': meta['executed_on'] ?? '',
          'execution': 'ON_DEVICE',
          'usage': meta['usage'] ?? const {},
          'citations': citations,
        };
      } else if (reply?['kind'] == 'search') {
        turn.evidence = {'citations': citations};
      }
      entries.add(_Entry(m['text'] as String)
        ..routing = false
        ..turn = turn);
    }
    return entries;
  }

  Future<void> _showThreads() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    await _refreshThreads();
    if (!mounted) return;
    final picked = await showDialog<String>(
      context: context,
      builder: (_) => ThreadsDialog(
        service: service,
        threads: _threads,
        currentId: _threadId,
        onDeleted: (id) {
          if (id == _threadId) {
            setState(() {
              _entries.clear();
              _threadId = null;
            });
          }
          _refreshThreads();
        },
        onRenamed: (_) => _refreshThreads(),
      ),
    );
    if (picked != null && picked != _threadId && mounted) {
      await _openThread(picked);
    }
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
    // The sheet takes the keyboard; closing it must not hand the keyboard
    // back to the ask box over the result.
    FocusManager.instance.primaryFocus?.unfocus();
    setState(() => entry.skill = skill);
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

  /// Start a fresh conversation; the current one stays in the history.
  void _newConversation() {
    setState(() {
      _entries.clear();
      _threadId = null;
    });
    _publishThreads();
    _composerFocus.requestFocus();
  }

  Widget _composerBlock(
      AppLocalizations l10n, HarborTheme t, HarborService? service,
      {required bool docked}) {
    final office = _officeAttachment;
    final models = [for (final m in chatModelsOf(service)) m['id'] as String];
    final generating = _generating;
    final canSearch =
        service != null && !_busy && _composer.text.trim().isNotEmpty;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        HarborComposer(
          hint: docked ? l10n.homeFollowUpHint : l10n.homeComposerHint,
          controller: _composer,
          focusNode: _composerFocus,
          elevated: true,
          onAttach: _attachFiles,
          onScan: () => _capture(showScanFlow),
          onRecord: () => _capture(showVoiceFlow),
          onSubmit: _submit,
          busy: _busy,
          onCancel: generating == null ? null : _cancel,
          scope: _effectiveScope(service),
          scopeOptions: office != null
              ? null
              : [
                  const ComposerScope.everything(),
                  for (final c in service?.knowledgeCollections ?? const [])
                    ComposerScope.collection(c['name'] as String),
                ],
          onScopeChanged: (s) => setState(() => _scope = s),
          // Ask's tools live inside the one ask box: the chat model answers
          // run with, and Library-only search.
          tools: [
            if (models.isNotEmpty && service != null)
              _ModelPicker(
                models: models,
                selected: _chatPackage(service),
                runtimeLabel: runtimeLabelFor(service),
                onSelected: widget.state.setChatModel,
              ),
            IconButton(
              key: const ValueKey('home-search-only'),
              tooltip: l10n.askSearchOnly,
              onPressed: canSearch ? _searchOnly : null,
              icon: const Icon(Icons.manage_search_outlined),
            ),
          ],
          attachments: [
            for (final a in _attachments)
              _FileCard(
                key: ValueKey('home-attachment-${a.name}'),
                name: a.name,
                office: a.isOffice,
                removeTooltip: l10n.homeAttachmentRemove(a.name),
                onRemove:
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

  /// The thread: its turns as a readable column, the follow-up box pinned
  /// below.
  Widget _conversation(
      AppLocalizations l10n, HarborTheme t, HarborService? service) {
    final wc = HarborBreakpoints.of(context);
    final gutter = HarborBreakpoints.gutter(wc);
    const width = BoxConstraints(maxWidth: HarborLayout.askMax);
    return Column(
      key: const ValueKey('home-conversation'),
      children: [
        Expanded(
          child: Stack(children: [
            Positioned.fill(
                child: Scrollbar(
              controller: _thread,
              child: ListView.builder(
                controller: _thread,
                keyboardDismissBehavior:
                    ScrollViewKeyboardDismissBehavior.onDrag,
                padding: EdgeInsets.fromLTRB(
                    gutter, HarborSpace.s3, gutter, HarborSpace.s4),
                itemCount: _entries.length,
                itemBuilder: (context, i) => Center(
                  child: ConstrainedBox(
                    constraints: width,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        if (i > 0) Divider(height: 1, color: t.colors.border),
                        _entryView(l10n, service, _entries[i]),
                      ],
                    ),
                  ),
                ),
              ),
            )),
            // The follow-up box sits on the canvas; a soft fade lets the
            // thread slide under it instead of being cut at a hard edge.
            Positioned(
              left: 0,
              right: 0,
              bottom: 0,
              height: HarborSpace.s4,
              child: IgnorePointer(
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    gradient: LinearGradient(
                      begin: Alignment.topCenter,
                      end: Alignment.bottomCenter,
                      colors: [
                        t.colors.canvas.withValues(alpha: 0),
                        t.colors.canvas
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ]),
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
      // Reading the request: a quiet placeholder where the reply will be.
      outcome = HarborSkeleton.lines(2, key: const ValueKey('home-routing'));
    } else if (entry.notice case final notice?) {
      outcome = _noticeBanner(l10n, notice);
    } else if (entry.route case final route? when service != null) {
      outcome = _TaskCard(
        decision: route,
        report: entry.report,
        skill: entry.skill,
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
      padding: const EdgeInsets.symmetric(vertical: HarborSpace.s4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          QuestionHeading(text: entry.request),
          if (outcome != null) ...[
            const SizedBox(height: HarborSpace.s4),
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

  /// The empty conversation: one headline, the centred ask box, a few
  /// suggestions — and, only when there is something to say, the first-run
  /// banner, a core failure or work running in the background.
  Widget _dashboard(
      AppLocalizations l10n, HarborTheme t, HarborService? service) {
    final wc = HarborBreakpoints.of(context);
    final compact = HarborBreakpoints.isCompact(wc);
    final gutter = HarborBreakpoints.gutter(wc);
    final sp = HarborServiceProvider.of(context);
    final ops = service?.activeOps ?? const [];
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
          avatar: Icon(icon, size: 16, color: t.colors.inkMuted),
          label: Text(label, style: t.text.smallOf(t.colors.ink)),
          shape: const StadiumBorder(),
          side: BorderSide(color: t.colors.borderSubtle),
          backgroundColor: t.colors.canvas,
          visualDensity: VisualDensity.compact,
          padding: const EdgeInsetsDirectional.fromSTEB(
              HarborSpace.s2, 0, HarborSpace.s3, 0),
          onPressed: () {
            HapticFeedback.selectionClick();
            _prefill(label);
          },
        ),
    ];

    final column = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(l10n.homeHeadline,
            key: const ValueKey('home-headline'),
            textAlign: TextAlign.center,
            style: compact
                ? t.text.titleOf(t.colors.ink)
                : t.text.displayOf(t.colors.ink)),
        SizedBox(height: compact ? HarborSpace.s5 : HarborSpace.s6),
        // First run (production plan C2): a model install gates every
        // model-backed action; say so once, plainly, with the Local Only
        // fact next to it, and take the user straight to Recommended.
        if (_needsFirstModel(context)) ...[
          _FirstRunCard(
            key: const ValueKey('home-first-run'),
            title: l10n.homeFirstRunTitle,
            body: l10n.homeFirstRunBody,
            action: FilledButton.icon(
              key: const ValueKey('home-first-run-install'),
              onPressed: () => widget.state.openModels(ModelsTab.recommended),
              icon: const Icon(Icons.download_rounded, size: 18),
              label: Text(l10n.homeFirstRunAction),
            ),
          ),
          const SizedBox(height: HarborSpace.s4),
        ],
        if (sp.failed) ...[
          HarborBanner(
            key: const ValueKey('home-core-failed'),
            tone: HarborBannerTone.danger,
            title: l10n.coreDegradedTitle,
            body: l10n.coreNotLoadedActivity,
            action: sp.failureDetail == null
                ? null
                : HarborIdentifier(sp.failureDetail!, size: 11, maxLines: 6),
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
              alignment: WrapAlignment.center,
              spacing: HarborSpace.s2,
              runSpacing: HarborSpace.s2,
              children: chips),
        if (ops.isNotEmpty) ...[
          const SizedBox(height: HarborSpace.s6),
          HarborSectionHeader(title: l10n.homeSectionActive),
          for (final op in ops)
            Padding(
              padding: const EdgeInsets.only(bottom: HarborSpace.s2),
              child: OpProgressCard(progress: op, service: service),
            ),
        ],
      ],
    );

    // Centred in the free space; scrolls when the keyboard or large text
    // leaves too little room.
    return LayoutBuilder(
      builder: (context, box) => SingleChildScrollView(
        controller: _scroll,
        keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
        padding:
            EdgeInsets.symmetric(horizontal: gutter, vertical: HarborSpace.s5),
        child: ConstrainedBox(
          constraints: BoxConstraints(
              minHeight: math.max(0, box.maxHeight - HarborSpace.s5 * 2)),
          child: Align(
            // A little above centre reads as "the page", not a dialog.
            alignment: const Alignment(0, -0.25),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: HarborLayout.askMax),
              child: column,
            ),
          ),
        ),
      ),
    );
  }
}

/// A routed task in the conversation: which skill's sheet was opened (or
/// why none was picked, with choices) and, once the sheet started a run,
/// the run as a result card — the skill, its state, what it produced
/// (readable, not JSON) and the way back into the sheet or to the run.
class _TaskCard extends StatelessWidget {
  const _TaskCard({
    required this.decision,
    required this.report,
    required this.skill,
    required this.onPick,
    required this.onReopen,
    required this.onBrowse,
    required this.onViewRun,
  });
  final RouteDecision decision;
  final Map<String, dynamic>? report;
  final SkillSummary? skill;
  final ValueChanged<SkillSummary> onPick;
  final VoidCallback? onReopen;
  final VoidCallback onBrowse;
  final VoidCallback onViewRun;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final runId = report?['run_id'] as String?;
    final state = report?['state'] as String?;
    final outputs = (report?['status'] as Map?)?['outputs'];
    final picked = skill ??
        (decision is RouteSkill ? (decision as RouteSkill).skill : null);
    final reopen = onReopen ?? (picked == null ? null : () => onPick(picked));
    // Once a run exists the choices are history: the run is the answer.
    final showRoute = runId == null || decision is RouteSkill;
    return Column(
      key: const ValueKey('home-task'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (showRoute)
          RoutePanel(decision: decision, onPick: onPick, onBrowse: onBrowse),
        if (runId != null) ...[
          if (showRoute) const SizedBox(height: HarborSpace.s3),
          HarborCard(
            key: const ValueKey('home-task-result'),
            padding: const EdgeInsets.all(HarborSpace.s4),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(children: [
                  Container(
                    width: 32,
                    height: 32,
                    decoration: BoxDecoration(
                      color: t.colors.brandSoft,
                      borderRadius: BorderRadius.circular(HarborRadius.sm),
                    ),
                    child: Icon(Icons.account_tree_outlined,
                        size: 18, color: t.colors.brand),
                  ),
                  const SizedBox(width: HarborSpace.s3),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        if (picked != null)
                          // Skill titles are data from the core (English).
                          Text(picked.title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: t.text.bodyStrongOf(t.colors.ink)),
                        HarborIdentifier(runId,
                            size: 11, color: t.colors.inkMuted),
                      ],
                    ),
                  ),
                  if (state != null) RunStateBadge(state: state),
                ]),
                if (outputs is Map && outputs.isNotEmpty) ...[
                  const SizedBox(height: HarborSpace.s3),
                  Divider(height: 1, color: t.colors.borderSubtle),
                  const SizedBox(height: HarborSpace.s3),
                  SkillOutputView(outputs: outputs, preview: true),
                ],
                const SizedBox(height: HarborSpace.s2),
                Wrap(
                  spacing: HarborSpace.s2,
                  runSpacing: HarborSpace.s1,
                  alignment: WrapAlignment.end,
                  children: [
                    TextButton.icon(
                      key: const ValueKey('home-task-view-run'),
                      onPressed: onViewRun,
                      icon: const Icon(Icons.timeline_outlined, size: 16),
                      label: Text(l10n.homeTaskViewRun),
                    ),
                    if (reopen != null)
                      FilledButton.tonalIcon(
                        key: const ValueKey('home-task-reopen'),
                        onPressed: reopen,
                        icon: const Icon(Icons.open_in_new, size: 16),
                        label: Text(l10n.homeTaskReopen),
                      ),
                  ],
                ),
              ],
            ),
          ),
        ] else if (onReopen != null) ...[
          const SizedBox(height: HarborSpace.s1),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: TextButton.icon(
              key: const ValueKey('home-task-reopen'),
              onPressed: onReopen,
              icon: const Icon(Icons.open_in_new, size: 16),
              label: Text(l10n.homeTaskReopen),
            ),
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
    // Phones keep the ask box to one row of tools: the model is an icon
    // (its name in the tooltip and the menu).
    final compact = HarborBreakpoints.isCompact(HarborBreakpoints.of(context));
    return MenuAnchor(
      builder: (context, controller, _) {
        void toggle() =>
            controller.isOpen ? controller.close() : controller.open();
        if (compact) {
          // Phones: a sheet from the bottom edge, not a menu squeezed
          // against the screen edge.
          return IconButton(
            key: const ValueKey('home-model-picker'),
            tooltip: '${l10n.askModelPicker} · ${selected ?? ''}',
            onPressed: () => _showSheet(context),
            icon: const Icon(Icons.memory_outlined),
          );
        }
        return ModelDock.chip(
          key: const ValueKey('home-model-picker'),
          modelLabel: selected ?? l10n.askModelPicker,
          runtimeLabel: runtimeLabel,
          semantic: ExecutionSemantic.local,
          onTap: toggle,
        );
      },
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
            onPressed: () {
              HapticFeedback.selectionClick();
              onSelected(id);
            },
            child: HarborIdentifier(id, size: 12),
          ),
      ],
    );
  }

  Future<void> _showSheet(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return showModalBottomSheet<void>(
      context: context,
      useSafeArea: true,
      showDragHandle: true,
      builder: (sheet) => SafeArea(
        top: false,
        child: Column(
          key: const ValueKey('home-model-sheet'),
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(
                  HarborSpace.s5, 0, HarborSpace.s5, HarborSpace.s1),
              child:
                  Text(l10n.askModelPicker, style: t.text.h2Of(t.colors.ink)),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(
                  HarborSpace.s5, 0, HarborSpace.s5, HarborSpace.s2),
              child:
                  Text(runtimeLabel, style: t.text.smallOf(t.colors.inkMuted)),
            ),
            for (final id in models)
              ListTile(
                contentPadding:
                    const EdgeInsets.symmetric(horizontal: HarborSpace.s5),
                minTileHeight: 52,
                leading: Icon(
                  id == selected
                      ? Icons.radio_button_checked
                      : Icons.radio_button_unchecked,
                  color: id == selected ? t.colors.brand : t.colors.inkMuted,
                ),
                title: HarborIdentifier(id, size: 13),
                selected: id == selected,
                onTap: () {
                  HapticFeedback.selectionClick();
                  onSelected(id);
                  Navigator.of(sheet).pop();
                },
              ),
            const SizedBox(height: HarborSpace.s3),
          ],
        ),
      ),
    );
  }
}

/// First run: one calm card under the headline — what is missing, why,
/// and the one action that fixes it. Not an alert: nothing is wrong yet.
class _FirstRunCard extends StatelessWidget {
  const _FirstRunCard(
      {super.key,
      required this.title,
      required this.body,
      required this.action});
  final String title;
  final String body;
  final Widget action;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return HarborCard(
      padding: const EdgeInsets.all(HarborSpace.s4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Container(
              width: 36,
              height: 36,
              decoration: BoxDecoration(
                color: t.colors.brandSoft,
                borderRadius: BorderRadius.circular(HarborRadius.sm),
              ),
              child:
                  Icon(Icons.memory_outlined, size: 20, color: t.colors.brand),
            ),
            const SizedBox(width: HarborSpace.s3),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(title, style: t.text.bodyStrongOf(t.colors.ink)),
                  const SizedBox(height: HarborSpace.s1),
                  Text(body,
                      style: t.text
                          .smallOf(t.colors.inkMuted)
                          .copyWith(height: 1.45)),
                ],
              ),
            ),
          ]),
          const SizedBox(height: HarborSpace.s3),
          Align(alignment: AlignmentDirectional.centerEnd, child: action),
        ],
      ),
    );
  }
}

/// A file attached to the request: a small card with the file's type, its
/// name (data, shown as is) and a remove action — the request's file at a
/// glance, not a filter chip.
class _FileCard extends StatelessWidget {
  const _FileCard({
    super.key,
    required this.name,
    required this.office,
    required this.removeTooltip,
    required this.onRemove,
  });
  final String name;
  final bool office;
  final String removeTooltip;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final dot = name.lastIndexOf('.');
    final ext = dot > 0 && dot < name.length - 1
        ? name.substring(dot + 1).toUpperCase()
        : '';
    return Container(
      constraints: const BoxConstraints(maxWidth: 260, minHeight: 48),
      padding: const EdgeInsetsDirectional.fromSTEB(
          HarborSpace.s2, HarborSpace.s1, 0, HarborSpace.s1),
      decoration: BoxDecoration(
        color: t.colors.canvas,
        borderRadius: BorderRadius.circular(HarborRadius.md),
        border: Border.all(color: t.colors.border),
      ),
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        Container(
          width: 32,
          height: 32,
          decoration: BoxDecoration(
            color: t.colors.brandSoft,
            borderRadius: BorderRadius.circular(HarborRadius.sm),
          ),
          child: Icon(
              office
                  ? Icons.description_outlined
                  : Icons.insert_drive_file_outlined,
              size: 18,
              color: t.colors.brand),
        ),
        const SizedBox(width: HarborSpace.s2),
        Flexible(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: t.text
                      .smallOf(t.colors.ink)
                      .copyWith(fontWeight: FontWeight.w600)),
              if (ext.isNotEmpty)
                Text(ext, style: t.text.captionOf(t.colors.inkMuted)),
            ],
          ),
        ),
        IconButton(
          tooltip: removeTooltip,
          onPressed: onRemove,
          iconSize: 18,
          icon: const Icon(Icons.close_rounded),
        ),
      ]),
    );
  }
}
