import 'package:flutter/material.dart';
import 'package:harbor_domain/harbor_domain.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import 'skill_run.dart';

/// Skills library (UX-026): the built-in skill definitions from the core,
/// filterable, grouped by family, with a detail sheet listing tools and —
/// for graph-bearing skills (decision 0006) — the graph facts and a Run
/// action. Prose skills are shown honestly as declarations: nothing in
/// the core executes them yet.
class SkillsSurface extends StatefulWidget {
  const SkillsSurface({super.key});

  @override
  State<SkillsSurface> createState() => _SkillsSurfaceState();
}

class _SkillsSurfaceState extends State<SkillsSurface> {
  final _filter = TextEditingController();
  final _suggest = TextEditingController();
  Map<String, dynamic>? _suggestion;
  bool _suggesting = false;
  AppState? _state;

  @override
  void initState() {
    super.initState();
    _filter.addListener(() => setState(() {}));
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final state = AppStateScope.maybeOf(context);
    if (!identical(state, _state)) {
      _state?.removeListener(_consumePending);
      _state = state;
      _state?.addListener(_consumePending);
    }
    _consumePending();
  }

  @override
  void dispose() {
    _state?.removeListener(_consumePending);
    _filter.dispose();
    _suggest.dispose();
    super.dispose();
  }

  /// Ask the router which skill fits a request. Recommend-only: the
  /// result opens a skill's detail sheet; running still goes through
  /// the skill's own run and approval path.
  Future<void> _runSuggest() async {
    final service = HarborServiceProvider.of(context).notifier;
    final text = _suggest.text.trim();
    if (service == null || text.isEmpty) return;
    setState(() => _suggesting = true);
    final preferred = AppStateScope.maybeOf(context)?.chatModel;
    final installed = [
      for (final m in service.installedModels) m['id'] as String
    ];
    final result = await service.suggestSkills(text,
        chatPackage: preferred != null && installed.contains(preferred)
            ? preferred
            : null);
    if (!mounted) return;
    setState(() {
      _suggesting = false;
      _suggestion = result ?? {'unavailable': true};
    });
  }

  Widget _suggestBar(AppLocalizations l10n, double gutter) {
    final t = HarborTheme.of(context);
    final all = HarborServiceProvider.of(context).notifier?.skills ??
        const <SkillSummary>[];
    final r = _suggestion;
    final ranked = (r?['ranked'] as List?)?.cast<Map>() ?? const [];
    final String? note = r == null
        ? null
        : r['unavailable'] == true
            ? l10n.skillsSuggestUnavailable
            : (r['llm_choice'] as Map?)?['status'] == 'chose'
                ? l10n.skillsSuggestLlmChose
                : r['abstained'] == true
                    ? switch (r['reason']) {
                        'ambiguous' => l10n.skillsSuggestAmbiguous,
                        'uncalibrated' => l10n.skillsSuggestUncalibrated,
                        _ => l10n.skillsSuggestNoMatch,
                      }
                    : l10n.skillsSuggestBest;
    return Padding(
      padding: EdgeInsets.fromLTRB(gutter, 0, gutter, HarborSpace.s3),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        TextField(
          key: const ValueKey('skills-suggest-field'),
          controller: _suggest,
          textInputAction: TextInputAction.search,
          onSubmitted: (_) => _runSuggest(),
          decoration: InputDecoration(
            hintText: l10n.skillsSuggestHint,
            prefixIcon: const Icon(Icons.auto_awesome_outlined),
            suffixIcon: _suggesting
                ? const Padding(
                    padding: EdgeInsets.all(12),
                    child: SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2)))
                : IconButton(
                    tooltip: l10n.skillsSuggestAction,
                    icon: const Icon(Icons.arrow_forward),
                    onPressed: _runSuggest),
          ),
        ),
        if (note != null) ...[
          const SizedBox(height: HarborSpace.s2),
          Text(note, style: t.text.smallOf(t.colors.inkMuted)),
        ],
        if (ranked.isNotEmpty) ...[
          const SizedBox(height: HarborSpace.s2),
          Wrap(spacing: HarborSpace.s2, runSpacing: HarborSpace.s1, children: [
            for (final h in ranked)
              ActionChip(
                label: Text(h['title'] as String? ?? ''),
                onPressed: () {
                  for (final s in all) {
                    if (s.id == h['skill_id']) {
                      _showDetail(s);
                      return;
                    }
                  }
                },
              ),
          ]),
          const SizedBox(height: HarborSpace.s1),
          Text(l10n.skillsSuggestNote,
              style: t.text.smallOf(t.colors.inkMuted)),
        ],
      ]),
    );
  }

  void _consumePending() {
    final q = _state?.pendingSkillsFilter;
    if (q == null) return;
    _state!.pendingSkillsFilter = null;
    _filter.text = q;
  }

  void _showDetail(SkillSummary s) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final compact = HarborBreakpoints.isCompact(HarborBreakpoints.of(context));
    final service = HarborServiceProvider.of(context).notifier;
    final graph = s.graph;
    final content = SafeArea(
      top: false,
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(HarborSpace.s5),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(children: [
              // Skill titles and descriptions are English-only data from
              // builtin_skills.json — nothing routes them through l10n.
              // Rendered with the ambient direction they inherit RTL in
              // Arabic, which puts the sentence period at the left edge
              // ("`.information from DOCX`"). Give the paragraph its own
              // direction, the way identifiers already get theirs. If a
              // skill's prose is ever localized, this has to become
              // conditional on the content.
              Expanded(
                  child: Text(s.title,
                      textDirection: TextDirection.ltr,
                      style: t.text.h2Of(t.colors.ink))),
              StatusBadge(
                semantic: s.runnable
                    ? ExecutionSemantic.local
                    : ExecutionSemantic.hybrid,
                icon: s.runnable
                    ? Icons.account_tree_outlined
                    : Icons.description_outlined,
                label:
                    s.runnable ? l10n.skillsRunnable : l10n.skillsDeclaration,
              ),
            ]),
            const SizedBox(height: HarborSpace.s2),
            Text(s.description,
                textDirection: TextDirection.ltr,
                style: t.text.bodyOf(t.colors.inkMuted)),
            const SizedBox(height: HarborSpace.s4),
            HarborKeyValue(label: l10n.skillsFamily, value: s.family),
            HarborKeyValue(label: 'ID', value: s.id, identifier: true),
            HarborKeyValue(label: 'Schema', value: s.schema, identifier: true),
            if (graph != null) ...[
              const SizedBox(height: HarborSpace.s3),
              Text(l10n.skillsGraphHeading,
                  style: t.text.captionOf(t.colors.inkMuted)),
              const SizedBox(height: HarborSpace.s2),
              HarborKeyValue(
                  label: l10n.skillsGraphNodes(graph.nodeCount),
                  value: l10n.skillsGraphModelNodes(graph.modelNodes)),
              HarborKeyValue(
                  label: 'v${graph.version}',
                  value: l10n.skillsGraphBudgets(
                      graph.maxSteps, graph.maxToolCalls)),
            ] else ...[
              const SizedBox(height: HarborSpace.s3),
              HarborBanner(
                tone: HarborBannerTone.info,
                dense: true,
                title: l10n.skillsDeclaration,
                body: l10n.skillsDeclarationBody,
              ),
            ],
            const SizedBox(height: HarborSpace.s3),
            Text(l10n.skillsToolsHeading,
                style: t.text.captionOf(t.colors.inkMuted)),
            const SizedBox(height: HarborSpace.s2),
            Wrap(
              spacing: HarborSpace.s2,
              runSpacing: HarborSpace.s2,
              children: [
                for (final tool in s.tools)
                  HarborPill(tool, icon: Icons.build_outlined),
              ],
            ),
            if (s.runnable && service != null) ...[
              const SizedBox(height: HarborSpace.s4),
              Align(
                alignment: AlignmentDirectional.centerEnd,
                child: FilledButton.icon(
                  key: ValueKey('skill-run-${s.id}'),
                  onPressed: () {
                    Navigator.of(context).pop();
                    SkillRunSheet.show(context, s, service);
                  },
                  icon: const Icon(Icons.play_arrow_outlined),
                  label: Text(l10n.skillsRun),
                ),
              ),
            ],
          ],
        ),
      ),
    );
    if (compact) {
      showModalBottomSheet<void>(
          context: context,
          isScrollControlled: true,
          useSafeArea: true,
          builder: (_) => content);
    } else {
      showDialog<void>(
        context: context,
        builder: (_) => Dialog(
          child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 560), child: content),
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final wc = HarborBreakpoints.of(context);
    final gutter = HarborBreakpoints.gutter(wc);
    final query = _filter.text.trim().toLowerCase();
    final all = service?.skills ?? const <SkillSummary>[];
    final skills = query.isEmpty
        ? all
        : [
            for (final s in all)
              if (s.title.toLowerCase().contains(query) ||
                  s.family.toLowerCase().contains(query) ||
                  s.description.toLowerCase().contains(query) ||
                  s.tools.any((tool) => tool.toLowerCase().contains(query)))
                s,
          ];
    final families = <String, List<SkillSummary>>{};
    for (final s in skills) {
      families.putIfAbsent(s.family, () => []).add(s);
    }

    return Column(children: [
      HarborSurfaceHeader(
        title: l10n.surfaceSkills,
        subtitle: l10n.skillsSubtitle,
        actions: [
          if (all.isNotEmpty)
            HarborPill(l10n.skillsCount(all.length),
                icon: Icons.construction_outlined, brand: true),
        ],
      ),
      if (all.isNotEmpty)
        Padding(
          padding: EdgeInsets.fromLTRB(gutter, 0, gutter, HarborSpace.s3),
          child: TextField(
            controller: _filter,
            decoration: InputDecoration(
              hintText: l10n.skillsSearchHint,
              prefixIcon: const Icon(Icons.filter_list),
              suffixIcon: query.isEmpty
                  ? null
                  : IconButton(
                      tooltip: l10n.closeAction,
                      icon: const Icon(Icons.clear),
                      onPressed: _filter.clear,
                    ),
            ),
          ),
        ),
      if (all.isNotEmpty && (service?.knowledgeOpen ?? false))
        _suggestBar(l10n, gutter),
      Expanded(
        child: Builder(builder: (context) {
          if (sp.failed || service == null) {
            return HarborErrorState(
                title: l10n.coreDegradedTitle,
                message: l10n.coreNotLoadedSkills);
          }
          if (all.isEmpty) {
            return HarborEmptyState(
                icon: Icons.construction_outlined,
                title: l10n.skillsEmptyTitle,
                body: l10n.skillsEmptyBody);
          }
          if (skills.isEmpty) {
            return HarborEmptyState(
                icon: Icons.search_off_outlined,
                title: l10n.skillsNoMatch,
                body: l10n.skillsEmptyBody);
          }
          return LayoutBuilder(builder: (context, constraints) {
            final columns = constraints.maxWidth >= 1100
                ? 3
                : constraints.maxWidth >= 700
                    ? 2
                    : 1;
            final cardWidth = (constraints.maxWidth -
                    2 * gutter -
                    (columns - 1) * HarborSpace.s3) /
                columns;
            return Scrollbar(
              child: ListView(
                key: const ValueKey('skills-list'),
                padding: EdgeInsets.fromLTRB(gutter, 0, gutter, HarborSpace.s8),
                children: [
                  for (final entry in families.entries) ...[
                    Padding(
                      padding: const EdgeInsets.only(
                          top: HarborSpace.s3, bottom: HarborSpace.s2),
                      child: Text(entry.key.toUpperCase(),
                          style: t.text
                              .captionOf(t.colors.inkMuted)
                              .copyWith(letterSpacing: 0.6)),
                    ),
                    Wrap(
                      spacing: HarborSpace.s3,
                      runSpacing: HarborSpace.s3,
                      children: [
                        for (final s in entry.value)
                          SizedBox(
                            width: cardWidth,
                            child: HarborCard(
                              onTap: () => _showDetail(s),
                              semanticLabel: s.title,
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Row(children: [
                                    Icon(Icons.construction_outlined,
                                        size: 18, color: t.colors.brand),
                                    const SizedBox(width: HarborSpace.s2),
                                    Expanded(
                                      child: Text(s.title,
                                          textDirection: TextDirection.ltr,
                                          style:
                                              t.text.bodyStrongOf(t.colors.ink),
                                          maxLines: 1,
                                          overflow: TextOverflow.ellipsis),
                                    ),
                                  ]),
                                  const SizedBox(height: HarborSpace.s2),
                                  Text(s.description,
                                      textDirection: TextDirection.ltr,
                                      maxLines: 3,
                                      overflow: TextOverflow.ellipsis,
                                      style: t.text.smallOf(t.colors.inkMuted)),
                                  const SizedBox(height: HarborSpace.s3),
                                  Row(children: [
                                    HarborPill(l10n.toolsCount(s.tools.length),
                                        icon: Icons.build_outlined),
                                    const SizedBox(width: HarborSpace.s2),
                                    Flexible(
                                      child: HarborPill(
                                        s.runnable
                                            ? l10n.skillsRunnable
                                            : l10n.skillsDeclaration,
                                        icon: s.runnable
                                            ? Icons.account_tree_outlined
                                            : Icons.description_outlined,
                                        brand: s.runnable,
                                      ),
                                    ),
                                    const Spacer(),
                                    Icon(Icons.chevron_right,
                                        size: 18, color: t.colors.inkMuted),
                                  ]),
                                ],
                              ),
                            ),
                          ),
                      ],
                    ),
                  ],
                ],
              ),
            );
          });
        }),
      ),
    ]);
  }
}
