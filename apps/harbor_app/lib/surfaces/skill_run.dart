import 'dart:convert';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:harbor_domain/harbor_domain.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';

/// Run a graph skill (decision 0006): the form is generated from the
/// graph's input schema, the run executes on the durable executor in the
/// core, and a proposal parks the run for an explicit approval here. The
/// UI owns no policy: every state shown is read back from the core.
class SkillRunSheet extends StatefulWidget {
  const SkillRunSheet({super.key, required this.skill, required this.service});

  final SkillSummary skill;
  final HarborService service;

  static Future<void> show(
      BuildContext context, SkillSummary skill, HarborService service) {
    final compact = HarborBreakpoints.isCompact(HarborBreakpoints.of(context));
    final sheet = SkillRunSheet(skill: skill, service: service);
    if (compact) {
      return showModalBottomSheet<void>(
        context: context,
        isScrollControlled: true,
        useSafeArea: true,
        builder: (_) => sheet,
      );
    }
    return showDialog<void>(
      context: context,
      builder: (_) => Dialog(
        child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 640, maxHeight: 760),
            child: sheet),
      ),
    );
  }

  @override
  State<SkillRunSheet> createState() => _SkillRunSheetState();
}

class _SkillRunSheetState extends State<SkillRunSheet> {
  final Map<String, TextEditingController> _fields = {};
  final Map<String, XFile> _attached = {};
  final Map<String, List<int>> _attachedBytes = {};
  String? _chatPackage;
  bool _busy = false;
  String? _error;
  Map<String, dynamic>? _report;

  SkillGraphInfo get _graph => widget.skill.graph!;

  @override
  void initState() {
    super.initState();
    for (final e in _graph.inputProperties) {
      if (!_isArtifact(e.key)) {
        _fields[e.key] = TextEditingController();
      }
    }
    final models = widget.service.installedModels;
    if (models.isNotEmpty) _chatPackage = models.first['id'] as String;
  }

  @override
  void dispose() {
    for (final c in _fields.values) {
      c.dispose();
    }
    super.dispose();
  }

  bool _isArtifact(String key) => key == 'artifact_id';

  bool _isObject(Map<String, dynamic> schema) => schema['type'] == 'object';

  bool _isLong(Map<String, dynamic> schema) =>
      ((schema['maxLength'] as num?) ?? 0) > 300;

  Future<void> _attach(String key) async {
    final l10n = AppLocalizations.of(context)!;
    final XFile? file;
    try {
      file = await openFile(acceptedTypeGroups: [
        XTypeGroup(
          label: l10n.fileGroupDocuments,
          extensions: const ['docx', 'pdf', 'xlsx', 'pptx', 'txt', 'md'],
        ),
      ]);
    } catch (_) {
      return; // picker dismissed
    }
    if (file == null) return;
    final bytes = await file.readAsBytes();
    setState(() {
      _attached[key] = file!;
      _attachedBytes[key] = bytes;
    });
  }

  /// `key = value` lines → object; blank lines ignored.
  Map<String, dynamic> _parseValues(String text) {
    final out = <String, dynamic>{};
    for (final raw in text.split('\n')) {
      final line = raw.trim();
      if (line.isEmpty) continue;
      final eq = line.indexOf('=');
      if (eq <= 0) continue;
      out[line.substring(0, eq).trim()] = line.substring(eq + 1).trim();
    }
    return out;
  }

  bool get _canRun {
    if (_busy) return false;
    if (_graph.usesModel && _chatPackage == null) return false;
    for (final key in _graph.requiredInputs) {
      if (_isArtifact(key)) {
        if (!_attachedBytes.containsKey(key)) return false;
      } else if ((_fields[key]?.text.trim() ?? '').isEmpty) {
        return false;
      }
    }
    return true;
  }

  Future<void> _run() async {
    final inputs = <String, dynamic>{};
    final artifacts = <SkillArtifact>[];
    for (final e in _graph.inputProperties) {
      final key = e.key;
      if (_isArtifact(key)) {
        final bytes = _attachedBytes[key];
        if (bytes == null) continue;
        const id = 'a1';
        inputs[key] = id;
        artifacts.add(
            SkillArtifact(id: id, name: _attached[key]!.name, bytes: bytes));
        continue;
      }
      final text = _fields[key]!.text;
      if (text.trim().isEmpty) continue;
      inputs[key] = _isObject(e.value) ? _parseValues(text) : text;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final report = await widget.service.startSkillRun(
        skillId: widget.skill.id,
        inputs: inputs,
        artifacts: artifacts,
        chatPackage: _graph.usesModel ? _chatPackage : null,
      );
      if (!mounted) return;
      setState(() => _report = report);
    } on ffi.HarborCoreException catch (e) {
      if (!mounted) return;
      setState(() => _error = e.message);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _decide(bool approved) async {
    final runId = _report?['run_id'] as String?;
    if (runId == null) return;
    setState(() => _busy = true);
    try {
      final report = await widget.service.decideRun(runId, approved: approved);
      if (!mounted) return;
      setState(() => _report = report);
    } on ffi.HarborCoreException catch (e) {
      if (!mounted) return;
      setState(() => _error = e.message);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return SafeArea(
      top: false,
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(HarborSpace.s5),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(l10n.skillsRunTitle(widget.skill.title),
                style: t.text.h2Of(t.colors.ink)),
            const SizedBox(height: HarborSpace.s2),
            Text(widget.skill.description,
                style: t.text.bodyOf(t.colors.inkMuted)),
            const SizedBox(height: HarborSpace.s4),
            if (_report == null) ..._form(context, l10n, t),
            if (_report != null) ..._result(context, l10n, t),
            if (_error != null) ...[
              const SizedBox(height: HarborSpace.s3),
              HarborBanner(
                  tone: HarborBannerTone.danger,
                  title: l10n.skillsRunFailed,
                  body: _error),
            ],
          ],
        ),
      ),
    );
  }

  List<Widget> _form(
      BuildContext context, AppLocalizations l10n, HarborTheme t) {
    final required = _graph.requiredInputs.toSet();
    return [
      Text(l10n.skillsInputsHeading,
          style: t.text.captionOf(t.colors.inkMuted)),
      const SizedBox(height: HarborSpace.s2),
      for (final e in _graph.inputProperties) ...[
        if (_isArtifact(e.key))
          Row(children: [
            OutlinedButton.icon(
              key: ValueKey('attach-${e.key}'),
              onPressed: _busy ? null : () => _attach(e.key),
              icon: const Icon(Icons.attach_file),
              label: Text(l10n.skillsAttachFile),
            ),
            const SizedBox(width: HarborSpace.s3),
            Expanded(
              child: Text(
                _attached[e.key] != null
                    ? l10n.skillsAttached(_attached[e.key]!.name)
                    : (required.contains(e.key)
                        ? l10n.skillsRequiredField
                        : ''),
                style: t.text.smallOf(t.colors.inkMuted),
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ])
        else
          TextField(
            key: ValueKey('input-${e.key}'),
            controller: _fields[e.key],
            enabled: !_busy,
            maxLines: _isObject(e.value) || _isLong(e.value) ? 6 : 1,
            minLines: _isObject(e.value) || _isLong(e.value) ? 3 : 1,
            decoration: InputDecoration(
              labelText: required.contains(e.key)
                  ? '${e.key} · ${l10n.skillsRequiredField}'
                  : e.key,
              helperText: _isObject(e.value)
                  ? l10n.skillsValuesHint
                  : (e.value['description'] as String?),
              helperMaxLines: 3,
            ),
            onChanged: (_) => setState(() {}),
          ),
        const SizedBox(height: HarborSpace.s3),
      ],
      if (_graph.usesModel) ...[
        if (widget.service.installedModels.isEmpty)
          HarborBanner(
              tone: HarborBannerTone.warning,
              title: l10n.skillsModelLabel,
              body: l10n.skillsNeedsModel)
        else
          DropdownButtonFormField<String>(
            key: const ValueKey('skill-model'),
            initialValue: _chatPackage,
            decoration: InputDecoration(labelText: l10n.skillsModelLabel),
            items: [
              for (final m in widget.service.installedModels)
                DropdownMenuItem(
                    value: m['id'] as String, child: Text(m['id'] as String)),
            ],
            onChanged: _busy ? null : (v) => setState(() => _chatPackage = v),
          ),
        const SizedBox(height: HarborSpace.s3),
      ],
      if (_busy)
        HarborOpProgress(
          title: l10n.skillsRunning,
          detail: _graph.id,
          icon: Icons.account_tree_outlined,
        )
      else
        Align(
          alignment: AlignmentDirectional.centerEnd,
          child: FilledButton.icon(
            key: const ValueKey('skill-run-button'),
            onPressed: _canRun ? _run : null,
            icon: const Icon(Icons.play_arrow_outlined),
            label: Text(l10n.skillsRun),
          ),
        ),
    ];
  }

  List<Widget> _result(
      BuildContext context, AppLocalizations l10n, HarborTheme t) {
    final report = _report!;
    final state = report['state'] as String? ?? '';
    final status = (report['status'] as Map?)?.cast<String, dynamic>() ?? {};
    final kind = status['status'] as String? ?? '';
    final trail = (report['trail'] as List?)?.cast<Map>() ?? const [];
    final semantic = switch (state) {
      'COMPLETED' => ExecutionSemantic.local,
      'WAITING_APPROVAL' => ExecutionSemantic.hybrid,
      _ => ExecutionSemantic.danger,
    };
    final outcomeLabel = switch (status['outcome']) {
      'completed' => l10n.skillsOutcomeCompleted,
      'needs_input' => l10n.skillsOutcomeNeedsInput,
      'abstained' => l10n.skillsOutcomeAbstained,
      _ => null,
    };
    return [
      Row(children: [
        StatusBadge(semantic: semantic, label: state, large: true),
        const SizedBox(width: HarborSpace.s3),
        Expanded(
          child: HarborIdentifier(report['run_id'] as String? ?? ''),
        ),
      ]),
      const SizedBox(height: HarborSpace.s3),
      if (outcomeLabel != null)
        HarborKeyValue(label: l10n.skillsOutcome, value: outcomeLabel),
      if (kind == 'failed')
        HarborBanner(
            tone: HarborBannerTone.danger,
            title: l10n.skillsRunFailed,
            body: status['error'] as String?),
      if (state == 'WAITING_APPROVAL') ..._approval(l10n, t, status),
      if (status['outputs'] is Map &&
          (status['outputs'] as Map).isNotEmpty) ...[
        const SizedBox(height: HarborSpace.s3),
        Text(l10n.skillsOutputsHeading,
            style: t.text.captionOf(t.colors.inkMuted)),
        const SizedBox(height: HarborSpace.s2),
        for (final e in (status['outputs'] as Map).entries)
          _jsonBlock(t, e.key.toString(), e.value),
      ],
      const SizedBox(height: HarborSpace.s3),
      Text(l10n.skillsTrailHeading, style: t.text.captionOf(t.colors.inkMuted)),
      const SizedBox(height: HarborSpace.s2),
      for (final n in trail)
        HarborListRow(
          dense: true,
          leading: Icon(_iconFor(n['kind'] as String? ?? ''),
              size: 18, color: t.colors.brand),
          title: Text('${n['node_id']} · ${n['kind']}',
              style: t.text.bodyStrongOf(t.colors.ink)),
          subtitle: Text(
            [
              if (n['tool'] != null) n['tool'],
              if (n['executed_on'] != null)
                l10n.skillsExecutedOn(n['executed_on'] as String),
              if (n['structured_mode'] != null)
                l10n.skillsStructuredMode(n['structured_mode'] as String),
              if (n['decision'] != null) '→ ${n['decision']}',
              '${n['elapsed_ms']} ms',
            ].join(' · '),
            style: t.text.smallOf(t.colors.inkMuted),
          ),
        ),
    ];
  }

  List<Widget> _approval(
      AppLocalizations l10n, HarborTheme t, Map<String, dynamic> status) {
    final approval =
        (status['approval'] as Map?)?.cast<String, dynamic>() ?? {};
    final batch = (approval['batch'] as Map?)?.cast<String, dynamic>() ?? {};
    final ops = (batch['operations'] as List?)?.length ?? 0;
    return [
      const SizedBox(height: HarborSpace.s3),
      HarborSheet(
        key: const ValueKey('skill-approval'),
        title: l10n.skillsApprovalTitle,
        explanation: l10n.skillsApprovalBody(
            approval['effect_class'] as String? ?? '', ops),
        approveLabel: l10n.skillsApprove,
        denyLabel: l10n.skillsReject,
        onApprove: _busy ? () {} : () => _decide(true),
        onDeny: _busy ? () {} : () => _decide(false),
        diffSummary: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (approval['base_content_hash'] != null)
              HarborKeyValue(
                  label: l10n.skillsBaseHash,
                  value: approval['base_content_hash'] as String,
                  identifier: true),
            if (approval['proposed_output_hash'] != null)
              HarborKeyValue(
                  label: l10n.skillsProposedHash,
                  value: approval['proposed_output_hash'] as String,
                  identifier: true),
            for (final op in (batch['operations'] as List?) ?? const [])
              HarborListRow(
                dense: true,
                titleIsIdentifier: true,
                title: Text(
                    '${op['kind']} → ${op['precondition']?['target_id']}',
                    style: t.text.monoOf(t.colors.ink)),
                subtitle: op['args']?['text'] != null
                    ? Text(op['args']['text'] as String,
                        style: t.text.smallOf(t.colors.inkMuted),
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis)
                    : (op['args']?['value'] != null
                        ? Text(
                            '${op['args']['address']}: ${op['args']['value']}',
                            style: t.text.smallOf(t.colors.inkMuted))
                        : null),
              ),
          ],
        ),
      ),
    ];
  }

  Widget _jsonBlock(HarborTheme t, String label, Object? value) {
    final text = value is String
        ? value
        : const JsonEncoder.withIndent('  ').convert(value);
    return Padding(
      padding: const EdgeInsets.only(bottom: HarborSpace.s2),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(label, style: t.text.smallOf(t.colors.inkMuted)),
          Container(
            width: double.infinity,
            padding: const EdgeInsets.all(HarborSpace.s3),
            decoration: BoxDecoration(
              color: t.colors.surfaceRaised,
              borderRadius: BorderRadius.circular(HarborRadius.sm),
            ),
            child: SelectableText(text,
                style: t.text.monoOf(t.colors.ink, size: 12)),
          ),
        ],
      ),
    );
  }

  IconData _iconFor(String kind) => switch (kind) {
        'tool.call' => Icons.build_outlined,
        'model.structured' || 'model.text' => Icons.psychology_outlined,
        'branch' => Icons.alt_route_outlined,
        'approval' => Icons.verified_user_outlined,
        'map' => Icons.repeat_outlined,
        'const' => Icons.data_object_outlined,
        'end' => Icons.flag_outlined,
        _ => Icons.circle_outlined,
      };
}
