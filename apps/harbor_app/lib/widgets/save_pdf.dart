import 'dart:convert';
import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:path_provider/path_provider.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';
import '../surfaces/skill_run.dart' show firstFreePath;

/// `Mohsin Ali — CV` → `Mohsin Ali — CV`: a file name from a document
/// title, without the characters file systems refuse.
String pdfFileStem(String? title) {
  final cleaned = (title ?? '')
      .replaceAll(RegExp(r'[\\/:*?"<>|\x00-\x1f]'), ' ')
      .replaceAll(RegExp(r'\s+'), ' ')
      .trim();
  if (cleaned.isEmpty) return 'Harbor answer';
  return cleaned.length <= 60 ? cleaned : cleaned.substring(0, 60).trim();
}

/// Save an answer drafted in chat (a CV, a letter) as a one-column text
/// PDF — the layout applicant-tracking systems parse best. The core writes
/// the PDF; the file lands where skill saves land: the user's choice on
/// desktop, Files › Harbor LM on mobile (no save picker there).
class SaveAnswerPdfButton extends StatefulWidget {
  const SaveAnswerPdfButton({super.key, required this.text});
  final String text;

  @override
  State<SaveAnswerPdfButton> createState() => _SaveAnswerPdfButtonState();
}

class _SaveAnswerPdfButtonState extends State<SaveAnswerPdfButton> {
  bool _busy = false;

  Future<void> _save() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final l10n = AppLocalizations.of(context)!;
    final messenger = ScaffoldMessenger.of(context);
    setState(() => _busy = true);
    try {
      final r = await service.textToPdf(widget.text);
      final name = '${pdfFileStem(r['title'] as String?)}.pdf';
      final desktop =
          Platform.isMacOS || Platform.isWindows || Platform.isLinux;
      String? dest;
      if (desktop) {
        dest = (await getSaveLocation(suggestedName: name))?.path;
      } else {
        final dir = await getApplicationDocumentsDirectory();
        dest = firstFreePath(dir.path, name, (p) => File(p).existsSync());
      }
      if (dest == null) return;
      await File(dest)
          .writeAsBytes(base64Decode(r['data_b64'] as String), flush: true);
      final shown = desktop
          ? dest
          : l10n.answerPdfInFiles(dest.split(Platform.pathSeparator).last);
      final missing = (r['unsupported_chars'] as num?)?.toInt() ?? 0;
      messenger.showSnackBar(SnackBar(
          content: Text(missing > 0
              ? l10n.answerPdfSavedPartial(shown, missing)
              : l10n.answerPdfSaved(shown))));
    } on ffi.HarborCoreException catch (e) {
      messenger.showSnackBar(
          SnackBar(content: Text('${l10n.answerPdfFailed}: ${e.message}')));
    } on FileSystemException catch (e) {
      messenger.showSnackBar(
          SnackBar(content: Text('${l10n.answerPdfFailed}: ${e.message}')));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return IconButton(
      key: const ValueKey('answer-save-pdf'),
      tooltip: l10n.answerSavePdf,
      iconSize: 18,
      visualDensity: VisualDensity.compact,
      onPressed: _busy ? null : _save,
      icon: const Icon(Icons.picture_as_pdf_outlined),
    );
  }
}
