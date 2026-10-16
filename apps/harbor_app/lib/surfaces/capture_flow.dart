import 'dart:async';
import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/capture.dart';
import '../services/file_types.dart';
import '../services/harbor_service.dart';

/// Scan / voice entry points of the composer (production plan T3). Both end
/// in a review dialog and return TEXT for the caller to put in the composer;
/// neither ever submits anything. The text came from a model or an OS engine,
/// so the person reads it (and the engine and low-confidence pages) first.

/// The label for an action kind the decision layer named.
String intentLabel(String id, AppLocalizations l10n) => switch (id) {
      'run_skill' => l10n.intentRunSkill,
      'search_documents' => l10n.intentSearchDocuments,
      'remember_fact' => l10n.intentRememberFact,
      'recall_memory' => l10n.intentRecallMemory,
      'schedule_goal' => l10n.intentScheduleGoal,
      'read_scan' => l10n.intentReadScan,
      'transcribe_audio' => l10n.intentTranscribeAudio,
      'general_chat' => l10n.intentGeneralChat,
      _ => id,
    };

/// The suggestion line for [text], or null (abstained, no index, unknown).
Future<String?> _suggestionFor(
    HarborService service, String text, AppLocalizations l10n) async {
  final d = await service.suggestIntent(text);
  if (d == null || d['abstained'] == true) return null;
  final ranked = (d['ranked'] as List?) ?? const [];
  if (ranked.isEmpty) return null;
  final id = (ranked.first as Map)['id'] as String;
  return l10n.captureSuggestion(intentLabel(id, l10n));
}

enum _Source { file, camera, record }

Future<_Source?> _chooseSource(BuildContext context,
    {required String fileLabel,
    required String liveLabel,
    required IconData liveIcon,
    required _Source live}) {
  return showModalBottomSheet<_Source>(
    context: context,
    showDragHandle: true,
    builder: (ctx) => SafeArea(
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        ListTile(
          key: const ValueKey('capture-source-file'),
          leading: const Icon(Icons.folder_open_outlined),
          title: Text(fileLabel),
          onTap: () => Navigator.pop(ctx, _Source.file),
        ),
        ListTile(
          key: const ValueKey('capture-source-live'),
          leading: Icon(liveIcon),
          title: Text(liveLabel),
          onTap: () => Navigator.pop(ctx, live),
        ),
      ]),
    ),
  );
}

/// Run [start] behind a cancellable "working" dialog. Returns null when the
/// person cancelled; rethrows the core's error otherwise.
Future<Map<String, dynamic>?> _runCancellable(
  BuildContext context,
  HarborService service,
  String message,
  Future<Map<String, dynamic>> Function(void Function(String) onStarted)
      start, {
  Future<void> Function()? onCancel,
}) async {
  final l10n = AppLocalizations.of(context)!;
  String? opId;
  var cancelled = false;
  final navigator = Navigator.of(context, rootNavigator: true);
  var open = true;
  unawaited(showDialog<void>(
    context: context,
    barrierDismissible: false,
    builder: (ctx) => AlertDialog(
      content: Row(children: [
        const SizedBox(
            width: 24,
            height: 24,
            child: CircularProgressIndicator(strokeWidth: 2)),
        const SizedBox(width: 16),
        Expanded(child: Text(message)),
      ]),
      actions: [
        TextButton(
          key: const ValueKey('capture-cancel'),
          onPressed: () {
            cancelled = true;
            final id = opId;
            if (id != null) service.cancelOp(id);
            onCancel?.call();
            if (open) {
              open = false;
              navigator.pop();
            }
          },
          child: Text(l10n.captureCancel),
        ),
      ],
    ),
  ).then((_) => open = false));
  try {
    return await start((id) => opId = id);
  } catch (e) {
    if (cancelled) return null;
    rethrow;
  } finally {
    if (open) {
      open = false;
      navigator.pop();
    }
    if (cancelled) {
      // Fall through: the caller sees null.
    }
  }
}

Future<void> _showError(BuildContext context, Object error) {
  final l10n = AppLocalizations.of(context)!;
  final message = error is CapturePermissionDenied
      ? l10n.captureMicDenied
      : error is CaptureUnresponsive
          ? l10n.captureMicUnresponsive
          : l10n.captureFailed(error
              .toString()
              .replaceFirst(RegExp(r'^HarborCoreException:?\s*'), ''));
  ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(message)));
  return Future.value();
}

/// Read a scan or photo. Returns the reviewed text, or null.
Future<String?> showScanFlow(
  BuildContext context,
  HarborService service, {
  HarborCapture? capture,
}) async {
  final l10n = AppLocalizations.of(context)!;
  final cap = capture ?? HarborCapture();
  final support = await cap.support();
  if (!context.mounted) return null;
  final source = support.photo
      ? await _chooseSource(context,
          fileLabel: l10n.captureChooseFile,
          liveLabel: l10n.captureTakePhoto,
          liveIcon: Icons.photo_camera_outlined,
          live: _Source.camera)
      : _Source.file;
  if (source == null || !context.mounted) return null;

  List<int> bytes;
  String name;
  try {
    if (source == _Source.camera) {
      final path = await cap.takePhoto();
      if (path == null) return null;
      bytes = await File(path).readAsBytes();
      name = 'photo.jpg';
      unawaited(File(path).delete().catchError((_) => File(path)));
    } else {
      final XFile? file;
      try {
        file = await openFile(
            acceptedTypeGroups: [scanTypeGroup(l10n.fileGroupScans)]);
      } on Error {
        rethrow; // a misconfigured group must surface, never read as a dismissal
      } catch (_) {
        return null; // dismissed
      }
      if (file == null) return null;
      bytes = await file.readAsBytes();
      name = file.name;
    }
  } catch (e) {
    if (context.mounted) await _showError(context, e);
    return null;
  }
  if (!context.mounted) return null;

  final Map<String, dynamic>? result;
  try {
    result = await _runCancellable(context, service, l10n.captureReading,
        (onStarted) => service.ocrDocument(bytes, name, onStarted: onStarted));
  } catch (e) {
    if (context.mounted) await _showError(context, e);
    return null;
  }
  if (result == null || !context.mounted) return null;
  final text = (result['text'] as String? ?? '').trim();
  if (text.isEmpty) {
    ScaffoldMessenger.of(context)
        .showSnackBar(SnackBar(content: Text(l10n.captureEmptyResult)));
    return null;
  }
  final check = ((result['check_pages'] as List?) ?? const [])
      .map((e) => e.toString())
      .toList();
  final engine = result['engine'] as String?;
  final suggestion = await _suggestionFor(service, text, l10n);
  if (!context.mounted) return null;
  return showReviewDialog(
    context,
    title: l10n.captureReviewScanTitle,
    text: text,
    engine: engine,
    checkPages: check,
    suggestion: suggestion,
  );
}

/// Dictate (microphone) or transcribe a recording. Returns the reviewed text.
Future<String?> showVoiceFlow(
  BuildContext context,
  HarborService service, {
  HarborCapture? capture,
}) async {
  final l10n = AppLocalizations.of(context)!;
  final cap = capture ?? HarborCapture();
  final support = await cap.support();
  if (!context.mounted) return null;
  final source = support.record
      ? await _chooseSource(context,
          fileLabel: l10n.captureChooseRecording,
          liveLabel: l10n.captureRecordNow,
          liveIcon: Icons.mic_none,
          live: _Source.record)
      : _Source.file;
  if (source == null || !context.mounted) return null;

  final locale = Localizations.localeOf(context);
  final localeId = switch (locale.languageCode) {
    'ar' => 'ar_SA',
    'fr' => 'fr_FR',
    'de' => 'de_DE',
    'es' => 'es_ES',
    _ => 'en_US',
  };

  List<int> bytes;
  String name;
  String? path;
  var ownsFile = false; // a recording made here is deleted when we are done
  try {
    if (source == _Source.record) {
      path = await _recordDialog(context, cap);
      if (path == null) return null;
      ownsFile = true;
      bytes = await File(path).readAsBytes();
      name = 'dictation.${path.split('.').last}';
    } else {
      final XFile? file;
      try {
        file = await openFile(
            acceptedTypeGroups: [audioTypeGroup(l10n.fileGroupAudio)]);
      } on Error {
        rethrow;
      } catch (_) {
        return null;
      }
      if (file == null) return null;
      path = file.path;
      bytes = await file.readAsBytes();
      name = file.name;
    }
  } catch (e) {
    if (context.mounted) await _showError(context, e);
    return null;
  }
  if (!context.mounted) {
    if (ownsFile) unawaited(File(path).delete().catchError((_) => File(path!)));
    return null;
  }

  // The core's own speech engine (Apple) when it has one; otherwise the
  // platform's on-device recognizer (Android) for recordings it can read.
  final coreSpeech =
      (await service.engineStatus())['speech']?['available'] == true;
  final native = !coreSpeech && support.transcribe;
  if (!context.mounted) {
    if (ownsFile) unawaited(File(path).delete().catchError((_) => File(path!)));
    return null;
  }
  final Map<String, dynamic>? result;
  try {
    result = await _runCancellable(
      context,
      service,
      l10n.captureTranscribing,
      (onStarted) => native
          ? cap.transcribe(path!, localeId)
          : service.transcribeAudio(bytes, name,
              locale: localeId, onStarted: onStarted),
      onCancel: native ? cap.transcribeCancel : null,
    );
  } catch (e) {
    if (context.mounted) await _showError(context, e);
    return null;
  } finally {
    if (ownsFile) unawaited(File(path).delete().catchError((_) => File(path!)));
  }
  if (result == null || !context.mounted) return null;
  final text = (result['text'] as String? ?? '').trim();
  if (text.isEmpty) {
    ScaffoldMessenger.of(context)
        .showSnackBar(SnackBar(content: Text(l10n.captureEmptyResult)));
    return null;
  }
  final suggestion = await _suggestionFor(service, text, l10n);
  if (!context.mounted) return null;
  return showReviewDialog(
    context,
    title: l10n.captureReviewVoiceTitle,
    text: text,
    engine: result['engine'] as String?,
    checkPages: const [],
    suggestion: suggestion,
  );
}

/// Record until the person presses stop; returns the file path or null.
Future<String?> _recordDialog(BuildContext context, HarborCapture cap) async {
  await cap.recordStart();
  if (!context.mounted) {
    await cap.recordCancel();
    return null;
  }
  final l10n = AppLocalizations.of(context)!;
  final stopped = await showDialog<bool>(
    context: context,
    barrierDismissible: false,
    builder: (ctx) => _RecordingDialog(l10n: l10n),
  );
  if (stopped != true) {
    await cap.recordCancel();
    return null;
  }
  return cap.recordStop();
}

class _RecordingDialog extends StatefulWidget {
  const _RecordingDialog({required this.l10n});
  final AppLocalizations l10n;
  @override
  State<_RecordingDialog> createState() => _RecordingDialogState();
}

class _RecordingDialogState extends State<_RecordingDialog> {
  final _watch = Stopwatch()..start();
  Timer? _tick;

  @override
  void initState() {
    super.initState();
    _tick = Timer.periodic(
        const Duration(seconds: 1), (_) => mounted ? setState(() {}) : null);
  }

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final s = _watch.elapsed.inSeconds;
    final clock =
        '${(s ~/ 60).toString().padLeft(2, '0')}:${(s % 60).toString().padLeft(2, '0')}';
    return AlertDialog(
      title: Row(children: [
        const Icon(Icons.fiber_manual_record, color: Colors.red, size: 16),
        const SizedBox(width: 8),
        Text(widget.l10n.captureRecording),
      ]),
      content: Text(clock, style: Theme.of(context).textTheme.headlineMedium),
      actions: [
        TextButton(
          key: const ValueKey('capture-record-cancel'),
          onPressed: () => Navigator.pop(context, false),
          child: Text(widget.l10n.captureCancel),
        ),
        FilledButton(
          key: const ValueKey('capture-record-stop'),
          onPressed: () => Navigator.pop(context, true),
          child: Text(widget.l10n.captureStop),
        ),
      ],
    );
  }
}

/// The review step: editable text, the engine that produced it, pages worth
/// checking by eye, and (recommend-only) what kind of request it looks like.
/// Returns the edited text for "Use as input", null for discard.
Future<String?> showReviewDialog(
  BuildContext context, {
  required String title,
  required String text,
  String? engine,
  List<String> checkPages = const [],
  String? suggestion,
}) {
  return showDialog<String>(
    context: context,
    builder: (ctx) => _ReviewDialog(
      title: title,
      text: text,
      engine: engine,
      checkPages: checkPages,
      suggestion: suggestion,
    ),
  );
}

class _ReviewDialog extends StatefulWidget {
  const _ReviewDialog({
    required this.title,
    required this.text,
    required this.engine,
    required this.checkPages,
    required this.suggestion,
  });
  final String title;
  final String text;
  final String? engine;
  final List<String> checkPages;
  final String? suggestion;
  @override
  State<_ReviewDialog> createState() => _ReviewDialogState();
}

class _ReviewDialogState extends State<_ReviewDialog> {
  late final TextEditingController _c =
      TextEditingController(text: widget.text);

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return AlertDialog(
      title: Text(widget.title),
      content: SizedBox(
        width: 520,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(l10n.captureReviewBody,
                  style: t.text.smallOf(t.colors.inkMuted)),
              const SizedBox(height: HarborSpace.s2),
              if (widget.engine != null)
                Text(l10n.captureEngine(widget.engine!),
                    style: t.text.captionOf(t.colors.inkMuted)),
              if (widget.checkPages.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.only(top: HarborSpace.s1),
                  child: Text(
                      l10n.captureCheckPages(widget.checkPages.join(', ')),
                      key: const ValueKey('capture-check-pages'),
                      style: t.text.smallOf(t.colors.statusDangerText)),
                ),
              if (widget.suggestion != null)
                Padding(
                  padding: const EdgeInsets.only(top: HarborSpace.s1),
                  child: Text(widget.suggestion!,
                      key: const ValueKey('capture-suggestion'),
                      style: t.text.smallOf(t.colors.ink)),
                ),
              const SizedBox(height: HarborSpace.s3),
              TextField(
                key: const ValueKey('capture-review-text'),
                controller: _c,
                minLines: 4,
                maxLines: 14,
                decoration: const InputDecoration(border: OutlineInputBorder()),
              ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          key: const ValueKey('capture-discard'),
          onPressed: () => Navigator.pop(context),
          child: Text(l10n.captureDiscard),
        ),
        FilledButton(
          key: const ValueKey('capture-use'),
          onPressed: () => Navigator.pop(context, _c.text.trim()),
          child: Text(l10n.captureUseAsInput),
        ),
      ],
    );
  }
}
