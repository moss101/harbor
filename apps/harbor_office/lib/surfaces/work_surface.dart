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
    if (state == null || !state.pendingOpenFile) return;
    if (state.surface != OfficeSurface.work) return;
    state.pendingOpenFile = false;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _openFile();
    });
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
    try {
      final bytes = lower.endsWith('.pdf')
          ? await service.convertPdfToDocx(await file.readAsBytes(),
              title: baseName)
          : await service.convertMarkdownToDocx(await file.readAsString(),
              title: baseName);
      await service.loadPreviewFromBytes(bytes, name: '$baseName.docx');
      final dest = await _saveConvertedCopy('$baseName.docx', bytes);
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
            return switch (kind) {
              'workbook' => WorkbookView(preview: map),
              'docx' => DocumentView(preview: map),
              'pdf' => PdfView(preview: map),
              _ => DeckView(preview: map),
            };
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
