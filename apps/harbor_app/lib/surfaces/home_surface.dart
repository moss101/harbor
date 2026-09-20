import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import '../shell/keyboard.dart';
import '../widgets/ops.dart';
import '../widgets/trust.dart';
import 'knowledge_ingest.dart';

/// Home answers "What do you want to get done?" (goal §21): composer plus
/// work-phrased quick actions, never "chat with model X". Quick actions
/// prefill the composer with their work phrasing; the attach action
/// routes real files into the local knowledge index. Below the composer:
/// background work in progress, recent durable runs, the active model and
/// the knowledge index — all live facts from the core.
class HomeSurface extends StatefulWidget {
  const HomeSurface({super.key, required this.state});
  final AppState state;

  @override
  State<HomeSurface> createState() => _HomeSurfaceState();
}

class _HomeSurfaceState extends State<HomeSurface> {
  final _composer = TextEditingController();
  final _composerFocus = FocusNode();
  final _scroll = ScrollController();

  @override
  void initState() {
    super.initState();
    widget.state.addListener(_consumePending);
  }

  @override
  void dispose() {
    widget.state.removeListener(_consumePending);
    _composer.dispose();
    _composerFocus.dispose();
    _scroll.dispose();
    super.dispose();
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

  Future<void> _attachFiles() async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null) return;
    final l10n = AppLocalizations.of(context)!;
    final group = XTypeGroup(
      label: l10n.fileGroupDocuments,
      extensions: const ['txt', 'md', 'csv', 'json', 'log', 'docx', 'pdf'],
    );
    final List<XFile> files;
    try {
      files = await openFiles(acceptedTypeGroups: [group]);
    } catch (_) {
      return; // picker dismissed
    }
    if (files.isEmpty || !mounted) return;
    final outcome = await ingestFiles(service, files, l10n);
    if (!mounted) return;
    showIngestFeedback(context, outcome, l10n);
  }

  Future<void> _submit(String text) async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final l10n = AppLocalizations.of(context)!;
    final messenger = ScaffoldMessenger.of(context);
    if (service == null) {
      messenger.showSnackBar(SnackBar(content: Text(l10n.homeRequestFailed)));
      return;
    }
    final runId = await service.submitRequest(text);
    if (!mounted) return;
    messenger.showSnackBar(SnackBar(
      content:
          Text(runId == null ? l10n.homeRequestFailed : l10n.homeRequestQueued),
      action: runId == null
          ? null
          : SnackBarAction(
              label: l10n.homeOpenActivity,
              onPressed: () => widget.state.goTo(HarborSurface.activity),
            ),
    ));
  }

  String _greeting(AppLocalizations l10n) {
    final hour = DateTime.now().hour;
    if (hour < 12) return l10n.homeGreetingMorning;
    if (hour < 18) return l10n.homeGreetingAfternoon;
    return l10n.homeGreetingEvening;
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
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
              onPressed: () => widget.state.goTo(HarborSurface.models),
              icon: const Icon(Icons.recommend_outlined, size: 18),
              label: Text(l10n.homeFirstRunAction),
            ),
          ),
          const SizedBox(height: HarborSpace.s4),
        ],
        Composer(
          hint: l10n.homeComposerHint,
          controller: _composer,
          focusNode: _composerFocus,
          onAttach: _attachFiles,
          onSubmit: _submit,
        ),
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

/// The composer accepts text plus file/context attachments. The attach
/// action is real: picked documents are routed into the local knowledge
/// index (extraction + indexing happen in the core). Desktop: Enter
/// sends, Shift+Enter inserts a newline; mobile: the keyboard's send key.
class Composer extends StatefulWidget {
  const Composer({
    super.key,
    required this.hint,
    this.onSubmit,
    this.controller,
    this.focusNode,
    this.onAttach,
  });
  final String hint;

  /// Called with the submitted text (Home wires this to the runtime).
  final ValueChanged<String>? onSubmit;

  /// Optional external controller/focus so quick actions can prefill.
  final TextEditingController? controller;
  final FocusNode? focusNode;

  /// Attach action; when null the button renders visibly disabled.
  final VoidCallback? onAttach;

  @override
  State<Composer> createState() => _ComposerState();
}

class _ComposerState extends State<Composer> {
  TextEditingController? _ownedController;

  TextEditingController get _controller =>
      widget.controller ?? (_ownedController ??= TextEditingController());

  @override
  void initState() {
    super.initState();
    _controller.addListener(_onChanged);
  }

  void _onChanged() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _controller.removeListener(_onChanged);
    // Only dispose a controller the composer itself created.
    if (widget.controller == null) _ownedController?.dispose();
    super.dispose();
  }

  void _send() {
    final text = _controller.text.trim();
    if (text.isEmpty) return;
    widget.onSubmit?.call(text);
    _controller.clear();
  }

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final l10n = AppLocalizations.of(context)!;
    final compact = HarborBreakpoints.isCompact(HarborBreakpoints.of(context));
    final canSend = _controller.text.trim().isNotEmpty;
    final field = TextField(
      controller: _controller,
      focusNode: widget.focusNode,
      minLines: 1,
      maxLines: 6,
      textInputAction: harborHasKeyboardShortcuts
          ? TextInputAction.newline
          : TextInputAction.send,
      onSubmitted: (_) => _send(),
      style: t.text.bodyOf(t.colors.ink).copyWith(fontSize: 15),
      decoration: InputDecoration(
        hintText: widget.hint,
        border: InputBorder.none,
        enabledBorder: InputBorder.none,
        focusedBorder: InputBorder.none,
        filled: false,
        contentPadding: const EdgeInsets.symmetric(
            horizontal: HarborSpace.s2, vertical: HarborSpace.s3),
      ),
    );
    final input = harborHasKeyboardShortcuts
        ? CallbackShortcuts(
            bindings: {
              const SingleActivator(LogicalKeyboardKey.enter): _send,
              const SingleActivator(LogicalKeyboardKey.numpadEnter): _send,
            },
            child: field,
          )
        : field;
    return Container(
      padding: const EdgeInsets.fromLTRB(
          HarborSpace.s2, HarborSpace.s1, HarborSpace.s2, HarborSpace.s2),
      decoration: ShapeDecoration(
        color: t.colors.surface,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(HarborRadius.lg),
          side: BorderSide(color: t.colors.border),
        ),
        shadows: [
          BoxShadow(
            color: t.colors.ink.withValues(alpha: t.isDark ? 0.0 : 0.04),
            blurRadius: 12,
            offset: const Offset(0, 4),
          ),
        ],
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          input,
          Row(children: [
            IconButton(
              tooltip: l10n.attachFilesTooltip,
              onPressed: widget.onAttach,
              icon: const Icon(Icons.attach_file_outlined),
            ),
            const Spacer(),
            if (compact)
              IconButton.filled(
                tooltip: l10n.sendAction,
                onPressed: canSend ? _send : null,
                icon: const Icon(Icons.arrow_upward),
              )
            else
              FilledButton.icon(
                onPressed: canSend ? _send : null,
                icon: const Icon(Icons.arrow_forward, size: 16),
                label: Text(l10n.sendAction),
              ),
          ]),
        ],
      ),
    );
  }
}
