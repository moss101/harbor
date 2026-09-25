import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';

import '../services/file_types.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';

/// Outcome of routing picked files into the local knowledge index.
class IngestOutcome {
  IngestOutcome({required this.indexed, required this.failed});
  final List<String> indexed;
  final List<String> failed;
}

/// Opens the file picker and ingests the selection into the durable
/// knowledge index. Plain-text formats are read directly; docx and pdf
/// are extracted through the core's qualified preview paths (the same
/// IR the Work Canvas renders). Everything else is honestly refused —
/// nothing is indexed silently.
Future<IngestOutcome?> pickAndIngest(
  BuildContext context,
  HarborService service,
) async {
  final l10n = AppLocalizations.of(context)!;
  final group = knowledgeTypeGroup(l10n.fileGroupDocuments);
  final List<XFile> files;
  try {
    files = await openFiles(acceptedTypeGroups: [group]);
  } on Error {
    // A misconfigured type group throws Error from Dart before any
    // picker exists. Swallowing that as "dismissed" is exactly how
    // every file entry point on iOS stayed dead: silent, and labelled
    // as the user's choice. Let it surface.
    rethrow;
  } catch (_) {
    return null; // picker dismissed
  }
  if (files.isEmpty) return null;
  return ingestFiles(service, files, l10n);
}

/// Ingest already-picked files (shared by the composer attach and the
/// Knowledge surface).
Future<IngestOutcome> ingestFiles(
  HarborService service,
  List<XFile> files,
  AppLocalizations l10n,
) async {
  final outcome = IngestOutcome(indexed: [], failed: []);
  if (!service.knowledgeOpen) {
    final opened = await service.openKnowledge();
    if (!opened) {
      outcome.failed.add(l10n.knowledgeOpenNeedsModel);
      return outcome;
    }
  }
  for (final file in files) {
    final name = file.name;
    final ext = name.contains('.') ? name.split('.').last.toLowerCase() : '';
    try {
      final String text;
      if (const ['txt', 'md', 'csv', 'json', 'log'].contains(ext)) {
        text = await File(file.path).readAsString();
      } else if (ext == 'docx' || ext == 'pdf') {
        final extracted = await _extractDocument(service, file);
        if (extracted == null) {
          outcome.failed.add(name);
          continue;
        }
        text = extracted;
      } else {
        outcome.failed.add(name);
        continue;
      }
      if (text.trim().isEmpty) {
        outcome.failed.add(name);
        continue;
      }
      final result = await service.ingestSources([
        {'id': name, 'title': name, 'text': text},
      ]);
      if (result == null) {
        outcome.failed.add(name);
      } else {
        outcome.indexed.add(name);
      }
    } catch (_) {
      outcome.failed.add(name);
    }
  }
  return outcome;
}

/// docx/pdf text via the core preview IR (qualified extraction paths).
Future<String?> _extractDocument(HarborService service, XFile file) async {
  final bytes = await File(file.path).readAsBytes();
  final preview = await service.extractPreview(bytes);
  if (preview == null) return null;
  final data = preview['preview'];
  if (data is! Map) return null;
  if (preview['kind'] == 'docx') {
    final paras = (data['paragraphs'] as List? ?? const []).cast<Map>();
    return paras.map((p) => p['text'] ?? '').join('\n\n');
  }
  if (preview['kind'] == 'pdf') {
    final pages = (data['pages'] as List? ?? const []).cast<Map>();
    return pages.map((pg) => pg['text'] ?? '').join('\n\n');
  }
  return null;
}

/// Show the standard post-ingest feedback.
void showIngestFeedback(
  BuildContext context,
  IngestOutcome outcome,
  AppLocalizations l10n,
) {
  final messenger = ScaffoldMessenger.of(context);
  for (final title in outcome.indexed) {
    messenger.showSnackBar(
      SnackBar(content: Text(l10n.knowledgeSourceAdded(title))),
    );
  }
  for (final title in outcome.failed) {
    final kind = title.contains('.')
        ? title.split('.').last.toUpperCase()
        : l10n.modelsImport;
    messenger.showSnackBar(
      SnackBar(content: Text(l10n.knowledgeAttachUnsupported(kind))),
    );
  }
}
