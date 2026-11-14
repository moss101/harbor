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

final _blockStart = RegExp(r'^\s*(#{1,4}\s|[-*+•]\s|\d{1,3}[.)]\s|>|```)');

/// An answer, set as a document rather than a text dump: the question
/// becomes the title (the first line, so it also names the file and the
/// PDF's metadata), consecutive lines of one paragraph are joined (small
/// models break lines mid-sentence, which the PDF would keep as hard
/// breaks), and paragraphs, lists and headings are separated by a blank
/// line so each gets its own spacing. Light Markdown is left for the
/// core's writer, which sets headings, bullets and code.
String composeAnswerDocument(String? question, String answer) {
  final out = <String>[];
  final title = (question ?? '').replaceAll(RegExp(r'\s+'), ' ').trim();
  final hasHeading = answer.trimLeft().startsWith('#');
  if (title.isNotEmpty && !hasHeading) {
    // A very long request is cut at a word boundary to a title's length.
    var shown = title;
    if (shown.length > 120) {
      shown = shown.substring(0, 120);
      final space = shown.lastIndexOf(' ');
      if (space > 60) shown = shown.substring(0, space);
    }
    out
      ..add('# $shown')
      ..add('');
  }
  final listMark = RegExp(r'^\s*([-*+•]|\d{1,3}[.)])\s');
  var inCode = false;
  var openParagraph = false; // the last line written is a paragraph's
  var openItem = false; // the last line written is a list item's
  for (final raw in answer.replaceAll('\r\n', '\n').split('\n')) {
    final line = raw.trimRight();
    if (line.trimLeft().startsWith('```')) {
      if (!inCode && out.isNotEmpty && out.last.isNotEmpty) out.add('');
      inCode = !inCode;
      out.add(line);
      openParagraph = openItem = false;
      continue;
    }
    if (inCode) {
      out.add(line);
      continue;
    }
    if (line.trim().isEmpty) {
      if (out.isNotEmpty && out.last.isNotEmpty) out.add('');
      openParagraph = openItem = false;
      continue;
    }
    final starts = _blockStart.hasMatch(line);
    final indented = raw.startsWith(RegExp(r'\s'));
    if (!starts && (openParagraph || (openItem && indented))) {
      out[out.length - 1] = '${out.last} ${line.trim()}';
      continue;
    }
    // A new block: a heading, a list line or a paragraph. List lines stay
    // together; everything else gets a blank line before it.
    final isItem = listMark.hasMatch(line);
    final prevItem = out.isNotEmpty && listMark.hasMatch(out.last);
    if (out.isNotEmpty && out.last.isNotEmpty && !(isItem && prevItem)) {
      out.add('');
    }
    out.add(line.trim());
    openItem = isItem;
    openParagraph = !starts;
  }
  while (out.isNotEmpty && out.last.isEmpty) {
    out.removeLast();
  }
  return out.join('\n');
}

/// Save an answer drafted in chat (a CV, a letter) as a one-column text
/// PDF — the layout applicant-tracking systems parse best. The core writes
/// the PDF; the file lands where skill saves land: the user's choice on
/// desktop, Files › Harbor LM on mobile (no save picker there).
class SaveAnswerPdfButton extends StatefulWidget {
  const SaveAnswerPdfButton({super.key, required this.text, this.question});
  final String text;

  /// What was asked: the document's title.
  final String? question;

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
      final r = await service
          .textToPdf(composeAnswerDocument(widget.question, widget.text));
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
      iconSize: 20,
      onPressed: _busy ? null : _save,
      icon: const Icon(Icons.picture_as_pdf_outlined),
    );
  }
}
