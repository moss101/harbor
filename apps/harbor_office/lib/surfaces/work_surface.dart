import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';
import 'package:path_provider/path_provider.dart';

import '../services/file_types.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import '../services/open_intake.dart';
import '../services/preferences.dart' show RecentFile;
import 'work/deck_view.dart';
import 'work/document_view.dart';
import 'work/pdf_view.dart';
import 'save_destination.dart';
import 'work/workbook_view.dart';

/// Work Canvas (goal §22): the artifact workspace. The user must always be
/// able to determine which file, which kind, that it is a read-only
/// preview in this build, and which parts the Office Feature Matrix
/// preserves without rendering (compatibility banner). "Open file" is a
/// real picker routed through the core's qualified preview paths.
class WorkSurface extends StatefulWidget {
  const WorkSurface({super.key});

  @override
  State<WorkSurface> createState() => _WorkSurfaceState();
}

class _WorkSurfaceState extends State<WorkSurface> {
  bool _showParts = false;
  AppState? _state;

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
    super.dispose();
  }

  /// ⌘O / palette: open the picker once we are the visible surface.
  void _consumePending() {
    final state = _state;
    if (state == null || state.surface != OfficeSurface.work) return;
    if (state.pendingOpenFile) {
      state.pendingOpenFile = false;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _openFile();
      });
    }
    // A document delivered by the platform waits until the core is up
    // (a cold "Open in…" arrives before bootstrap finishes).
    final request = state.pendingOpen;
    if (request != null &&
        HarborServiceProvider.of(context).notifier != null) {
      state.pendingOpen = null;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _openDelivered(request);
      });
    }
  }

  /// Open a document the platform handed to the suite. Refusals are
  /// stated, never silent: an unsupported type names the supported ones,
  /// an unreadable handle says the system did not grant the file.
  Future<void> _openDelivered(OpenRequest request) async {
    final l10n = AppLocalizations.of(context)!;
    final messenger = ScaffoldMessenger.of(context);
    if (!request.ok) {
      messenger.showSnackBar(SnackBar(
          content: Text(request.failure == OpenFailure.unsupportedType
              ? l10n.workOpenUnsupported(request.name)
              : l10n.workOpenUnreadable(request.name))));
      return;
    }
    await _loadPath(request.path!, request.name);
  }

  /// Read [path], preview it through the core and record it in recents.
  /// A core refusal (not a real document) lands in the preview-error
  /// state; a path that cannot be read surfaces a snackbar.
  Future<void> _loadPath(String path, String name) async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final l10n = AppLocalizations.of(context)!;
    final messenger = ScaffoldMessenger.of(context);
    final List<int> bytes;
    try {
      bytes = await File(path).readAsBytes();
    } catch (_) {
      messenger.showSnackBar(SnackBar(content: Text(l10n.workRecentFailed(name))));
      return;
    }
    setState(() => _showParts = false);
    await service.loadPreviewFromBytes(bytes, name: name);
    if (mounted && service.previewError == null) _recordRecent(name, path);
  }

  /// Convert a picked Markdown file to a real .docx through the core,
  /// preview it in the canvas, and save a copy where the user chooses
  /// (desktop save picker; mobile documents directory).
  Future<void> _convertMarkdown() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final XFile? file;
    try {
      file = await openFile(
          acceptedTypeGroups: [documentTypeGroup('Markdown or PDF')]);
    } catch (_) {
      return; // picker dismissed
    }
    if (file == null || !mounted) return;
    final lower = file.name.toLowerCase();
    if (!lower.endsWith('.md') && !lower.endsWith('.pdf')) return;
    final l10n = AppLocalizations.of(context)!;
    final messenger = ScaffoldMessenger.of(context);
    final baseName = file.name.replaceFirst(RegExp(r'\.(md|pdf)\$'), '');
    String? asDeckExt;
    try {
      final List<int> bytes;
      if (lower.endsWith('.pdf')) {
        bytes = await service.convertPdfToDocx(await file.readAsBytes(),
            title: baseName);
      } else {
        // Markdown: Word document or PowerPoint deck — the user chooses.
        final asDeck = await showDialog<bool>(
          context: context,
          builder: (context) => AlertDialog(
            title: Text(l10n.workConvertTo),
            content: Text(l10n.workMdFormatBody),
            actions: [
              TextButton(
                onPressed: () => Navigator.of(context).pop(false),
                child: Text(l10n.workMdFormatWord),
              ),
              FilledButton(
                onPressed: () => Navigator.of(context).pop(true),
                child: Text(l10n.workMdFormatSlides),
              ),
            ],
          ),
        );
        if (!mounted || asDeck == null) return;
        asDeckExt = asDeck ? 'pptx' : 'docx';
        bytes = asDeck
            ? await service.markdownToPptx(await file.readAsString())
            : await service.convertMarkdownToDocx(
                await file.readAsString(),
                title: baseName);
      }
      final targetExt =
          lower.endsWith('.pdf') ? 'docx' : (asDeckExt ?? 'docx');
      await service.loadPreviewFromBytes(bytes, name: '$baseName.$targetExt');
      final dest = await _saveConvertedCopy('$baseName.$targetExt', bytes);
      if (mounted && dest != null) {
        messenger.showSnackBar(
            SnackBar(content: Text(l10n.workConvertedSaved(dest))));
      }
    } on ffi.HarborCoreException catch (e) {
      if (mounted) {
        messenger.showSnackBar(
            SnackBar(content: Text('${l10n.workConvertFailed}: ${e.message}')));
      }
    }
  }

  /// Where a converted copy lands: the user's choice on desktop, the
  /// app's documents directory on mobile (same policy as skill saves).
  Future<String?> _saveConvertedCopy(String name, List<int> bytes) async {
    String? dest;
    if (Platform.isMacOS || Platform.isWindows || Platform.isLinux) {
      final location = await getSaveLocation(suggestedName: name);
      dest = location?.path;
    } else {
      final dir = await getApplicationDocumentsDirectory();
      dest = firstFreePath(dir.path, name, (p) => File(p).existsSync());
    }
    if (dest == null) return null;
    await File(dest).writeAsBytes(bytes, flush: true);
    return dest;
  }

  /// Create a new blank workbook, preview it and offer a save location.
  Future<void> _newWorkbook() async {
    final l10n = AppLocalizations.of(context)!;
    await _createAndOffer(
      bytes: await HarborServiceProvider.of(context)
          .notifier!
          .createWorkbook(),
      name: '${l10n.workUntitledSheet}.xlsx',
      previewName: l10n.workUntitledSheet,
    );
  }

  /// Create a new document (title block only) and offer a save location.
  Future<void> _newDocument() async {
    final l10n = AppLocalizations.of(context)!;
    final name = l10n.workUntitledDoc;
    await _createAndOffer(
      bytes: await HarborServiceProvider.of(context)
          .notifier!
          .createDocument(title: name),
      name: '$name.docx',
      previewName: name,
    );
  }

  Future<void> _createAndOffer(
      {required List<int> bytes,
      required String name,
      required String previewName}) async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final l10n = AppLocalizations.of(context)!;
    final messenger = ScaffoldMessenger.of(context);
    try {
      await service.loadPreviewFromBytes(bytes, name: previewName);
      if (!mounted) return;
      final dest = await _saveConvertedCopy(name, bytes);
      if (mounted && dest != null) {
        _recordRecent(name, dest);
        // The full container path is noise on mobile: the saved NAME is
        // what the user recognizes (it is in Files / the recents list).
        final shown = dest.split(Platform.pathSeparator).last;
        messenger.showSnackBar(
            SnackBar(content: Text(l10n.workConvertedSaved(shown))));
      }
    } on ffi.HarborCoreException catch (e) {
      if (mounted) {
        messenger.showSnackBar(
            SnackBar(content: Text('${l10n.workConvertFailed}: ${e.message}')));
      }
    }
  }

  /// Print: render to PDF (documents and workbooks; PDFs print as-is)
  /// and hand it to the SYSTEM print dialog — the platform owns the
  /// rest; nothing leaves the device but the user's own print job.
  Future<void> _print() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final l10n = AppLocalizations.of(context)!;
    final messenger = ScaffoldMessenger.of(context);
    final kind = service.preview?['kind'] as String?;
    final name = service.previewName ?? 'document';
    try {
      final bytes = kind == 'pdf'
          ? service.currentWorkingBytes()
          : kind == 'workbook'
              ? await service.exportWorkbookToPdf(title: name)
              : await service.exportDocxToPdf(title: name);
      if (bytes == null) return;
      final bool sent;
      try {
        sent = await service.printPdf(bytes, name);
      } on ffi.HarborCoreException catch (e) {
        // The platform print path failed (not "no print support"): keep
        // its reason instead of reporting the printer as unavailable.
        if (mounted) {
          messenger.showSnackBar(SnackBar(
              content: Text('${l10n.workPrintUnavailable} (${e.message})')));
        }
        return;
      }
      if (!mounted) return;
      if (!sent) {
        messenger.showSnackBar(
            SnackBar(content: Text(l10n.workPrintUnavailable)));
      }
    } on ffi.HarborCoreException catch (e) {
      if (mounted) {
        messenger.showSnackBar(
            SnackBar(content: Text('${l10n.workConvertFailed}: ${e.message}')));
      }
    }
  }

  /// Export the open workbook to PDF (text-extraction level grid).
  Future<void> _exportWorkbookPdf() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final l10n = AppLocalizations.of(context)!;
    final messenger = ScaffoldMessenger.of(context);
    try {
      final bytes = await service.exportWorkbookToPdf(
          title: service.previewName ?? 'workbook');
      if (!mounted) return;
      final base = (service.previewName ?? 'workbook')
          .replaceFirst(RegExp(r'\.xlsx\$'), '');
      final dest = await _saveConvertedCopy('$base.pdf', bytes);
      if (mounted && dest != null) {
        messenger.showSnackBar(SnackBar(
            content:
                Text(l10n.workExportedPdf(dest.split(Platform.pathSeparator).last))));
      }
    } on ffi.HarborCoreException catch (e) {
      if (mounted) {
        messenger.showSnackBar(
            SnackBar(content: Text('${l10n.workConvertFailed}: ${e.message}')));
      }
    }
  }

  /// Export the open document to PDF (text-extraction level) and save a
  /// copy where the user chooses. The extraction level is the core's
  /// honest label; the app never claims layout fidelity.
  Future<void> _exportPdf() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    if (service.preview?['kind'] == 'workbook') return _exportWorkbookPdf();
    final l10n = AppLocalizations.of(context)!;
    final messenger = ScaffoldMessenger.of(context);
    try {
      final bytes = await service.exportDocxToPdf(
          title: service.previewName ?? 'document');
      if (!mounted) return;
      final base = (service.previewName ?? 'document')
          .replaceFirst(RegExp(r'\.docx\$'), '');
      final dest = await _saveConvertedCopy('$base.pdf', bytes);
      if (mounted && dest != null) {
        messenger.showSnackBar(SnackBar(
            content:
                Text(l10n.workExportedPdf(dest.split(Platform.pathSeparator).last))));
      }
    } on ffi.HarborCoreException catch (e) {
      if (mounted) {
        messenger.showSnackBar(
            SnackBar(content: Text('${l10n.workConvertFailed}: ${e.message}')));
      }
    }
  }

  /// Save the current working copy (edits included) as a new file.
  Future<void> _saveOpenCopy() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final l10n = AppLocalizations.of(context)!;
    final messenger = ScaffoldMessenger.of(context);
    final name = service.previewName;
    final bytes = service.currentWorkingBytes();
    if (name == null || bytes == null) return;
    try {
      final dest = await _saveConvertedCopy(name, bytes);
      if (!mounted) return;
      if (dest != null) {
        _recordRecent(name, dest);
        await service.markSaved();
        messenger.showSnackBar(
            SnackBar(content: Text(l10n.workSavedCopy(dest.split(Platform.pathSeparator).last))));
      }
    } on ffi.HarborCoreException catch (e) {
      if (mounted) {
        messenger.showSnackBar(
            SnackBar(content: Text('${l10n.workConvertFailed}: ${e.message}')));
      }
    }
  }

  /// Record a successfully opened/created file for the Work recents list.
  void _recordRecent(String name, String path) {
    final kind = name.toLowerCase().endsWith('.xlsx')
        ? 'workbook'
        : name.toLowerCase().endsWith('.pdf')
            ? 'pdf'
            : 'docx';
    final now = DateTime.now();
    final at =
        '${now.year}-${now.month.toString().padLeft(2, '0')}-${now.day.toString().padLeft(2, '0')}';
    AppStateScope.of(context).addRecentFile(
        RecentFile(name: name, path: path, kind: kind, at: at));
  }

  /// Reopen a recent file: bytes are read honestly (a path the system no
  /// longer grants surfaces an error, never a silent failure).
  Future<void> _openRecent(RecentFile r) => _loadPath(r.path, r.name);

  Future<void> _openFile() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null) return;
    final l10n = AppLocalizations.of(context)!;
    final groups = [
      officeTypeGroup(l10n.fileGroupDocuments),
    ];
    final XFile? file;
    try {
      file = await openFile(acceptedTypeGroups: groups);
    } on Error {
      // A misconfigured type group throws Error from Dart before any
      // picker exists. Swallowing that as "dismissed" is exactly how
      // every file entry point on iOS stayed dead: silent, and labelled
      // as the user's choice. Let it surface.
      rethrow;
    } catch (_) {
      return; // picker dismissed
    }
    if (file == null) return;
    final bytes = await file.readAsBytes();
    setState(() => _showParts = false);
    await service.loadPreviewFromBytes(bytes, name: file.name);
    if (mounted) _recordRecent(file.name, file.path);
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final preview = service?.preview;
    final name = service?.previewName;
    final wc = HarborBreakpoints.of(context);
    final compact = HarborBreakpoints.isCompact(wc);
    final canvasMinApplies = HarborBreakpoints.enforceWorkCanvasMin(wc);
    final gutter = HarborBreakpoints.gutter(wc);

    final kind = preview?['kind'] as String?;
    final data = preview?['preview'];
    final subtitle = preview == null
        ? l10n.workSubtitleEmpty
        : _subtitleFor(kind, data is Map ? data : const {}, l10n);
    final compat = preview?['compatibility'];
    final preserved = _preservedParts(kind, data, compat);

    return Column(
      children: [
        HarborSurfaceHeader(
          title: preview == null
              ? l10n.surfaceWork
              : (name ?? _kindLabel(kind, l10n)),
          titleIsIdentifier: preview != null && name != null,
          showTitleOnCompact: preview != null,
          subtitle: subtitle,
          leading: preview == null
              ? null
              : Container(
                  width: 40,
                  height: 40,
                  decoration: BoxDecoration(
                    color: t.colors.brandSoft,
                    borderRadius: BorderRadius.circular(HarborRadius.sm),
                  ),
                  child: Icon(_kindIcon(kind), size: 20, color: t.colors.brand),
                ),
          actions: [
            if (preview != null && kind != 'workbook')
              StatusBadge(
                semantic: ExecutionSemantic.hybrid,
                icon: Icons.visibility_outlined,
                label: l10n.workPreviewOnly,
                tooltip: l10n.workPreviewOnlyBody,
              ),
            if (service != null && !sp.failed && preview != null) ...[
              // Compact icon actions: the header row must never overflow
              // at compact widths (labels live in the tooltips).
              if (service.canUndo)
                IconButton(
                  tooltip: l10n.workUndo,
                  onPressed: () => service.undo(),
                  icon: const Icon(Icons.undo, size: 18),
                ),
              if (service.canRedo)
                IconButton(
                  tooltip: l10n.workRedo,
                  onPressed: () => service.redo(),
                  icon: const Icon(Icons.redo, size: 18),
                ),
              IconButton(
                tooltip: l10n.workSaveCopy,
                onPressed: service.previewLoading ? null : _saveOpenCopy,
                icon: const Icon(Icons.save_outlined, size: 18),
              ),
              if (kind == 'docx' || kind == 'workbook')
                IconButton(
                  tooltip: l10n.workExportPdf,
                  onPressed: service.previewLoading ? null : _exportPdf,
                  icon: const Icon(Icons.picture_as_pdf_outlined, size: 18),
                ),
              if (kind == 'docx' || kind == 'workbook' || kind == 'pdf')
                IconButton(
                  tooltip: l10n.workPrint,
                  onPressed: service.previewLoading ? null : _print,
                  icon: const Icon(Icons.print_outlined, size: 18),
                ),
            ],
            // The empty state carries the primary "Open file" call to
            // action; the header offers it once a file is open.
            if (service != null && !sp.failed && preview != null)
              OutlinedButton.icon(
                onPressed: service.previewLoading ? null : _openFile,
                icon: const Icon(Icons.folder_open_outlined, size: 16),
                label: Text(l10n.openFile),
              ),
            if (preview != null)
              IconButton(
                tooltip: l10n.workCloseFile,
                onPressed: service?.clearPreview,
                icon: const Icon(Icons.close),
              ),
            if (!HarborBreakpoints.lensIsPersistent(wc) && !compact)
              IconButton(
                tooltip: l10n.lensOpenTooltip,
                onPressed: () => Scaffold.maybeOf(context)?.openEndDrawer(),
                icon: const Icon(Icons.insights_outlined),
              ),
          ],
        ),
        if (preview != null && preserved.isNotEmpty)
          Padding(
            padding: EdgeInsets.fromLTRB(gutter, 0, gutter, HarborSpace.s3),
            child: HarborBanner(
              tone: HarborBannerTone.warning,
              dense: true,
              title: l10n.workCompatibilityTitle,
              body: l10n.workCompatibilityBody(preserved.length),
              action: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  TextButton(
                    onPressed: () => setState(() => _showParts = !_showParts),
                    child: Text(_showParts
                        ? l10n.workCompatibilityHide
                        : l10n.workCompatibilityShow),
                  ),
                  if (_showParts)
                    Wrap(
                      spacing: HarborSpace.s2,
                      runSpacing: HarborSpace.s1,
                      children: [
                        for (final p in preserved)
                          Tooltip(
                            message: p.$2,
                            child: HarborPill(p.$1,
                                icon: Icons.inventory_2_outlined),
                          ),
                      ],
                    ),
                ],
              ),
            ),
          ),
        Expanded(
          child: Builder(builder: (context) {
            if (sp.failed || service == null) {
              return HarborErrorState(
                title: l10n.coreDegradedTitle,
                message: l10n.coreStartFailed,
              );
            }
            if (service.previewLoading) {
              return HarborLoadingState(label: l10n.workOpening);
            }
            if (service.previewError != null) {
              return HarborErrorState(
                title: l10n.workOpenFailedTitle,
                message: l10n.workOpenFailedBody,
                technical: service.previewError,
                retryLabel: l10n.openFile,
                onRetry: _openFile,
              );
            }
            if (preview == null) {
              return HarborEmptyState(
                icon: Icons.description_outlined,
                title: l10n.workEmptyTitle,
                body: l10n.workEmptyBody,
                actionLabel: l10n.openFile,
                onAction: _openFile,
                secondaryActionLabel: l10n.workConvertMarkdown,
                onSecondaryAction: _convertMarkdown,
                footer: Column(children: [
                  Wrap(
                    spacing: HarborSpace.s2,
                    runSpacing: HarborSpace.s2,
                    alignment: WrapAlignment.center,
                    children: [
                      OutlinedButton.icon(
                        onPressed: _newWorkbook,
                        icon: const Icon(Icons.table_chart_outlined, size: 18),
                        label: Text(l10n.workNewSpreadsheet),
                      ),
                      OutlinedButton.icon(
                        onPressed: _newDocument,
                        icon: const Icon(Icons.description_outlined, size: 18),
                        label: Text(l10n.workNewDocument),
                      ),
                    ],
                  ),
                  if (AppStateScope.of(context).recentFiles.isNotEmpty) ...[
                    const SizedBox(height: HarborSpace.s4),
                    Row(children: [
                      Expanded(
                          child: Text(l10n.workRecents,
                              style: t.text.bodyStrongOf(t.colors.ink))),
                      TextButton(
                        onPressed: () =>
                            AppStateScope.of(context).clearRecentFiles(),
                        child: Text(l10n.workClearRecents),
                      ),
                    ]),
                    const SizedBox(height: HarborSpace.s2),
                    for (final r
                        in AppStateScope.of(context).recentFiles.take(4))
                      HarborListRow(
                        dense: true,
                        title: Text(r.name),
                        subtitle: r.at.isEmpty ? null : Text(r.at),
                        leading: Icon(
                            r.kind == 'workbook'
                                ? Icons.table_chart_outlined
                                : r.kind == 'pdf'
                                    ? Icons.picture_as_pdf_outlined
                                    : Icons.description_outlined,
                            size: 20),
                        onTap: () => _openRecent(r),
                      ),
                  ],
                  const SizedBox(height: HarborSpace.s3),
                  Text(l10n.workSupportedTypes,
                      style: t.text.captionOf(t.colors.inkMuted)),
                  const SizedBox(height: HarborSpace.s2),
                  const Wrap(
                    spacing: HarborSpace.s2,
                    runSpacing: HarborSpace.s2,
                    alignment: WrapAlignment.center,
                    children: [
                      HarborPill('DOCX', icon: Icons.description_outlined),
                      HarborPill('XLSX', icon: Icons.table_chart_outlined),
                      HarborPill('PPTX', icon: Icons.slideshow_outlined),
                      HarborPill('PDF', icon: Icons.picture_as_pdf_outlined),
                    ],
                  ),
                  const SizedBox(height: HarborSpace.s3),
                  Text(
                    canvasMinApplies
                        ? l10n.workCanvasRuleActive(
                            HarborLayout.workCanvasMin.toInt())
                        : l10n.canvasViewportEditing,
                    style: t.text.captionOf(t.colors.inkMuted),
                  ),
                ]),
              );
            }
            final map = data is Map ? data : const <String, dynamic>{};
            final body = switch (kind) {
              'workbook' => WorkbookView(
                  preview: map, onSheetSelected: service.switchSheet),
              'docx' => DocumentView(preview: map),
              'pdf' => PdfView(preview: map),
              _ => DeckView(preview: map),
            };
            if (service.draftRestored) {
              return Column(children: [
                HarborBanner(
                  tone: HarborBannerTone.info,
                  icon: Icons.history,
                  title: l10n.workDraftRestored(service.draftName ?? ''),
                ),
                Expanded(child: body),
              ]);
            }
            return body;
          }),
        ),
      ],
    );
  }

  static String _kindLabel(String? kind, AppLocalizations l10n) =>
      switch (kind) {
        'workbook' => l10n.workKindWorkbook,
        'docx' => l10n.workKindDocument,
        'pdf' => l10n.workKindPdf,
        _ => l10n.workKindDeck,
      };

  static IconData _kindIcon(String? kind) => switch (kind) {
        'workbook' => Icons.table_chart_outlined,
        'docx' => Icons.description_outlined,
        'pdf' => Icons.picture_as_pdf_outlined,
        _ => Icons.slideshow_outlined,
      };

  static String _subtitleFor(String? kind, Map data, AppLocalizations l10n) {
    final label = _kindLabel(kind, l10n);
    switch (kind) {
      case 'workbook':
        final sheets = (data['sheets'] as List? ?? const []).length;
        final cells = (data['cells'] as List? ?? const []).length;
        return '$label · ${l10n.sheetLabel(data['sheet'] as String? ?? '')} · ${l10n.workCells(cells)}'
            '${sheets > 1 ? ' · $sheets' : ''}';
      case 'docx':
        final paras = (data['paragraphs'] as List? ?? const []).length;
        return '$label · ${l10n.workParagraphs(paras)}';
      case 'pdf':
        final pages = (data['page_count'] as num?)?.toInt() ??
            (data['pages'] as List? ?? const []).length;
        return '$label · ${l10n.workPages(pages)}';
      default:
        final slides = (data['slides'] as List? ?? const []).length;
        final title = data['title'] as String? ?? '';
        return '$label · ${l10n.workSlides(slides)}${title.isEmpty ? '' : ' · $title'}';
    }
  }

  /// Parts preserved without rendering: the docx preview's own list plus
  /// every compatibility-report entry outside SUPPORTED_GA.
  static List<(String, String)> _preservedParts(
      String? kind, dynamic data, dynamic compat) {
    final out = <(String, String)>[];
    final seen = <String>{};
    if (data is Map) {
      for (final p in (data['preserved_parts'] as List? ?? const [])) {
        final s = p.toString();
        if (seen.add(s)) out.add((s, 'PRESERVE_ONLY'));
      }
    }
    if (compat is Map) {
      for (final e in (compat['entries'] as List? ?? const [])) {
        if (e is! Map) continue;
        final cls = e['class']?.toString() ?? '';
        if (cls == 'SupportedGa' || cls == 'SUPPORTED_GA') continue;
        final part = e['part']?.toString() ?? '';
        final reason = e['reason']?.toString() ?? cls;
        if (part.isNotEmpty && seen.add(part)) {
          out.add((part, '$cls · $reason'));
        }
      }
      for (final p in (compat['unknown_parts'] as List? ?? const [])) {
        final s = p.toString();
        if (seen.add(s)) out.add((s, 'UNKNOWN_REJECT_OR_PRESERVE_ONLY'));
      }
    }
    return out;
  }
}
