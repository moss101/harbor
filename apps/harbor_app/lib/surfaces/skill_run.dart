import 'dart:convert';
import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:harbor_domain/harbor_domain.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';
import 'package:path_provider/path_provider.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';
import '../widgets/ops.dart';

/// Whether this platform's file picker hands back the user's actual file.
///
/// It does on the desktops, which use NSOpenPanel and its equivalents. It
/// does NOT on mobile: iOS presents `UIDocumentPickerViewController` in
/// `.import` mode, which copies the selection into the app's temporary
/// directory, and Android's Storage Access Framework path is resolved
/// through `getPathFromCopyOfFileFromUri`. Both hand back a COPY.
///
/// That matters for one action only, and it matters a great deal:
/// "Overwrite original" takes the picked path as its destination. On
/// mobile that path is the temporary copy, so the commit would succeed,
/// the receipt would verify, the UI would report the file overwritten —
/// and the user's document would be untouched. Harbor's whole
/// safe-commit design is about writes that are honest and verified, so
/// the option is not offered where it cannot be honoured. Save new copy
/// is unaffected: it writes to a destination the user chooses.
bool get pickerReturnsTheUsersFile =>
    Platform.isMacOS || Platform.isWindows || Platform.isLinux;

/// Run a graph skill (decision 0006): the form is generated from the
/// graph's input schema, the run executes on the durable executor in the
/// core, and a proposal parks the run for an explicit approval here. The
/// UI owns no policy: every state shown is read back from the core.
/// Resolves where a Save New Copy lands. The default asks the platform
/// (native save dialog on desktop, the app documents folder elsewhere);
/// tests inject a fixed path.
typedef SaveDestinationResolver = Future<String?> Function(
    String suggestedName);

class SkillRunSheet extends StatefulWidget {
  const SkillRunSheet({
    super.key,
    required this.skill,
    required this.service,
    this.resolveSaveDestination,
    this.pickArtifact,
  });

  final SkillSummary skill;
  final HarborService service;
  final SaveDestinationResolver? resolveSaveDestination;

  /// Replaces the platform file picker (tests attach fixtures directly).
  final Future<XFile?> Function()? pickArtifact;

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
  Map<String, dynamic>? _commit;

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
      file = widget.pickArtifact != null
          ? await widget.pickArtifact!()
          : await openFile(acceptedTypeGroups: [
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

  /// The attached artifact the proposal is bound to (its bytes go back to
  /// the core with the commit; the core keeps no document content).
  MapEntry<String, XFile>? get _proposalFile {
    if (_attached.isEmpty) return null;
    return _attached.entries.first;
  }

  String _suggestedCopyName(String original) {
    final dot = original.lastIndexOf('.');
    if (dot <= 0) return '$original (Harbor)';
    return '${original.substring(0, dot)} (Harbor)${original.substring(dot)}';
  }

  Future<String?> _defaultSaveDestination(String suggestedName) async {
    if (Platform.isMacOS || Platform.isWindows || Platform.isLinux) {
      final location = await getSaveLocation(suggestedName: suggestedName);
      return location?.path;
    }
    // Mobile pickers hand out cached copies, not the user's folder, and
    // neither file_selector_ios nor file_selector_android implements a
    // save picker — so the copy lands in the app's documents directory.
    //
    // On iOS that directory is only REACHABLE because Info.plist now sets
    // UIFileSharingEnabled and LSSupportsOpeningDocumentsInPlace; without
    // them the save succeeds, the UI reports where, and the file cannot
    // be opened, shared or found by anyone. Safe to expose because
    // Harbor's own data lives in Application Support, not here — the only
    // thing in Documents is a copy the user deliberately saved.
    //
    // Android has the same reachability problem and NO equivalent
    // one-line fix: app-private storage is invisible under scoped
    // storage, and solving it properly needs a SAF create-document
    // channel or a share sheet. Untouched, and flagged rather than
    // papered over.
    final dir = await getApplicationDocumentsDirectory();
    return '${dir.path}${Platform.pathSeparator}$suggestedName';
  }

  /// Approve and write the proposal (production plan B1). Save New Copy is
  /// the default; Overwrite asks first and only proceeds in the core when
  /// the original still matches the approved base.
  Future<void> _commitProposal(CommitTarget target) async {
    final l10n = AppLocalizations.of(context)!;
    final runId = _report?['run_id'] as String?;
    final file = _proposalFile;
    if (runId == null || file == null) return;
    final String? destination;
    if (target == CommitTarget.overwrite) {
      final original = file.value.path;
      if (original.isEmpty) return;
      final ok = await showDialog<bool>(
        context: context,
        builder: (ctx) => AlertDialog(
          title: Text(l10n.skillsOverwriteConfirmTitle),
          content: Text(l10n.skillsOverwriteConfirmBody(file.value.name)),
          actions: [
            TextButton(
                onPressed: () => Navigator.of(ctx).pop(false),
                child: Text(l10n.skillsReject)),
            FilledButton(
                key: const ValueKey('skill-overwrite-confirm'),
                onPressed: () => Navigator.of(ctx).pop(true),
                child: Text(l10n.skillsOverwrite)),
          ],
        ),
      );
      if (ok != true) return;
      destination = original;
    } else {
      final resolve = widget.resolveSaveDestination ?? _defaultSaveDestination;
      destination = await resolve(_suggestedCopyName(file.value.name));
      if (destination == null) return; // dialog dismissed
    }
    if (!mounted) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final result = await widget.service.commitProposal(
        runId: runId,
        destination: destination,
        target: target,
        artifacts: [
          SkillArtifact(
              id: (_report!['status']?['approval']?['artifact_id']
                      as String?) ??
                  'a1',
              name: file.value.name,
              bytes: _attachedBytes[file.key]!),
        ],
      );
      if (!mounted) return;
      final commitError =
          (result['commit_error'] as Map?)?.cast<String, dynamic>();
      setState(() {
        _report = (result['report'] as Map).cast<String, dynamic>();
        _commit = (result['commit'] as Map?)?.cast<String, dynamic>();
        if (commitError != null) {
          _error = '${commitError['outcome']}: ${commitError['error']}';
        }
      });
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
            // English-only prose from builtin_skills.json, like the
            // Skills surface: without its own direction the trailing
            // full stop lands at the left edge in Arabic. The title
            // above is deliberately NOT given one — it is interpolated
            // into a localized Arabic template, where an embedded LTR
            // run is exactly what bidi already handles.
            Text(widget.skill.description,
                textDirection: TextDirection.ltr,
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
        // The core's own op snapshot — phase, detail and a REAL cancel —
        // as soon as the first status arrives; the static card only
        // covers the gap before it. A run the user cannot abandon is the
        // worst failure mode the sheet has: the poll behind it is
        // unbounded, so a run that stops progressing otherwise leaves the
        // sheet spinning with nothing to do but restart the app.
        ListenableBuilder(
          listenable: widget.service,
          builder: (context, _) {
            final live = widget.service.kindProgress['skill_run'];
            if (live == null) {
              return HarborOpProgress(
                key: const ValueKey('skill-progress'),
                title: l10n.skillsRunning,
                detail: _graph.id,
                icon: Icons.account_tree_outlined,
              );
            }
            return OpProgressCard(
              key: const ValueKey('skill-progress'),
              progress: live,
              service: widget.service,
            );
          },
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
      if (_commit != null) ...[
        const SizedBox(height: HarborSpace.s3),
        HarborBanner(
          key: const ValueKey('skill-committed'),
          tone: HarborBannerTone.info,
          title: _commit!['mode'] == 'new_copy'
              ? l10n.skillsSavedNewCopy
              : l10n.skillsOverwritten,
          body: l10n.skillsCommittedTo(_commit!['destination'] as String? ?? '',
              _commit!['version_id'] as String? ?? ''),
        ),
      ],
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
    final canCommit = approval['effect_class'] == 'artifact.commit' &&
        _proposalFile != null &&
        approval['proposed_output_hash'] != null;
    final canOverwrite = canCommit &&
        pickerReturnsTheUsersFile &&
        (_proposalFile?.value.path.isNotEmpty ?? false);
    return [
      const SizedBox(height: HarborSpace.s3),
      HarborSheet(
        key: const ValueKey('skill-approval'),
        title: l10n.skillsApprovalTitle,
        explanation: canCommit
            ? l10n.skillsCommitBody(
                approval['effect_class'] as String? ?? '', ops)
            : l10n.skillsApprovalBody(
                approval['effect_class'] as String? ?? '', ops),
        approveLabel: canCommit ? l10n.skillsSaveNewCopy : l10n.skillsApprove,
        denyLabel: l10n.skillsReject,
        onApprove: _busy
            ? () {}
            : (canCommit
                ? () => _commitProposal(CommitTarget.saveNewCopy)
                : () => _decide(true)),
        onDeny: _busy ? () {} : () => _decide(false),
        diffSummary: proposalDiff(l10n, t, approval, batch),
      ),
      if (canOverwrite)
        Align(
          alignment: AlignmentDirectional.centerEnd,
          child: TextButton.icon(
            key: const ValueKey('skill-overwrite'),
            onPressed:
                _busy ? null : () => _commitProposal(CommitTarget.overwrite),
            icon: const Icon(Icons.save_as_outlined, size: 18),
            label: Text(l10n.skillsOverwrite),
          ),
        ),
    ];
  }

  /// Version-bound before/after view of the proposal (production plan B2):
  /// the core's diff entries (paragraph text for DOCX, formula-or-value for
  /// XLSX) rendered with the shared [ArtifactDiffView]; the raw operation
  /// list is the fallback when the core supplied no diff.
  static Widget proposalDiff(AppLocalizations l10n, HarborTheme t,
      Map<String, dynamic> approval, Map<String, dynamic> batch) {
    final diff = (approval['diff'] as List?)?.cast<Map>() ?? const [];
    final base = approval['base_content_hash'] as String? ??
        batch['base_version_id'] as String? ??
        '';
    final proposed = approval['proposed_output_hash'] as String? ?? '';
    if (diff.isNotEmpty) {
      return ArtifactDiffView(
        key: const ValueKey('skill-proposal-diff'),
        baseVersion: base.length >= 16 ? '${base.substring(0, 16)}…' : base,
        proposedHash: proposed,
        baseLabel: l10n.skillsDiffBase,
        proposedLabel: l10n.skillsDiffProposed,
        entries: [
          for (final d in diff)
            DiffEntryVM(
              summary: '${d['location'] ?? d['target_id']} · ${d['kind']}',
              before: d['before'] as String?,
              after: d['after'] as String?,
            ),
        ],
      );
    }
    return Column(
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
            title: Text('${op['kind']} → ${op['precondition']?['target_id']}',
                style: t.text.monoOf(t.colors.ink)),
            subtitle: op['args']?['text'] != null
                ? Text(op['args']['text'] as String,
                    style: t.text.smallOf(t.colors.inkMuted),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis)
                : (op['args']?['value'] != null
                    ? Text('${op['args']['address']}: ${op['args']['value']}',
                        style: t.text.smallOf(t.colors.inkMuted))
                    : null),
          ),
      ],
    );
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
