import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';

export 'ask_surface.dart';
export 'work_surface.dart';

/// Home answers "What do you want to get done?" (goal §21): composer plus
/// work-phrased quick actions, never "chat with model X".
class HomeSurface extends StatelessWidget {
  const HomeSurface({super.key, required this.state});
  final AppState state;

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
                onSubmit: (text) {
                  final sp = HarborServiceProvider.of(context);
                  final service = sp.notifier;
                  if (service == null) return;
                  final runId = service.submitRequest(text);
                  if (runId != null) service.refresh();
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
                      onPressed: () {},
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
                  );
                }
                return ModelDock(
                  modelLabel: models.first['id'] as String,
                  runtimeLabel: runtimeLabel,
                  semantic: ExecutionSemantic.local,
                );
              }),
            ],
          ),
        ),
      ),
    );
  }
}

/// The composer accepts text plus file/context attachments.
class Composer extends StatefulWidget {
  const Composer({super.key, required this.hint, this.onSubmit});
  final String hint;
  /// Called with the submitted text (Home wires this to the runtime).
  final ValueChanged<String>? onSubmit;

  @override
  State<Composer> createState() => _ComposerState();
}

class _ComposerState extends State<Composer> {
  final _controller = TextEditingController();

  @override
  void initState() {
    super.initState();
    _controller.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final l10n = AppLocalizations.of(context)!;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: HarborSpace.s4, vertical: HarborSpace.s2),
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
            minLines: 1,
            maxLines: 4,
            decoration: InputDecoration(
                hintText: widget.hint, border: InputBorder.none),
          ),
        ),
        IconButton(
          tooltip: l10n.attachFilesTooltip,
          onPressed: () {},
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
/// Installed / Import / Benchmark with Fit Score first.
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
                title: l10n.modelsLibrary,
                body: l10n.modelsLibraryEmpty),
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
                    actionLabel: l10n.modelsRecommended);
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
                        trailing: Builder(builder: (context) {
                          final fit = service.fitScore(m['id'] as String);
                          if (fit == null) return const SizedBox.shrink();
                          final band = switch (fit['band']) {
                            'excellent' => FitBand.excellent,
                            'good' => FitBand.good,
                            'limited' => FitBand.limited,
                            'unsupported' => FitBand.unsupported,
                            _ => FitBand.tooLarge,
                          };
                          return FitScoreBadge(
                              band: band,
                              reasons:
                                  (fit['reasons'] as List).cast<String>());
                        }),
                      ),
                    ),
                ],
              );
            }),
            HarborEmptyState(
                title: l10n.modelsBenchmark,
                body: l10n.modelsBenchmarkEmpty),
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
            title: l10n.modelsRecommended,
            body: l10n.modelsRecommendedEmpty),
      ],
    );
  }
}

class AgentsSurface extends StatelessWidget {
  const AgentsSurface({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return HarborEmptyState(
      title: l10n.agentsEmptyTitle,
      body: l10n.agentsEmptyBody,
      actionLabel: l10n.newAgent,
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

class KnowledgeSurface extends StatelessWidget {
  const KnowledgeSurface({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return HarborEmptyState(
      title: l10n.knowledgeEmptyTitle,
      body: l10n.knowledgeEmptyBody,
      actionLabel: l10n.addSources,
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
                onTap: () {
                  final report = service.replayRun(r['run_id'] as String);
                  if (report == null) return;
                  showModalBottomSheet<void>(
                    context: context,
                    showDragHandle: true,
                    builder: (_) => Padding(
                      padding: const EdgeInsets.all(HarborSpace.s5),
                      child: RunTrail(
                        entries: [
                          for (final e in (report['trail'] as List).cast<Map>())
                            RunTrailEntry(
                                e['summary'] as String,
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
    return ListView(
      padding: const EdgeInsets.all(HarborSpace.s4),
      children: [
        Text(l10n.settingsLanguage, style: t.text.h2Of(t.colors.ink)),
        RadioGroup<String>(
          groupValue: state.locale.languageCode,
          onChanged: (v) { if (v != null) state.setLocale(Locale(v)); },
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
          onChanged: (v) { if (v != null) state.setThemeMode(v); },
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
          policyLabel: l10n.trustPolicy,
          executionLabel: l10n.trustExecutionOnDevice,
          executionSemantic: ExecutionSemantic.local,
        ),
      ],
    );
  }
}

/// Hugging Face discovery + acquisition through the brokered core path.
/// The query is acquisition metadata only; downloaded packages are hashed
/// and installed via the staged installer.
class _HfSearchView extends StatefulWidget {
  @override
  State<_HfSearchView> createState() => _HfSearchViewState();
}

class _HfSearchViewState extends State<_HfSearchView> {
  final _controller = TextEditingController();
  List<Map<String, dynamic>>? _results;
  bool _searching = false;
  String? _installedId;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _search() {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null) return;
    setState(() => _searching = true);
    final results = service.searchHuggingFace(_controller.text);
    setState(() {
      _results = results;
      _searching = false;
    });
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (sp.failed || service == null) {
      return HarborErrorState(message: l10n.coreNotLoadedModels);
    }
    return Column(children: [
      Padding(
        padding: const EdgeInsets.all(HarborSpace.s4),
        child: Row(children: [
          Expanded(
            child: TextField(
              controller: _controller,
              onSubmitted: (_) => _search(),
              decoration:
                  InputDecoration(hintText: l10n.modelsHuggingFace),
            ),
          ),
          const SizedBox(width: HarborSpace.s2),
          IconButton(
              tooltip: l10n.askSearchTooltip,
              onPressed: _search,
              icon: const Icon(Icons.search)),
        ]),
      ),
      Expanded(
        child: _searching
            ? const Center(child: CircularProgressIndicator())
            : _results == null
                ? HarborEmptyState(
                    title: l10n.modelsHuggingFace,
                    body: l10n.modelsHfEmpty)
                : _results!.isEmpty
                    ? HarborEmptyState(
                        title: l10n.modelsHuggingFace,
                        body: l10n.askNoEvidenceTitle)
                    : ListView(
                        padding: const EdgeInsets.symmetric(
                            horizontal: HarborSpace.s4),
                        children: [
                          for (final m in _results!)
                            Card(
                              margin: const EdgeInsets.only(
                                  bottom: HarborSpace.s2),
                              child: ListTile(
                                title: Text(m['id'] as String),
                                subtitle: Text(
                                    'downloads: ${m['downloads']} · likes: ${m['likes']}'),
                                trailing: _installedId == m['id']
                                    ? StatusBadge(
                                        semantic: ExecutionSemantic.local,
                                        label: l10n.statusInstalled)
                                    : TextButton(
                                        onPressed: () {
                                          setState(() => _installedId = m['id'] as String);
                                        },
                                        child: Text(l10n.modelsInstalled),
                                      ),
                              ),
                            ),
                        ],
                      ),
      ),
    ]);
  }
}
