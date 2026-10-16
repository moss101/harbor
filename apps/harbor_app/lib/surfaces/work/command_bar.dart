import 'dart:io';

import 'package:flutter/material.dart';
import 'package:harbor_domain/harbor_domain.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

import '../../l10n/app_localizations.dart';
import '../../main.dart';
import '../../services/file_passages.dart';
import '../../services/harbor_service.dart';
import '../../services/request_router.dart';
import '../../widgets/answer.dart';
import '../../widgets/composer.dart';
import '../../widgets/route_panel.dart';
import '../skill_run.dart';

/// The Work command bar: a composer docked under the canvas while a file
/// is open, scoped to that file. Two paths, picked by the router or by
/// the person's Ask / Change toggle:
///
/// - **Ask** answers from THIS file's text only (its passages go to
///   `op.start_generate` as a `document`); citations name the paragraph,
///   page, slide or cell, and tapping one brings it into view.
/// - **Change** opens a skill's run sheet with this file already attached
///   and the request prefilled. Work itself still edits nothing: the skill
///   proposes, the person approves, the core writes; the canvas then shows
///   the saved result.
///
/// The bar is keyed to the loaded file, so a new file starts clean.
class WorkCommandBar extends StatefulWidget {
  const WorkCommandBar({super.key, required this.onCite});

  /// A citation of the open file was tapped.
  final ValueChanged<PassageTarget> onCite;

  @override
  State<WorkCommandBar> createState() => _WorkCommandBarState();
}

class _WorkCommandBarState extends State<WorkCommandBar> {
  final _controller = TextEditingController();
  final _focus = FocusNode();
  RequestMode? _mode;
  AnswerTurn? _turn;
  RouteDecision? _route;
  String? _lastRequest;
  bool _routing = false;
  bool _needsModel = false;

  @override
  void dispose() {
    _controller.dispose();
    _focus.dispose();
    super.dispose();
  }

  bool get _busy => _routing || (_turn?.busy ?? false);

  String? _chatPackage(HarborService service) =>
      chatPackageFor(service, AppStateScope.maybeOf(context)?.chatModel);

  Future<void> _submit(String text) async {
    final service = HarborServiceProvider.of(context).notifier;
    final name = service?.previewName;
    if (service == null || _busy) return;
    setState(() {
      _routing = true;
      _needsModel = false;
      _route = null;
      _lastRequest = text;
    });
    final decision = await RequestRouter(service).route(
      text,
      file: RouteFile(name: name ?? ''),
      mode: _mode,
      chatPackage: _chatPackage(service),
    );
    if (!mounted) return;
    setState(() => _routing = false);
    switch (decision) {
      case RouteAsk():
        await _ask(service, text);
      case RouteSkill(:final skill):
        setState(() {
          _turn = null;
          _route = decision;
        });
        await _openSkill(service, skill, text);
      case RouteNone():
        setState(() {
          _turn = null;
          _route = decision;
        });
    }
  }

  /// Grounded answer from the open file's passages, as a durable run.
  Future<void> _ask(HarborService service, String question) async {
    final l10n = AppLocalizations.of(context)!;
    final chat = _chatPackage(service);
    if (chat == null) {
      setState(() {
        _turn = null;
        _needsModel = true;
      });
      return;
    }
    final doc = FileDocument.fromPreview(service.previewName, service.preview);
    final runId = HarborService.newRunId();
    final turn = AnswerTurn(question, runId: runId);
    setState(() {
      _route = null;
      _turn = turn;
    });
    if (doc == null || doc.passages.isEmpty) {
      setState(() {
        turn.busy = false;
        turn.error = l10n.workNoReadableText;
      });
      return;
    }
    try {
      await service.createRun(runId);
      final answer = await service.askAboutFile(question,
          chatPackage: chat, runId: runId, document: doc);
      if (!mounted) return;
      setState(() {
        turn.answer = answer;
        turn.busy = false;
      });
    } on ffi.HarborCoreException catch (e) {
      if (!mounted) return;
      setState(() {
        turn.cancelled = e.message.toLowerCase().contains('cancel');
        turn.error = turn.cancelled ? null : e.message;
        turn.busy = false;
      });
    }
  }

  void _cancel() {
    final service = HarborServiceProvider.of(context).notifier;
    final opId = service?.kindProgress['generate']?['op_id'] as String?;
    if (opId != null) service?.cancelOp(opId);
  }

  /// The skill's own run sheet, with this file attached and the request
  /// prefilled. Nothing runs until the person presses Run there.
  Future<void> _openSkill(
      HarborService service, SkillSummary skill, String request) async {
    final bytes = service.previewSourceBytes;
    final name = service.previewName;
    if (skill.graph == null) return;
    await SkillRunSheet.show(
      context,
      skill,
      service,
      initialArtifact: bytes == null || name == null
          ? null
          : AttachedFile(name: name, bytes: bytes, path: service.previewPath),
      initialRequest: request,
      chatPackage: _chatPackage(service),
      onCommitted: (commit) => _showCommitted(service, commit),
    );
  }

  /// Open the file the approved proposal was written to, so the canvas
  /// shows the result rather than the file it started from.
  Future<void> _showCommitted(
      HarborService service, Map<String, dynamic> commit) async {
    final l10n = AppLocalizations.of(context)!;
    final messenger = ScaffoldMessenger.maybeOf(context);
    final dest = commit['destination'] as String?;
    if (dest == null || dest.isEmpty) return;
    final name = dest.split(Platform.pathSeparator).last;
    try {
      final bytes = await File(dest).readAsBytes();
      await service.loadPreviewFromBytes(bytes, name: name, path: dest);
      messenger?.showSnackBar(
          SnackBar(content: Text(l10n.workReloadedResult(name))));
    } on FileSystemException {
      messenger?.showSnackBar(SnackBar(content: Text(l10n.workReloadFailed)));
    }
  }

  void _dismiss() => setState(() {
        _turn = null;
        _route = null;
        _needsModel = false;
      });

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final service = HarborServiceProvider.of(context).notifier;
    final state = AppStateScope.maybeOf(context);
    final wc = HarborBreakpoints.of(context);
    final gutter = HarborBreakpoints.gutter(wc);
    final name = service?.previewName ?? '';
    final turn = _turn;
    final route = _route;
    final progress =
        turn?.busy == true ? service?.kindProgress['generate'] : null;

    Widget? panel;
    if (_needsModel) {
      panel = HarborBanner(
        key: const ValueKey('work-needs-model'),
        tone: HarborBannerTone.info,
        dense: true,
        icon: Icons.memory_outlined,
        title: l10n.askModelPicker,
        body: l10n.workNeedsChatModel,
        action: state == null
            ? null
            : TextButton(
                onPressed: () => state.goTo(HarborSurface.models),
                child: Text(l10n.surfaceModels),
              ),
      );
    } else if (turn != null) {
      panel = AnswerTurnView(
        key: const ValueKey('work-answer'),
        turn: turn,
        progress: progress,
        onCancel: _cancel,
        citationLabel: (c) => fileCitationLabel(c, l10n),
        onCitationTap: (c) {
          final target = PassageTarget.parse(c['passage_id'] as String?);
          if (target != null) widget.onCite(target);
        },
      );
    } else if (route != null) {
      final svc = service;
      panel = RoutePanel(
        decision: route,
        onPick: (skill) {
          if (svc == null) return;
          _openSkill(svc, skill, _lastRequest ?? '');
        },
        onBrowse: state == null ? null : () => state.goTo(HarborSurface.skills),
        onDismiss: _dismiss,
      );
    }

    // The canvas stays the main thing: the answer takes at most about a
    // third of a phone screen (it scrolls; it can be dismissed).
    final maxPanel = MediaQuery.sizeOf(context).height *
        (HarborBreakpoints.isCompact(wc) ? 0.34 : 0.42);
    return Material(
      color: t.colors.canvas,
      child: SafeArea(
        top: false,
        child: Padding(
          padding: EdgeInsets.fromLTRB(gutter, 0, gutter, HarborSpace.s3),
          child: Center(
            child: ConstrainedBox(
              constraints:
                  const BoxConstraints(maxWidth: HarborLayout.readingMax + 80),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (panel != null)
                    Padding(
                      padding: const EdgeInsets.only(bottom: HarborSpace.s2),
                      child: ConstrainedBox(
                        constraints: BoxConstraints(maxHeight: maxPanel),
                        child: Stack(children: [
                          SingleChildScrollView(child: panel),
                          if (turn != null && !_busy)
                            PositionedDirectional(
                              top: 0,
                              start: 0,
                              child: IconButton(
                                key: const ValueKey('work-answer-dismiss'),
                                tooltip: l10n.answerDismiss,
                                visualDensity: VisualDensity.compact,
                                onPressed: _dismiss,
                                icon: const Icon(Icons.close, size: 18),
                              ),
                            ),
                        ]),
                      ),
                    ),
                  HarborComposer(
                    key: const ValueKey('work-command-bar'),
                    controller: _controller,
                    focusNode: _focus,
                    elevated: false,
                    hint: switch (_mode) {
                      RequestMode.ask => l10n.workCommandHintAsk,
                      RequestMode.change => l10n.workCommandHintChange,
                      null => l10n.workCommandHintAuto,
                    },
                    scope: ComposerScope.thisFile(name),
                    mode: _mode,
                    onModeChanged: (m) => setState(() => _mode = m),
                    busy: _busy,
                    onCancel: turn?.busy == true ? _cancel : null,
                    onSubmit: _submit,
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
