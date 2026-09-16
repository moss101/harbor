import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';
import '../widgets/ops.dart';

/// Activity center (UX-030): durable runs (state badge, id, executor time,
/// replayable Run Trail) and background operations (live and finished),
/// all straight from the core.
class ActivitySurface extends StatefulWidget {
  const ActivitySurface({super.key});

  @override
  State<ActivitySurface> createState() => _ActivitySurfaceState();
}

class _ActivitySurfaceState extends State<ActivitySurface>
    with SingleTickerProviderStateMixin {
  late final TabController _tabs = TabController(length: 2, vsync: this);

  @override
  void dispose() {
    _tabs.dispose();
    super.dispose();
  }

  Future<void> _showRun(HarborService service, Map<String, dynamic> run) async {
    final l10n = AppLocalizations.of(context)!;
    final runId = run['run_id'] as String;
    final results = await Future.wait([
      service.replayRun(runId),
      service.runState(runId),
    ]);
    if (!mounted) return;
    final report = results[0];
    final stateInfo = results[1];
    if (report == null) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(l10n.activityReplayFailed)));
      return;
    }
    final compact = HarborBreakpoints.isCompact(HarborBreakpoints.of(context));
    final detail = _RunDetail(run: run, report: report, counters: stateInfo);
    if (compact) {
      await showModalBottomSheet<void>(
        context: context,
        isScrollControlled: true,
        useSafeArea: true,
        builder: (_) => DraggableScrollableSheet(
          expand: false,
          initialChildSize: 0.7,
          maxChildSize: 0.95,
          builder: (_, controller) =>
              SingleChildScrollView(controller: controller, child: detail),
        ),
      );
    } else {
      await showDialog<void>(
        context: context,
        builder: (_) => Dialog(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 640, maxHeight: 720),
            child: SingleChildScrollView(child: detail),
          ),
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final runs = service?.runs ?? const [];
    final wc = HarborBreakpoints.of(context);
    final gutter = HarborBreakpoints.gutter(wc);
    return Column(children: [
      HarborSurfaceHeader(
        title: l10n.surfaceActivity,
        subtitle: l10n.activitySubtitle,
        actions: [
          if (runs.isNotEmpty)
            HarborPill(l10n.activityRunsCount(runs.length),
                icon: Icons.timeline_outlined, brand: true),
        ],
      ),
      TabBar(
        controller: _tabs,
        isScrollable: true,
        padding: EdgeInsets.symmetric(horizontal: gutter - HarborSpace.s3),
        tabs: [
          Tab(text: l10n.activityTabRuns),
          Tab(text: l10n.activityTabOps),
        ],
      ),
      Expanded(
        child: TabBarView(
          controller: _tabs,
          children: [
            _RunsTab(onOpen: (run) => _showRun(service!, run)),
            const _OpsTab(),
          ],
        ),
      ),
    ]);
  }
}

class _RunsTab extends StatelessWidget {
  const _RunsTab({required this.onOpen});
  final ValueChanged<Map<String, dynamic>> onOpen;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (sp.failed || service == null) {
      return HarborErrorState(
          title: l10n.coreDegradedTitle, message: l10n.coreNotLoadedActivity);
    }
    final runs = service.runs;
    if (runs.isEmpty) {
      return HarborEmptyState(
        icon: Icons.timeline_outlined,
        title: l10n.activityEmptyTitle,
        body: l10n.activityEmptyBody,
      );
    }
    final gutter = HarborBreakpoints.gutter(HarborBreakpoints.of(context));
    return Scrollbar(
      child: ListView.builder(
        padding:
            EdgeInsets.fromLTRB(gutter, HarborSpace.s3, gutter, HarborSpace.s8),
        itemCount: runs.length,
        itemBuilder: (context, i) {
          final r = runs[i];
          final state = r['state'] as String;
          return Padding(
            padding: const EdgeInsets.only(bottom: HarborSpace.s2),
            child: HarborCard(
              padding: EdgeInsets.zero,
              onTap: () => onOpen(r),
              child: HarborListRow(
                titleIsIdentifier: true,
                title: HarborIdentifier(r['run_id'] as String, size: 13),
                subtitle: Text(l10n.runStateLine(
                  state,
                  r['active_compute_ms_total'] as int,
                )),
                trailing: Row(mainAxisSize: MainAxisSize.min, children: [
                  RunStateBadge(state: state),
                  const SizedBox(width: HarborSpace.s1),
                  Icon(Icons.chevron_right, color: t.colors.inkMuted),
                ]),
              ),
            ),
          );
        },
      ),
    );
  }
}

class _OpsTab extends StatefulWidget {
  const _OpsTab();

  @override
  State<_OpsTab> createState() => _OpsTabState();
}

class _OpsTabState extends State<_OpsTab> {
  List<Map<String, dynamic>>? _ops;
  bool _loading = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // Refresh the registry snapshot whenever live progress changes.
    _load();
  }

  Future<void> _load() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null || _loading) return;
    _loading = true;
    final ops = await service.listOps();
    _loading = false;
    if (!mounted) return;
    setState(() => _ops = ops);
  }

  static int _rank(String? state) => switch (state) {
        'running' => 0,
        'cancelling' => 1,
        'failed' => 2,
        'cancelled' => 3,
        _ => 4,
      };

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (sp.failed || service == null) {
      return HarborErrorState(
          title: l10n.coreDegradedTitle, message: l10n.coreNotLoadedActivity);
    }
    final live = {for (final op in service.activeOps) op['op_id']: op};
    final ops = [
      ...live.values,
      for (final op in _ops ?? const <Map<String, dynamic>>[])
        if (!live.containsKey(op['op_id'])) op,
    ]..sort((a, b) =>
        _rank(a['state'] as String?).compareTo(_rank(b['state'] as String?)));
    if (ops.isEmpty) {
      return HarborEmptyState(
        icon: Icons.hourglass_empty_outlined,
        title: l10n.activityOpsEmptyTitle,
        body: l10n.activityOpsEmptyBody,
      );
    }
    final gutter = HarborBreakpoints.gutter(HarborBreakpoints.of(context));
    return Scrollbar(
      child: ListView.builder(
        padding:
            EdgeInsets.fromLTRB(gutter, HarborSpace.s3, gutter, HarborSpace.s8),
        itemCount: ops.length,
        itemBuilder: (context, i) {
          final op = ops[i];
          final state = (op['state'] as String?) ?? '';
          final active = state == 'running' || state == 'cancelling';
          if (active) {
            return Padding(
              padding: const EdgeInsets.only(bottom: HarborSpace.s2),
              child: OpProgressCard(progress: op, service: service),
            );
          }
          final semantic = switch (state) {
            'done' => ExecutionSemantic.local,
            'cancelled' => ExecutionSemantic.hybrid,
            _ => ExecutionSemantic.danger,
          };
          final error =
              op['result'] is Map ? op['result']['error']?.toString() : null;
          return Padding(
            padding: const EdgeInsets.only(bottom: HarborSpace.s2),
            child: HarborCard(
              padding: EdgeInsets.zero,
              child: HarborListRow(
                leading: Icon(opKindIcon(op['kind'] as String?)),
                title: Text(opKindTitle(op['kind'] as String?, l10n)),
                subtitle: error != null
                    ? Directionality(
                        textDirection: TextDirection.ltr,
                        child: Text(error,
                            style: t.text.monoOf(t.colors.inkMuted, size: 11)))
                    : HarborIdentifier(
                        (op['detail'] as String?) ?? (op['op_id'] as String),
                        size: 11,
                        color: t.colors.inkMuted),
                trailing: StatusBadge(
                  semantic: semantic,
                  icon: switch (state) {
                    'done' => Icons.check_circle_outline,
                    'cancelled' => Icons.cancel_outlined,
                    _ => Icons.error_outline,
                  },
                  label: state.toUpperCase(),
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}

class _RunDetail extends StatelessWidget {
  const _RunDetail(
      {required this.run, required this.report, required this.counters});
  final Map<String, dynamic> run;
  final Map<String, dynamic> report;
  final Map<String, dynamic>? counters;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final trail = (report['trail'] as List? ?? const []).cast<Map>();
    final finalState =
        (report['final_state'] as String?) ?? (run['state'] as String);
    final c = counters?['counters'];
    final steps = c is Map ? (c['step_count_total'] as num?)?.toInt() : null;
    final compute = c is Map
        ? (c['active_compute_ms_total'] as num?)?.toInt()
        : run['active_compute_ms_total'] as int?;
    return Padding(
      padding: const EdgeInsets.all(HarborSpace.s5),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(children: [
            Expanded(
                child: Text(l10n.activityRunDetail,
                    style: t.text.h2Of(t.colors.ink))),
            RunStateBadge(state: finalState),
          ]),
          const SizedBox(height: HarborSpace.s2),
          HarborIdentifier(run['run_id'] as String, size: 12, selectable: true),
          const SizedBox(height: HarborSpace.s4),
          Wrap(spacing: HarborSpace.s2, runSpacing: HarborSpace.s2, children: [
            SizedBox(
              width: 150,
              child: HarborMetric(
                  value: finalState,
                  label: l10n.activityFinalState,
                  icon: Icons.flag_outlined),
            ),
            SizedBox(
              width: 150,
              child: HarborMetric(
                  value: '${report['verified_events'] ?? trail.length}',
                  label: l10n.activityVerifiedEvents,
                  icon: Icons.verified_outlined),
            ),
            if (steps != null)
              SizedBox(
                width: 150,
                child: HarborMetric(
                    value: '$steps',
                    label: l10n.activityStepsLabel,
                    icon: Icons.format_list_numbered),
              ),
            if (compute != null)
              SizedBox(
                width: 150,
                child: HarborMetric(
                    value: '$compute ms',
                    label: l10n.activityExecutorTime,
                    icon: Icons.timer_outlined),
              ),
          ]),
          const SizedBox(height: HarborSpace.s5),
          Text(l10n.activityTrailHeading,
              style: t.text.bodyStrongOf(t.colors.ink)),
          const SizedBox(height: HarborSpace.s3),
          RunTrail(
            emptyLabel: l10n.runTrailEmpty,
            entries: [
              for (final e in trail)
                RunTrailEntry(
                  e['summary'] as String,
                  icon: '${e['type']}'.contains('transition')
                      ? Icons.swap_horiz
                      : Icons.circle_outlined,
                  detail: '${e['actor']}',
                  technical: 'seq ${e['seq']} · ${e['type']}',
                ),
            ],
          ),
        ],
      ),
    );
  }
}
