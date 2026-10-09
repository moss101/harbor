import 'package:flutter/material.dart';
import 'package:harbor_domain/harbor_domain.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';

/// Agents: scheduled goals (decision 0011) — durable, user-authorized
/// proactive work — plus an honest account of what agent orchestration
/// is NOT in this release (no autonomous multi-step orchestration, no
/// computer use). The core hosts no timers: goals come due here and run
/// only while Harbor is open, with an explicit tap.
class AgentsSurface extends StatefulWidget {
  const AgentsSurface({super.key});

  @override
  State<AgentsSurface> createState() => _AgentsSurfaceState();
}

class _AgentsSurfaceState extends State<AgentsSurface> {
  List<Map<String, dynamic>> _goals = [];
  List<Map<String, dynamic>> _due = [];
  bool _loading = true;
  String? _busyGoal;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _reload());
  }

  Future<void> _reload() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final goals = await service.listGoals();
    final due = await service.dueGoals();
    if (!mounted) return;
    setState(() {
      _goals = goals;
      _due = due;
      _loading = false;
    });
  }

  String? _pickChatPackage(HarborService service) {
    final models = service.installedModels;
    if (models.isEmpty) return null;
    final installed = models.map((m) => m['id'] as String).toList();
    final preferred = AppStateScope.maybeOf(context)?.chatModel;
    if (preferred != null && installed.contains(preferred)) return preferred;
    return installed.first;
  }

  /// The only execution path: an explicit tap. The claim is write-ahead
  /// (a slot runs once, even across restarts); the answer generation is
  /// bound to the claimed run id, so the goal receipt and the durable
  /// run trail are one identity.
  Future<void> _runNow(Map<String, dynamic> entry) async {
    final service = HarborServiceProvider.of(context).notifier;
    final l10n = AppLocalizations.of(context)!;
    if (service == null) return;
    final goal = (entry['goal'] as Map).cast<String, dynamic>();
    final slot = entry['slot'] as String;
    final chat = _pickChatPackage(service);
    if (chat == null) {
      ScaffoldMessenger.maybeOf(context)
          ?.showSnackBar(SnackBar(content: Text(l10n.goalsNoChatModel)));
      return;
    }
    setState(() => _busyGoal = goal['id'] as String);
    try {
      final claim = await service.claimGoal(goal['id'] as String, slot);
      final runId = claim['run_id'] as String;
      final request = (goal['request'] as Map).cast<String, dynamic>();
      try {
        var outcome = 'completed';
        if (request['kind'] == 'skill') {
          // The claimed run id is the run's identity; the skill's own
          // graph, budgets and approvals apply. A goal never approves
          // anything: a run that stops at an approval is left WAITING
          // for the user in Activity.
          final skillId = request['skill_id'] as String? ?? '';
          final key = HarborService.goalSkillTextInput[skillId];
          if (key == null) {
            throw StateError('skill $skillId cannot run as a goal');
          }
          final report = await service.startSkillRun(
            skillId: skillId,
            inputs: {key: request['input'] as String? ?? ''},
            chatPackage: chat,
            runId: runId,
          );
          if (report['state'] == 'WAITING_APPROVAL') {
            outcome = 'awaiting_approval';
          }
        } else {
          await service.generateAnswer(
            request['text'] as String? ?? '',
            chatPackage: chat,
            runId: runId,
            maxTokens: 512,
          );
        }
        await service.recordGoalOutcome(goal['id'] as String, runId, outcome);
        if (mounted) {
          ScaffoldMessenger.maybeOf(context)?.showSnackBar(SnackBar(
              content: Text(outcome == 'awaiting_approval'
                  ? l10n.goalsRunAwaitingApproval
                  : l10n.goalsRunCompleted)));
        }
      } catch (e) {
        await service.recordGoalOutcome(
            goal['id'] as String, runId, 'failed: $e');
        if (mounted) {
          ScaffoldMessenger.maybeOf(context)
              ?.showSnackBar(SnackBar(content: Text(l10n.goalsRunFailed)));
        }
      }
    } catch (e) {
      // A refused claim means the slot already ran (restart, duplicate
      // driver) — say so instead of pretending.
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)
            ?.showSnackBar(SnackBar(content: Text(l10n.goalsClaimRefused)));
      }
    } finally {
      if (mounted) setState(() => _busyGoal = null);
      await _reload();
    }
  }

  Future<void> _createGoal() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final spec = await showDialog<_GoalDraft>(
      context: context,
      builder: (_) => _GoalCreateDialog(
        skills: [
          for (final k in service.skills)
            if (k.runnable &&
                HarborService.goalSkillTextInput.containsKey(k.id))
              k,
        ],
      ),
    );
    if (spec == null) return;
    try {
      await service.createGoal(
        title: spec.title,
        request: spec.skillId == null
            ? {'kind': 'prompt', 'text': spec.prompt}
            : {
                'kind': 'skill',
                'skill_id': spec.skillId,
                'input': spec.prompt,
              },
        schedule: spec.schedule,
        maxRuns: spec.maxRuns,
      );
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)
            ?.showSnackBar(SnackBar(content: Text(e.toString())));
      }
    }
    await _reload();
  }

  Future<void> _setPaused(Map<String, dynamic> goal, bool paused) async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    try {
      if (paused) {
        await service.pauseGoal(goal['id'] as String);
      } else {
        await service.resumeGoal(goal['id'] as String);
      }
    } catch (_) {}
    await _reload();
  }

  Future<void> _cancelGoal(Map<String, dynamic> goal) async {
    final service = HarborServiceProvider.of(context).notifier;
    final l10n = AppLocalizations.of(context)!;
    if (service == null) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(l10n.goalsCancelTitle),
        content: Text(l10n.goalsCancelBody),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: Text(l10n.cancelAction)),
          TextButton(
              onPressed: () => Navigator.pop(ctx, true),
              child: Text(l10n.goalsCancelConfirm)),
        ],
      ),
    );
    if (confirmed != true) return;
    try {
      await service.cancelGoal(goal['id'] as String);
    } catch (_) {}
    await _reload();
  }

  String _scheduleLabel(Map<String, dynamic> schedule, AppLocalizations l10n) {
    final kind = schedule['kind'] as String?;
    if (kind == 'once') {
      final at = DateTime.tryParse(schedule['at'] as String? ?? '');
      return l10n.goalsOnceAt(at == null
          ? ''
          : MaterialLocalizations.of(context).formatFullDate(at));
    }
    final minutes = schedule['minutes'];
    return l10n.goalsEveryMinutes(
        minutes is int ? minutes : int.tryParse('$minutes') ?? 0);
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return Column(children: [
      HarborSurfaceHeader(
          title: l10n.surfaceAgents, subtitle: l10n.agentsSubtitle),
      Expanded(
        child: HarborPage(
          maxWidth: HarborLayout.readingMax + 160,
          children: [
            HarborBanner(
              tone: HarborBannerTone.info,
              icon: Icons.schedule_outlined,
              title: l10n.goalsBannerTitle,
              body: l10n.goalsBannerBody,
            ),
            const SizedBox(height: HarborSpace.s5),
            Row(children: [
              Expanded(
                child: HarborSectionHeader(
                    title: l10n.goalsSectionTitle,
                    subtitle: l10n.goalsSectionSubtitle),
              ),
              const SizedBox(width: HarborSpace.s2),
              FilledButton.tonalIcon(
                onPressed: _createGoal,
                icon: const Icon(Icons.add_alarm_outlined, size: 18),
                label: Text(l10n.goalsNew),
              ),
            ]),
            if (_loading)
              Padding(
                padding: const EdgeInsets.all(HarborSpace.s5),
                child: Center(
                    child: CircularProgressIndicator(color: t.colors.brand)),
              )
            else ...[
              if (_due.isNotEmpty) ...[
                const SizedBox(height: HarborSpace.s3),
                for (final entry in _due)
                  _GoalCard(
                    goal: (entry['goal'] as Map).cast<String, dynamic>(),
                    scheduleLabel: _scheduleLabel(
                        ((entry['goal'] as Map)['schedule'] as Map)
                            .cast<String, dynamic>(),
                        l10n),
                    due: true,
                    busy: _busyGoal == entry['goal']['id'],
                    onRun: () => _runNow(entry),
                    onPauseResume: null,
                    onCancel: null,
                  ),
              ],
              if (_goals.isEmpty)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: HarborSpace.s6),
                  child: Center(
                    child: Text(l10n.goalsEmpty,
                        style: t.text.smallOf(t.colors.inkMuted)),
                  ),
                )
              else
                for (final goal in _goals)
                  _GoalCard(
                    goal: goal,
                    scheduleLabel: _scheduleLabel(
                        (goal['schedule'] as Map).cast<String, dynamic>(),
                        l10n),
                    due: false,
                    busy: false,
                    onRun: null,
                    onPauseResume:
                        (goal['state'] == 'active' || goal['state'] == 'paused')
                            ? (paused) => _setPaused(goal, paused)
                            : null,
                    onCancel:
                        (goal['state'] == 'active' || goal['state'] == 'paused')
                            ? () => _cancelGoal(goal)
                            : null,
                  ),
            ],
            const SizedBox(height: HarborSpace.s6),
            HarborSectionHeader(
                title: l10n.agentsWhatTitle, subtitle: l10n.agentsEmptyBody),
            const SizedBox(height: HarborSpace.s3),
            _NotYetCard(
                icon: Icons.hub_outlined,
                title: l10n.agentsNotAutonomyTitle,
                body: l10n.agentsNotAutonomyBody),
            const SizedBox(height: HarborSpace.s3),
            _NotYetCard(
                icon: Icons.mouse_outlined,
                title: l10n.agentsNotHandsTitle,
                body: l10n.agentsNotHandsBody),
            const SizedBox(height: HarborSpace.s3),
            HarborBanner(
              tone: HarborBannerTone.info,
              icon: Icons.construction_outlined,
              title: l10n.agentsUnavailableTitle,
              body: l10n.agentsUnavailableBody,
              action: AppStateScope.maybeOf(context) == null
                  ? null
                  : Wrap(
                      spacing: HarborSpace.s2,
                      runSpacing: HarborSpace.s2,
                      children: [
                          OutlinedButton.icon(
                            onPressed: () => AppStateScope.maybeOf(context)!
                                .goTo(HarborSurface.skills),
                            icon: const Icon(Icons.construction_outlined,
                                size: 16),
                            label: Text(l10n.agentsGoSkills),
                          ),
                          OutlinedButton.icon(
                            onPressed: () => AppStateScope.maybeOf(context)!
                                .goTo(HarborSurface.activity),
                            icon: const Icon(Icons.timeline_outlined, size: 16),
                            label: Text(l10n.agentsGoActivity),
                          ),
                        ]),
            ),
          ],
        ),
      ),
    ]);
  }
}

class _NotYetCard extends StatelessWidget {
  const _NotYetCard(
      {required this.icon, required this.title, required this.body});
  final IconData icon;
  final String title;
  final String body;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return HarborCard(
      child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Container(
          width: 36,
          height: 36,
          decoration: BoxDecoration(
            color: t.colors.surfaceRaised,
            borderRadius: BorderRadius.circular(HarborRadius.sm),
          ),
          child: Icon(icon, size: 18, color: t.colors.inkMuted),
        ),
        const SizedBox(width: HarborSpace.s3),
        Expanded(
          child:
              Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(title, style: t.text.bodyStrongOf(t.colors.ink)),
            const SizedBox(height: 2),
            Text(body, style: t.text.smallOf(t.colors.inkMuted)),
          ]),
        ),
      ]),
    );
  }
}

class _GoalCard extends StatelessWidget {
  const _GoalCard({
    required this.goal,
    required this.scheduleLabel,
    required this.due,
    required this.busy,
    required this.onRun,
    required this.onPauseResume,
    required this.onCancel,
  });

  final Map<String, dynamic> goal;
  final String scheduleLabel;
  final bool due;
  final bool busy;
  final VoidCallback? onRun;
  final ValueChanged<bool>? onPauseResume;
  final VoidCallback? onCancel;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final state = goal['state'] as String? ?? 'active';
    final stateLabel = switch (state) {
      'active' => l10n.goalsStateActive,
      'paused' => l10n.goalsStatePaused,
      'done' => l10n.goalsStateDone,
      'cancelled' => l10n.goalsStateCancelled,
      _ => state,
    };
    final runCount = goal['run_count'] is int ? goal['run_count'] as int : 0;
    final maxRuns = goal['max_runs'] is int ? goal['max_runs'] as int? : null;
    return Padding(
      padding: const EdgeInsets.only(bottom: HarborSpace.s3),
      child: HarborCard(
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            Expanded(
              child: Text(goal['title'] as String? ?? '',
                  style: t.text.bodyStrongOf(t.colors.ink),
                  overflow: TextOverflow.ellipsis),
            ),
            const SizedBox(width: HarborSpace.s2),
            HarborPill(stateLabel,
                icon: due
                    ? Icons.alarm_on_outlined
                    : switch (state) {
                        'active' => Icons.play_circle_outline,
                        'paused' => Icons.pause_circle_outline,
                        _ => Icons.flag_outlined,
                      },
                brand: due || state == 'active'),
          ]),
          const SizedBox(height: HarborSpace.s2),
          Wrap(
            spacing: HarborSpace.s2,
            runSpacing: HarborSpace.s1,
            children: [
              HarborPill(scheduleLabel, icon: Icons.schedule_outlined),
              if (maxRuns != null)
                HarborPill(l10n.goalsRunCount(runCount, maxRuns),
                    icon: Icons.repeat_outlined)
              else if (runCount > 0)
                HarborPill(l10n.goalsRunCountOpen(runCount),
                    icon: Icons.repeat_outlined),
            ],
          ),
          if (due ||
              onRun != null ||
              onPauseResume != null ||
              onCancel != null) ...[
            const SizedBox(height: HarborSpace.s3),
            Wrap(
                spacing: HarborSpace.s2,
                runSpacing: HarborSpace.s2,
                children: [
                  if (due)
                    FilledButton.icon(
                      onPressed: busy ? null : onRun,
                      icon: busy
                          ? const SizedBox(
                              width: 14,
                              height: 14,
                              child: CircularProgressIndicator(strokeWidth: 2))
                          : const Icon(Icons.play_arrow_outlined, size: 18),
                      label: Text(l10n.goalsRunNow),
                    ),
                  if (onPauseResume != null && !due)
                    OutlinedButton.icon(
                      onPressed: () => onPauseResume!(state == 'active'),
                      icon: Icon(
                          state == 'active'
                              ? Icons.pause_outlined
                              : Icons.play_arrow_outlined,
                          size: 18),
                      label: Text(state == 'active'
                          ? l10n.goalsPause
                          : l10n.goalsResume),
                    ),
                  if (onCancel != null && !due)
                    OutlinedButton.icon(
                      onPressed: onCancel,
                      icon: const Icon(Icons.close_outlined, size: 18),
                      label: Text(l10n.goalsCancel),
                    ),
                ]),
          ],
        ]),
      ),
    );
  }
}

/// A goal draft from the create dialog.
class _GoalDraft {
  _GoalDraft(this.title, this.prompt, this.schedule, this.maxRuns,
      {this.skillId});
  final String title;

  /// The prompt text, or — for a skill goal — the skill's text input.
  final String prompt;

  /// Null for a prompt goal.
  final String? skillId;
  final Map<String, dynamic> schedule;
  final int? maxRuns;
}

class _GoalCreateDialog extends StatefulWidget {
  const _GoalCreateDialog({this.skills = const []});

  /// Skills a goal may run (text-input skills only).
  final List<SkillSummary> skills;

  @override
  State<_GoalCreateDialog> createState() => _GoalCreateDialogState();
}

class _GoalCreateDialogState extends State<_GoalCreateDialog> {
  List<SkillSummary> get _skills => widget.skills;

  final _title = TextEditingController();
  final _prompt = TextEditingController();
  final _minutes = TextEditingController(text: '60');
  bool _repeating = true;
  bool _limited = false;
  String? _skillId; // null ⇒ prompt goal
  int _maxRuns = 5;

  @override
  void dispose() {
    _title.dispose();
    _prompt.dispose();
    _minutes.dispose();
    super.dispose();
  }

  Future<void> _pickOnceAt() async {
    final l10n = AppLocalizations.of(context)!;
    final now = DateTime.now();
    final date = await showDatePicker(
      context: context,
      initialDate: now.add(const Duration(hours: 1)),
      firstDate: now,
      lastDate: now.add(const Duration(days: 365)),
    );
    if (date == null || !mounted) return;
    final time = await showTimePicker(
      context: context,
      initialTime: TimeOfDay.fromDateTime(now.add(const Duration(hours: 1))),
    );
    if (time == null || !mounted) return;
    final at =
        DateTime(date.year, date.month, date.day, time.hour, time.minute);
    // Replace the placeholder controller content with the chosen time.
    _minutes.text = at.toIso8601String();
    // ignore: use_build_context_synchronously
    ScaffoldMessenger.maybeOf(context)?.showSnackBar(
        SnackBar(content: Text(l10n.goalsOncePicked(at.toLocal().toString()))));
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return AlertDialog(
      title: Text(l10n.goalsCreateTitle),
      content: SizedBox(
        width: 420,
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          TextField(
            controller: _title,
            decoration: InputDecoration(
                labelText: l10n.goalsNameLabel, hintText: l10n.goalsNameHint),
          ),
          const SizedBox(height: HarborSpace.s3),
          if (_skills.isNotEmpty) ...[
            SegmentedButton<bool>(
              segments: [
                ButtonSegment<bool>(
                    value: false, label: Text(l10n.goalsKindPrompt)),
                ButtonSegment<bool>(
                    value: true, label: Text(l10n.goalsKindSkill)),
              ],
              selected: {_skillId != null},
              onSelectionChanged: (sel) => setState(
                  () => _skillId = sel.first ? _skills.first.id : null),
            ),
            const SizedBox(height: HarborSpace.s3),
          ],
          if (_skillId != null) ...[
            DropdownButtonFormField<String>(
              initialValue: _skillId,
              isExpanded: true,
              decoration: InputDecoration(labelText: l10n.goalsSkillLabel),
              items: [
                for (final s in _skills)
                  DropdownMenuItem(
                      value: s.id,
                      child: Text(s.title, overflow: TextOverflow.ellipsis)),
              ],
              onChanged: (v) => setState(() => _skillId = v),
            ),
            const SizedBox(height: HarborSpace.s3),
          ],
          TextField(
            controller: _prompt,
            maxLines: 3,
            decoration: InputDecoration(
                labelText: _skillId == null
                    ? l10n.goalsPromptLabel
                    : l10n.goalsSkillInputLabel,
                hintText: _skillId == null ? l10n.goalsPromptHint : null),
          ),
          const SizedBox(height: HarborSpace.s3),
          SegmentedButton<bool>(
            segments: [
              ButtonSegment<bool>(
                  value: true, label: Text(l10n.goalsRepeatLabel)),
              ButtonSegment<bool>(
                  value: false, label: Text(l10n.goalsOnceLabel)),
            ],
            selected: {_repeating},
            onSelectionChanged: (selection) => setState(() {
              final v = selection.first;
              _repeating = v;
              if (!v && _minutes.text.contains('-')) {
                _minutes.text = '60';
              }
            }),
          ),
          if (_repeating)
            TextField(
              controller: _minutes,
              keyboardType: TextInputType.number,
              decoration:
                  InputDecoration(labelText: l10n.goalsEveryMinutesLabel),
            )
          else
            OutlinedButton.icon(
              onPressed: _pickOnceAt,
              icon: const Icon(Icons.event_outlined, size: 18),
              label: Text(_minutes.text.contains('-')
                  ? l10n.goalsOncePicked(_minutes.text)
                  : l10n.goalsOncePick),
            ),
          const SizedBox(height: HarborSpace.s2),
          CheckboxListTile(
            value: _limited,
            onChanged: (v) => setState(() => _limited = v ?? false),
            title:
                Text(l10n.goalsLimitLabel, style: t.text.smallOf(t.colors.ink)),
            contentPadding: EdgeInsets.zero,
            dense: true,
            controlAffinity: ListTileControlAffinity.leading,
          ),
          if (_limited)
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: SizedBox(
                width: 220,
                child: Row(children: [
                  Expanded(
                    child: Slider(
                      value: _maxRuns.toDouble(),
                      min: 1,
                      max: 20,
                      divisions: 19,
                      label: '$_maxRuns',
                      onChanged: (v) => setState(() => _maxRuns = v.round()),
                    ),
                  ),
                  Text(l10n.goalsRunsCount(_maxRuns)),
                ]),
              ),
            ),
        ]),
      ),
      actions: [
        TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text(l10n.cancelAction)),
        FilledButton(
          onPressed: () {
            final title = _title.text.trim();
            final prompt = _prompt.text.trim();
            if (title.isEmpty || prompt.isEmpty) return;
            final schedule = _repeating
                ? {
                    'kind': 'every_minutes',
                    'minutes': int.tryParse(_minutes.text.trim()) ?? 0,
                  }
                : {
                    'kind': 'once',
                    'at': _minutes.text.contains('-')
                        ? _minutes.text.trim()
                        : DateTime.now().toIso8601String(),
                  };
            Navigator.pop(
              context,
              _GoalDraft(title, prompt, schedule, _limited ? _maxRuns : null,
                  skillId: _skillId),
            );
          },
          child: Text(l10n.goalsCreateConfirm),
        ),
      ],
    );
  }
}
