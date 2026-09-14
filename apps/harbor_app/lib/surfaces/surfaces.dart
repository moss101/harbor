import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import 'knowledge_ingest.dart';

export 'ask_surface.dart';
export 'work_surface.dart';

/// Home answers "What do you want to get done?" (goal §21): composer plus
/// work-phrased quick actions, never "chat with model X". Quick actions
/// prefill the composer with their work phrasing; the attach action
/// routes real files into the local knowledge index.
class HomeSurface extends StatefulWidget {
  const HomeSurface({super.key, required this.state});
  final AppState state;

  @override
  State<HomeSurface> createState() => _HomeSurfaceState();
}

class _HomeSurfaceState extends State<HomeSurface> {
  final _composer = TextEditingController();
  final _composerFocus = FocusNode();

  @override
  void dispose() {
    _composer.dispose();
    _composerFocus.dispose();
    super.dispose();
  }

  void _prefill(String label) {
    _composer.text = label;
    _composerFocus.requestFocus();
  }

  Future<void> _attachFiles() async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null) return;
    final l10n = AppLocalizations.of(context)!;
    const group = XTypeGroup(
      label: 'Documents',
      extensions: ['txt', 'md', 'csv', 'json', 'log', 'docx', 'pdf'],
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

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final quick = [
      (l10n.quickSummarize, Icons.description_outlined),
      (l10n.quickAnalyze, Icons.table_chart_outlined),
      (l10n.quickPresent, Icons.slideshow_outlined),
      (l10n.quickCompare, Icons.difference_outlined),
      (l10n.quickOrganize, Icons.folder_open_outlined),
      (l10n.quickResearch, Icons.travel_explore_outlined),
      (l10n.quickTranslate, Icons.translate_outlined),
    ];
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: HarborLayout.contentMax),
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(HarborSpace.s8),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(l10n.homeHeadline, style: t.text.titleOf(t.colors.ink)),
              const SizedBox(height: HarborSpace.s4),
              Composer(
                hint: l10n.homeComposerHint,
                controller: _composer,
                focusNode: _composerFocus,
                onAttach: _attachFiles,
                onSubmit: (text) async {
                  final sp = HarborServiceProvider.of(context);
                  final service = sp.notifier;
                  if (service == null) return;
                  final messenger = ScaffoldMessenger.of(context);
                  final createdLine = l10n.runStateLine('CREATED', 0);
                  final runId = await service.submitRequest(text);
                  if (runId != null && mounted) {
                    messenger.showSnackBar(SnackBar(
                      content: Text(createdLine),
                    ));
                  }
                },
              ),
              const SizedBox(height: HarborSpace.s6),
              Wrap(
                spacing: HarborSpace.s2,
                runSpacing: HarborSpace.s2,
                children: [
                  for (final (label, icon) in quick)
                    ActionChip(
                      avatar: Icon(icon, size: 16, color: t.colors.brand),
                      label: Text(label),
                      onPressed: () => _prefill(label),
                    ),
                ],
              ),
              const SizedBox(height: HarborSpace.s8),
              Builder(builder: (context) {
                final l10n = AppLocalizations.of(context)!;
                final sp = HarborServiceProvider.of(context);
                final service = sp.notifier;
                if (sp.failed || service == null) {
                  return ModelDock(
                    modelLabel: l10n.modelDockCoreUnavailable,
                    runtimeLabel: 'OFFLINE',
                    semantic: ExecutionSemantic.danger,
                  );
                }
                final models = service.installedModels;
                final policy = service.policy;
                final runtimeLabel =
                    policy.contains('LOCAL_ONLY') ? 'LOCAL ONLY' : 'LOCAL';
                if (models.isEmpty) {
                  return ModelDock(
                    modelLabel: l10n.modelDockEmpty,
                    runtimeLabel: runtimeLabel,
                    semantic: ExecutionSemantic.local,
                    // The dock is a navigation affordance to Models.
                    onTap: () => widget.state.selectSurface(4),
                  );
                }
                return ModelDock(
                  modelLabel: models.first['id'] as String,
                  runtimeLabel: runtimeLabel,
                  semantic: ExecutionSemantic.local,
                  onTap: () => widget.state.selectSurface(4),
                );
              }),
            ],
          ),
        ),
      ),
    );
  }
}

/// The composer accepts text plus file/context attachments. The attach
/// action is real: picked documents are routed into the local knowledge
/// index (extraction + indexing happen in the core).
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
    _controller.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    // Only dispose a controller the composer itself created.
    if (widget.controller == null) _ownedController?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final l10n = AppLocalizations.of(context)!;
    return Container(
      padding: const EdgeInsets.symmetric(
          horizontal: HarborSpace.s4, vertical: HarborSpace.s2),
      decoration: ShapeDecoration(
        color: t.colors.surface,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(HarborRadius.lg),
          side: BorderSide(color: t.colors.border),
        ),
      ),
      child: Row(children: [
        Expanded(
          child: TextField(
            controller: _controller,
            focusNode: widget.focusNode,
            minLines: 1,
            maxLines: 4,
            decoration: InputDecoration(
                hintText: widget.hint, border: InputBorder.none),
          ),
        ),
        IconButton(
          tooltip: l10n.attachFilesTooltip,
          onPressed: widget.onAttach,
          icon: const Icon(Icons.attach_file_outlined),
        ),
        const SizedBox(width: HarborSpace.s2),
        FilledButton.icon(
          onPressed: _controller.text.isEmpty
              ? null
              : () {
                  widget.onSubmit?.call(_controller.text);
                  _controller.clear();
                  setState(() {});
                },
          icon: const Icon(Icons.arrow_forward),
          label: Text(l10n.sendAction),
        ),
      ]),
    );
  }
}

/// Models surface (goal §23): Recommended / Library / Hugging Face /
/// Installed / Import / Benchmark with Fit Score first. Acquisition and
/// local import run as cancellable background ops with live progress.
class ModelsSurface extends StatelessWidget {
  const ModelsSurface({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return DefaultTabController(
      length: 5,
      child: Column(children: [
        TabBar(tabs: [
          Tab(text: l10n.modelsRecommended),
          Tab(text: l10n.modelsLibrary),
          Tab(text: l10n.modelsHuggingFace),
          Tab(text: l10n.modelsInstalled),
          Tab(text: l10n.modelsBenchmark),
        ]),
        Expanded(
          child: TabBarView(children: [
            _recommendations(context),
            HarborEmptyState(
                title: l10n.modelsLibrary, body: l10n.modelsLibraryEmpty),
            _HfSearchView(),
            Builder(builder: (context) {
              final sp = HarborServiceProvider.of(context);
              final service = sp.notifier;
              if (sp.failed || service == null) {
                return HarborErrorState(
                  message: l10n.coreNotLoadedModels,
                );
              }
              final models = service.installedModels;
              if (models.isEmpty) {
                return HarborEmptyState(
                    title: l10n.modelsInstalled,
                    body: l10n.modelsInstalledEmpty,
                    actionLabel: l10n.modelsRecommended,
                    // Real navigation: the Recommended tab is one tap away.
                    onAction: () {
                      DefaultTabController.of(context).animateTo(0);
                    });
              }
              return ListView(
                padding: const EdgeInsets.all(HarborSpace.s4),
                children: [
                  for (final m in models)
                    Card(
                      margin: const EdgeInsets.only(bottom: HarborSpace.s2),
                      child: ListTile(
                        title: Text(m['id'] as String),
                        subtitle: Text(l10n.filesSizeRuntime(
                          m['files'] as int,
                          (m['total_bytes'] as int) ~/ (1024 * 1024),
                          m['runtime'] as String,
                        )),
                        trailing: _FitBadge(packageId: m['id'] as String),
                      ),
                    ),
                ],
              );
            }),
            HarborEmptyState(
                title: l10n.modelsBenchmark, body: l10n.modelsBenchmarkEmpty),
          ]),
        ),
      ]),
    );
  }

  Widget _recommendations(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return ListView(
      padding: const EdgeInsets.all(HarborSpace.s4),
      children: [
        Text(l10n.fitScore, style: t.text.captionOf(t.colors.inkMuted)),
        const SizedBox(height: HarborSpace.s2),
        const FitScoreBadge(band: FitBand.limited, reasons: [
          'close to the memory comfort limit at this context',
          'CPU-only execution',
        ]),
        const SizedBox(height: HarborSpace.s4),
        HarborEmptyState(
            title: l10n.modelsRecommended, body: l10n.modelsRecommendedEmpty),
      ],
    );
  }
}

/// Fit Score for one installed model, computed on the worker isolate.
class _FitBadge extends StatefulWidget {
  const _FitBadge({required this.packageId});
  final String packageId;

  @override
  State<_FitBadge> createState() => _FitBadgeState();
}

class _FitBadgeState extends State<_FitBadge> {
  Map<String, dynamic>? _fit;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_fit == null) {
      final service = HarborServiceProvider.of(context).notifier;
      service?.fitScore(widget.packageId).then((fit) {
        if (mounted) setState(() => _fit = fit);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final fit = _fit;
    if (fit == null) return const SizedBox.shrink();
    final band = switch (fit['band']) {
      'excellent' => FitBand.excellent,
      'good' => FitBand.good,
      'limited' => FitBand.limited,
      'unsupported' => FitBand.unsupported,
      _ => FitBand.tooLarge,
    };
    return FitScoreBadge(
        band: band, reasons: (fit['reasons'] as List).cast<String>());
  }
}

/// Acquisition progress line for the HF tab (real bytes, real cancel).
class _AcquireProgressView extends StatelessWidget {
  const _AcquireProgressView({required this.progress, required this.onCancel});

  final Map<String, dynamic> progress;
  final VoidCallback onCancel;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final phase = (progress['phase'] as String?) ?? '';
    final detail = (progress['detail'] as String?) ?? '';
    final bytesDone = (progress['bytes_done'] as num?)?.toInt() ?? 0;
    final bytesTotal = (progress['bytes_total'] as num?)?.toInt() ?? 0;
    final label = switch (phase) {
      'resolving' => l10n.opResolving,
      'verifying' => l10n.opVerifying,
      'installing' => l10n.opInstalling,
      _ => l10n.modelInstalling,
    };
    final byteLine = bytesTotal > 0
        ? l10n.opBytesMib(
            bytesDone ~/ (1024 * 1024), bytesTotal ~/ (1024 * 1024))
        : l10n.opDownloading(bytesDone ~/ (1024 * 1024), 0);
    return Card(
      margin: const EdgeInsets.symmetric(horizontal: HarborSpace.s4),
      child: Padding(
        padding: const EdgeInsets.all(HarborSpace.s3),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            Expanded(
              child:
                  Text('$label $detail', style: t.text.smallOf(t.colors.ink)),
            ),
            IconButton(
              tooltip: l10n.cancelAction,
              onPressed: onCancel,
              icon: const Icon(Icons.close),
            ),
          ]),
          const SizedBox(height: HarborSpace.s2),
          LinearProgressIndicator(
              value: bytesTotal > 0 ? bytesDone / bytesTotal : null),
          const SizedBox(height: HarborSpace.s2),
          Text(byteLine, style: t.text.captionOf(t.colors.inkMuted)),
        ]),
      ),
    );
  }
}

class AgentsSurface extends StatelessWidget {
  const AgentsSurface({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    // Honest disabled state: agent orchestration is not a shipped
    // capability of this release, so the surface says exactly that
    // instead of showing a button that would do nothing.
    return HarborEmptyState(
      title: l10n.agentsEmptyTitle,
      body: l10n.agentsUnavailableBody,
    );
  }
}

class SkillsSurface extends StatelessWidget {
  const SkillsSurface({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return Builder(builder: (context) {
      final sp = HarborServiceProvider.of(context);
      final service = sp.notifier;
      if (sp.failed || service == null) {
        return HarborErrorState(
          message: l10n.coreNotLoadedSkills,
        );
      }
      final skills = service.skills;
      return ListView(
        padding: const EdgeInsets.all(HarborSpace.s4),
        children: [
          Text(l10n.skillsEmptyBody, style: t.text.smallOf(t.colors.inkMuted)),
          const SizedBox(height: HarborSpace.s4),
          for (final s in skills)
            Card(
              margin: const EdgeInsets.only(bottom: HarborSpace.s2),
              child: ListTile(
                title: Text(s.title),
                subtitle: Text(s.description,
                    maxLines: 2, overflow: TextOverflow.ellipsis),
                trailing: Text(l10n.toolsCount(s.tools.length),
                    style: t.text.captionOf(t.colors.inkMuted)),
              ),
            ),
        ],
      );
    });
  }
}

/// Knowledge surface: the durable local index with real source
/// management — add files / paste text (background ingest with live
/// progress), inspect sources, remove sources, index identity.
class KnowledgeSurface extends StatefulWidget {
  const KnowledgeSurface({super.key});

  @override
  State<KnowledgeSurface> createState() => _KnowledgeSurfaceState();
}

class _KnowledgeSurfaceState extends State<KnowledgeSurface> {
  bool _opening = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _ensureOpen());
  }

  Future<void> _ensureOpen() async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null || service.knowledgeOpen || _opening) return;
    _opening = true;
    await service.openKnowledge();
    _opening = false;
  }

  Future<void> _addFiles() async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null || !mounted) return;
    final l10n = AppLocalizations.of(context)!;
    final outcome = await pickAndIngest(context, service);
    if (outcome == null || !mounted) return;
    showIngestFeedback(context, outcome, l10n);
  }

  Future<void> _pasteText() async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null || !mounted) return;
    final l10n = AppLocalizations.of(context)!;
    final titleController = TextEditingController();
    final bodyController = TextEditingController();
    final accepted = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l10n.knowledgeAddTextTitle),
        content: Column(mainAxisSize: MainAxisSize.min, children: [
          TextField(
            controller: titleController,
            decoration:
                InputDecoration(hintText: l10n.knowledgeAddTextTitleHint),
          ),
          const SizedBox(height: HarborSpace.s2),
          TextField(
            controller: bodyController,
            maxLines: 8,
            decoration:
                InputDecoration(hintText: l10n.knowledgeAddTextBodyHint),
          ),
        ]),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(l10n.cancelAction),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(l10n.knowledgeAddTextConfirm),
          ),
        ],
      ),
    );
    final text = bodyController.text;
    final explicitTitle = titleController.text.trim();
    titleController.dispose();
    bodyController.dispose();
    if (accepted != true || text.trim().isEmpty || !mounted) return;
    // The pasted source gets a durable, unique id; the user's title wins,
    // otherwise the first line stands in as the title.
    final firstLine = text.split('\n').first.trim();
    final sourceTitle = explicitTitle.isNotEmpty
        ? explicitTitle
        : (firstLine.isEmpty ? l10n.knowledgeAddTextTitle : firstLine);
    final result = await service.ingestSources([
      {
        'id': 'pasted-${HarborService.newRunId()}',
        'title': sourceTitle,
        'text': text,
      },
    ]);
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(
      content: Text(result == null
          ? l10n.knowledgeIngestFailed
          : l10n.knowledgeSourceAdded(sourceTitle)),
    ));
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return Builder(builder: (context) {
      final sp = HarborServiceProvider.of(context);
      final service = sp.notifier;
      if (sp.failed || service == null) {
        return HarborErrorState(message: l10n.coreNotLoadedSkills);
      }
      if (!service.knowledgeOpen) {
        return HarborEmptyState(
          title: l10n.surfaceKnowledge,
          body: l10n.knowledgeOpenNeedsModel,
          actionLabel: l10n.addSources,
          onAction: _addFiles,
        );
      }
      final sources = service.knowledgeSources;
      final ingest = service.kindProgress['ingest'];
      return Column(children: [
        Padding(
          padding: const EdgeInsets.all(HarborSpace.s4),
          child: Row(children: [
            FilledButton.icon(
              onPressed: _addFiles,
              icon: const Icon(Icons.note_add_outlined),
              label: Text(l10n.addSources),
            ),
            const SizedBox(width: HarborSpace.s2),
            TextButton.icon(
              onPressed: _pasteText,
              icon: const Icon(Icons.content_paste_outlined),
              label: Text(l10n.knowledgeAddTextAction),
            ),
            const Spacer(),
            if (service.knowledgeDimension > 0)
              Text(
                l10n.knowledgeChunksCount(sources.fold<int>(
                    0, (sum, s) => sum + (s['chunks'] as int))),
                style: t.text.captionOf(t.colors.inkMuted),
              ),
          ]),
        ),
        if (ingest != null)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: HarborSpace.s4),
            child: _IngestProgressView(progress: ingest),
          ),
        Expanded(
          child: sources.isEmpty && ingest == null
              ? HarborEmptyState(
                  title: l10n.surfaceKnowledge,
                  body: l10n.knowledgeNoSources,
                  actionLabel: l10n.addSources,
                  onAction: _addFiles,
                )
              : ListView(
                  padding: const EdgeInsets.all(HarborSpace.s4),
                  children: [
                    for (final s in sources)
                      Card(
                        margin: const EdgeInsets.only(bottom: HarborSpace.s2),
                        child: ListTile(
                          leading: const Icon(Icons.article_outlined),
                          title: Text(s['title'] as String),
                          subtitle: Text(
                            l10n.knowledgeChunksCount(s['chunks'] as int),
                          ),
                          trailing: IconButton(
                            tooltip: l10n.knowledgeRemoveAction,
                            icon: const Icon(Icons.delete_outline),
                            onPressed: () async {
                              await service.removeKnowledgeSource(
                                  s['source_id'] as String);
                              if (context.mounted) {
                                ScaffoldMessenger.of(context).showSnackBar(
                                  SnackBar(
                                    content: Text(l10n.knowledgeRemoved(
                                        s['title'] as String)),
                                  ),
                                );
                              }
                            },
                          ),
                        ),
                      ),
                  ],
                ),
        ),
      ]);
    });
  }
}

/// Ingest progress for the Knowledge surface (real chunk counts).
class _IngestProgressView extends StatelessWidget {
  const _IngestProgressView({required this.progress});
  final Map<String, dynamic> progress;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final done = (progress['items_done'] as num?)?.toInt() ?? 0;
    final total = (progress['items_total'] as num?)?.toInt() ?? 0;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(HarborSpace.s3),
        child: Column(children: [
          Row(children: [
            Expanded(child: Text(l10n.knowledgeIngesting(done, total))),
            IconButton(
              tooltip: l10n.cancelAction,
              onPressed: () => HarborServiceProvider.of(context)
                  .notifier
                  ?.cancelOp(progress['op_id'] as String),
              icon: const Icon(Icons.close),
            ),
          ]),
          LinearProgressIndicator(value: total > 0 ? done / total : null),
          const SizedBox(height: HarborSpace.s2),
          Text(l10n.opIngesting, style: t.text.captionOf(t.colors.inkMuted)),
        ]),
      ),
    );
  }
}

class ActivitySurface extends StatelessWidget {
  const ActivitySurface({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return Builder(builder: (context) {
      final sp = HarborServiceProvider.of(context);
      final service = sp.notifier;
      if (sp.failed || service == null) {
        return HarborErrorState(
          message: l10n.coreNotLoadedActivity,
        );
      }
      final runs = service.runs;
      if (runs.isEmpty) {
        return HarborEmptyState(
          title: l10n.activityEmptyTitle,
          body: l10n.activityEmptyBody,
        );
      }
      return ListView(
        padding: const EdgeInsets.all(HarborSpace.s4),
        children: [
          for (final r in runs)
            Card(
              margin: const EdgeInsets.only(bottom: HarborSpace.s2),
              child: ListTile(
                title: Text(r['run_id'] as String),
                subtitle: Text(l10n.runStateLine(
                  r['state'] as String,
                  r['active_compute_ms_total'] as int,
                )),
                trailing: const Icon(Icons.chevron_right),
                onTap: () async {
                  final report = await service.replayRun(r['run_id'] as String);
                  if (report == null || !context.mounted) return;
                  showModalBottomSheet<void>(
                    context: context,
                    showDragHandle: true,
                    builder: (_) => Padding(
                      padding: const EdgeInsets.all(HarborSpace.s5),
                      child: RunTrail(
                        entries: [
                          for (final e in (report['trail'] as List).cast<Map>())
                            RunTrailEntry(e['summary'] as String,
                                icon: Icons.circle_outlined,
                                detail: 'seq ${e['seq']} · ${e['actor']}'),
                        ],
                      ),
                    ),
                  );
                },
              ),
            ),
        ],
      );
    });
  }
}

class SettingsSurface extends StatelessWidget {
  const SettingsSurface({super.key, required this.state});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    return ListView(
      padding: const EdgeInsets.all(HarborSpace.s4),
      children: [
        Text(l10n.settingsLanguage, style: t.text.h2Of(t.colors.ink)),
        RadioGroup<String>(
          groupValue: state.locale.languageCode,
          onChanged: (v) {
            if (v != null) state.setLocale(Locale(v));
          },
          child: Column(children: [
            RadioListTile<String>(
              title: Text(l10n.settingsEnglish),
              value: 'en',
            ),
            RadioListTile<String>(
              title: Text(l10n.settingsArabic),
              value: 'ar',
            ),
          ]),
        ),
        const Divider(),
        Text(l10n.settingsTheme, style: t.text.h2Of(t.colors.ink)),
        RadioGroup<ThemeMode>(
          groupValue: state.themeMode,
          onChanged: (v) {
            if (v != null) state.setThemeMode(v);
          },
          child: Column(children: [
            RadioListTile<ThemeMode>(
              title: Text(l10n.settingsThemeLight),
              value: ThemeMode.light,
            ),
            RadioListTile<ThemeMode>(
              title: Text(l10n.settingsThemeDark),
              value: ThemeMode.dark,
            ),
          ]),
        ),
        const Divider(),
        Text(l10n.settingsPrivacy, style: t.text.h2Of(t.colors.ink)),
        const SizedBox(height: HarborSpace.s2),
        TrustPulse(
          policyLabel: service?.policy ?? l10n.trustPolicy,
          executionLabel: l10n.trustExecutionOnDevice,
          executionSemantic: ExecutionSemantic.local,
        ),
        if (service?.deviceId != null) ...[
          const Divider(),
          Text(l10n.settingsIdentity, style: t.text.h2Of(t.colors.ink)),
          const SizedBox(height: HarborSpace.s2),
          Text(service!.deviceId ?? '',
              style: t.text.monoOf(t.colors.inkMuted, size: 12)),
        ],
      ],
    );
  }
}

/// Hugging Face discovery + acquisition through the brokered core path.
/// The query is acquisition metadata only; downloaded packages are hashed
/// and installed via the staged installer — as a real background op with
/// live progress and cancel (no fake "Installed" flips).
class _HfSearchView extends StatefulWidget {
  @override
  State<_HfSearchView> createState() => _HfSearchViewState();
}

class _HfSearchViewState extends State<_HfSearchView> {
  final _controller = TextEditingController();
  List<Map<String, dynamic>>? _results;
  bool _searching = false;
  bool _searchFailed = false;
  String? _installedId;
  String? _installError;
  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _search() async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null) return;
    setState(() => _searching = true);
    final results = await service.searchHuggingFace(_controller.text);
    if (!mounted) return;
    setState(() {
      _results = results;
      _searching = false;
      _searchFailed = results == null;
    });
  }

  /// Real acquisition: resolve the repo's GGUF files through the broker,
  /// then run the cancellable acquire op. Progress renders from the
  /// service's kind-tracked snapshot.
  Future<void> _install(String repoId) async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null || !mounted) return;
    final l10n = AppLocalizations.of(context)!;
    setState(() {
      _installError = null;
    });
    final files = await service.huggingFaceFiles(repoId);
    if (files.isEmpty) {
      setState(() => _installError = l10n.modelInstallFailed);
      return;
    }
    try {
      final result = await service.acquireModelHf(
        packageId: repoId.split('/').last,
        repoId: repoId,
        files: [
          for (final f in files)
            {
              'path': f['path'] as String,
              'role': 'weights',
              'sha256': '',
            },
        ],
      );
      if (!mounted) return;
      setState(() {
        if (result['installed'] != null) {
          _installedId = repoId;
        } else {
          _installError = l10n.modelInstallFailed;
        }
      });
    } on ffi.HarborCoreException catch (e) {
      if (!mounted) return;
      setState(() {
        _installError = e.message.toLowerCase().contains('cancel')
            ? l10n.modelInstallCancelled
            : l10n.modelInstallFailed;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (sp.failed || service == null) {
      return HarborErrorState(message: l10n.coreNotLoadedModels);
    }
    final acquiring = service.kindProgress['acquire'];
    return Column(children: [
      Padding(
        padding: const EdgeInsets.all(HarborSpace.s4),
        child: Row(children: [
          Expanded(
            child: TextField(
              controller: _controller,
              onSubmitted: (_) => _search(),
              decoration: InputDecoration(hintText: l10n.modelsHuggingFace),
            ),
          ),
          const SizedBox(width: HarborSpace.s2),
          IconButton(
              tooltip: l10n.askSearchTooltip,
              onPressed: _search,
              icon: const Icon(Icons.search)),
        ]),
      ),
      if (acquiring != null)
        _AcquireProgressView(
          progress: acquiring,
          onCancel: () => service.cancelOp(acquiring['op_id'] as String),
        ),
      Expanded(
        child: _searching
            ? const Center(child: CircularProgressIndicator())
            : _results == null
                ? HarborEmptyState(
                    title: l10n.modelsHuggingFace, body: l10n.modelsHfEmpty)
                : _results!.isEmpty
                    ? HarborEmptyState(
                        title: l10n.modelsHuggingFace,
                        body: _searchFailed
                            ? l10n.modelsHfSearchFailed
                            : l10n.askNoEvidenceTitle)
                    : ListView(
                        padding: const EdgeInsets.symmetric(
                            horizontal: HarborSpace.s4),
                        children: [
                          if (_installError != null)
                            Padding(
                              padding:
                                  const EdgeInsets.only(bottom: HarborSpace.s2),
                              child: Text(_installError!,
                                  style: TextStyle(
                                      color: HarborTheme.of(context)
                                          .colors
                                          .danger)),
                            ),
                          for (final m in _results!)
                            Card(
                              margin:
                                  const EdgeInsets.only(bottom: HarborSpace.s2),
                              child: ListTile(
                                title: Text(m['id'] as String),
                                subtitle: Text(
                                    'downloads: ${m['downloads']} · likes: ${m['likes']}'),
                                trailing: _installedId == m['id']
                                    ? StatusBadge(
                                        semantic: ExecutionSemantic.local,
                                        label: l10n.statusInstalled)
                                    : FilledButton(
                                        onPressed: () =>
                                            _install(m['id'] as String),
                                        child: Text(l10n.modelInstallAction),
                                      ),
                              ),
                            ),
                        ],
                      ),
      ),
    ]);
  }
}
