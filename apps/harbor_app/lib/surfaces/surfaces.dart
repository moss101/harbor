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
              Composer(hint: l10n.homeComposerHint),
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
                final sp = HarborServiceProvider.of(context);
                final service = sp.notifier;
                if (sp.failed || service == null) {
                  return const ModelDock(
                    modelLabel: 'Core unavailable — native runtime not loaded',
                    runtimeLabel: 'OFFLINE',
                    semantic: ExecutionSemantic.danger,
                  );
                }
                final models = service.installedModels;
                final policy = service.policy;
                if (models.isEmpty) {
                  return ModelDock(
                    modelLabel: 'No model installed — open Models to install one',
                    runtimeLabel: policy.contains('LOCAL_ONLY') ? 'LOCAL ONLY' : 'LOCAL',
                    semantic: ExecutionSemantic.local,
                  );
                }
                return ModelDock(
                  modelLabel: models.first['id'] as String,
                  runtimeLabel: policy.contains('LOCAL_ONLY') ? 'LOCAL ONLY' : 'LOCAL',
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
  const Composer({super.key, required this.hint});
  final String hint;

  @override
  State<Composer> createState() => _ComposerState();
}

class _ComposerState extends State<Composer> {
  final _controller = TextEditingController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
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
          tooltip: 'Attach files',
          onPressed: () {},
          icon: const Icon(Icons.attach_file_outlined),
        ),
        const SizedBox(width: HarborSpace.s2),
        FilledButton.icon(
          onPressed: _controller.text.isEmpty ? null : () {},
          icon: const Icon(Icons.arrow_forward),
          label: const Text(''),
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
                body: 'Curated Harbor library packages appear here.'),
            HarborEmptyState(
                title: l10n.modelsHuggingFace,
                body: 'Search public repositories. Model packages are data — '
                    'no repository code ever executes.'),
            Builder(builder: (context) {
              final sp = HarborServiceProvider.of(context);
              final service = sp.notifier;
              if (sp.failed || service == null) {
                return HarborErrorState(
                  message: 'Native core not loaded; installed models are '
                      'unavailable. Build core/harbor_ffi to enable this view.',
                );
              }
              final models = service.installedModels;
              if (models.isEmpty) {
                return HarborEmptyState(
                    title: l10n.modelsInstalled,
                    body: 'Install a model from Recommended or the Library. '
                        'Fit Score shows what your device can run well.',
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
                        subtitle: Text(
                            '${m['files']} files · ${(m['total_bytes'] as int) ~/ (1024 * 1024)} MB · runtime ${m['runtime']}'),
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
                body: 'Controlled device-local benchmark workloads with '
                    'model/runtime/device identity.'),
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
            body: 'Recommendations appear once the catalog is synced. '
                'A model is recommended only when your device can run it well.'),
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
      actionLabel: 'New agent',
    );
  }
}

class SkillsSurface extends StatelessWidget {
  const SkillsSurface({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final skills = [
      ('Document Intelligence', Icons.description_outlined),
      ('Spreadsheet Analyst', Icons.table_chart_outlined),
      ('Presentation Builder', Icons.slideshow_outlined),
      ('PDF Research', Icons.picture_as_pdf_outlined),
      ('Research Synthesis', Icons.travel_explore_outlined),
      ('Meeting Notes', Icons.groups_outlined),
      ('Bilingual Writing', Icons.translate_outlined),
      ('Model Advisor', Icons.memory_outlined),
    ];
    final t = HarborTheme.of(context);
    return ListView(
      padding: const EdgeInsets.all(HarborSpace.s4),
      children: [
        Text(l10n.skillsEmptyBody, style: t.text.smallOf(t.colors.inkMuted)),
        const SizedBox(height: HarborSpace.s4),
        for (final (name, icon) in skills)
          Card(
            margin: const EdgeInsets.only(bottom: HarborSpace.s2),
            child: ListTile(
              leading: Icon(icon, color: t.colors.brand),
              title: Text(name),
              trailing: const Icon(Icons.chevron_right),
              onTap: () {},
            ),
          ),
      ],
    );
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
      actionLabel: 'Add sources',
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
          message: 'Native core not loaded; durable runs live in the core '
              'store and cannot be listed.',
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
                subtitle: Text('state: ${r['state']} · '
                    '${r['active_compute_ms_total']} ms executor time'),
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
