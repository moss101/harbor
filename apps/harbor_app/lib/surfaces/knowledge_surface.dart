import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart' show MaxLengthEnforcement;
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/file_types.dart';
import '../services/harbor_service.dart';
import '../services/video_frames.dart';
import '../widgets/ops.dart';
import '../widgets/trust.dart';
import 'knowledge_ingest.dart';
import 'memory_panel.dart';

/// Knowledge surface (UX-028/029): the durable local index with real
/// source management — add files / paste text (background ingest with
/// live progress), inspect index identity, remove sources (confirmed).
class KnowledgeSurface extends StatefulWidget {
  const KnowledgeSurface({super.key});

  @override
  State<KnowledgeSurface> createState() => _KnowledgeSurfaceState();
}

class _KnowledgeSurfaceState extends State<KnowledgeSurface> {
  bool _opening = false;

  /// The collection the source list is filtered to (null: every source).
  String? _filter;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _ensureOpen());
  }

  Future<void> _ensureOpen() async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null || service.knowledgeOpen || _opening) return;
    setState(() => _opening = true);
    await service.openKnowledge();
    if (mounted) setState(() => _opening = false);
  }

  Future<void> _addFiles() async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null || !mounted) return;
    final l10n = AppLocalizations.of(context)!;
    final outcome = await pickAndIngest(context, service);
    if (outcome == null || !mounted) return;
    showIngestFeedback(context, outcome, l10n);
  }

  /// Add an image or audio file to the index (multimodal embedding
  /// packages only). The file is embedded locally; only a vector and a
  /// caption/title are stored — never the media itself.
  Future<void> _addMedia() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null || !mounted) return;
    final l10n = AppLocalizations.of(context)!;
    final XFile? file;
    try {
      file = await openFile(
          acceptedTypeGroups: [mediaTypeGroup(l10n.fileGroupMedia)]);
    } on Error {
      // See file_types.dart: a throw from a malformed type group is a bug,
      // not a dismissal.
      rethrow;
    } catch (_) {
      return; // picker dismissed
    }
    if (file == null || !mounted) return;
    final ext =
        file.name.contains('.') ? file.name.split('.').last.toLowerCase() : '';
    final isVideo = const ['mp4', 'mov', 'm4v'].contains(ext);
    final kind = const ['wav', 'mp3', 'flac'].contains(ext) ? 'audio' : 'image';
    final messenger = ScaffoldMessenger.of(context);
    try {
      if (isVideo) {
        // The core has no video decoder: the platform samples frames
        // (about 1 per second) and the core embeds them as one item.
        final frames = await sampleVideoFrames(file.path);
        if (frames == null || frames.isEmpty) {
          // Windows / Linux sample through the user's own FFmpeg; say so
          // instead of a generic "unsupported".
          final needsFfmpeg = Platform.isWindows || Platform.isLinux;
          messenger.showSnackBar(SnackBar(
              content: Text(needsFfmpeg
                  ? l10n.knowledgeVideoNeedsFfmpeg
                  : l10n.knowledgeVideoUnsupported)));
          return;
        }
        await service.ingestVideo(frames: frames, title: file.name);
      } else {
        await service.ingestMedia(
          bytes: await File(file.path).readAsBytes(),
          kind: kind,
          title: file.name,
        );
      }
      messenger.showSnackBar(
          SnackBar(content: Text(l10n.knowledgeSourceAdded(file.name))));
    } catch (_) {
      messenger.showSnackBar(
          SnackBar(content: Text(l10n.knowledgeMediaFailed(file.name))));
    }
  }

  Future<void> _pasteText() async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null || !mounted) return;
    final l10n = AppLocalizations.of(context)!;
    final titleController = TextEditingController();
    final bodyController = TextEditingController();
    final accepted = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l10n.knowledgeAddTextTitle),
        content: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 520),
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            TextField(
              controller: titleController,
              decoration:
                  InputDecoration(hintText: l10n.knowledgeAddTextTitleHint),
            ),
            const SizedBox(height: HarborSpace.s3),
            TextField(
              controller: bodyController,
              maxLines: 8,
              decoration:
                  InputDecoration(hintText: l10n.knowledgeAddTextBodyHint),
            ),
          ]),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(l10n.cancelAction),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(l10n.knowledgeAddTextConfirm),
          ),
        ],
      ),
    );
    final text = bodyController.text;
    final explicitTitle = titleController.text.trim();
    titleController.dispose();
    bodyController.dispose();
    if (accepted != true || text.trim().isEmpty || !mounted) return;
    // The pasted source gets a durable, unique id; the user's title wins,
    // otherwise the first line stands in as the title.
    final firstLine = text.split('\n').first.trim();
    final sourceTitle = explicitTitle.isNotEmpty
        ? explicitTitle
        : (firstLine.isEmpty ? l10n.knowledgeAddTextTitle : firstLine);
    final result = await service.ingestSources([
      {
        'id': 'pasted-${HarborService.newRunId()}',
        'title': sourceTitle,
        'text': text
      },
    ]);
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(
      content: Text(result == null
          ? l10n.knowledgeIngestFailed
          : l10n.knowledgeSourceAdded(sourceTitle)),
    ));
  }

  Future<void> _remove(
      HarborService service, Map<String, dynamic> source) async {
    final l10n = AppLocalizations.of(context)!;
    final title = source['title'] as String;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l10n.knowledgeRemoveConfirmTitle(title)),
        content: Text(l10n.knowledgeRemoveConfirmBody),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(l10n.cancelAction),
          ),
          FilledButton(
            style: FilledButton.styleFrom(
              backgroundColor: HarborTheme.of(context).colors.danger,
              foregroundColor: HarborTheme.of(context).colors.onBrand,
            ),
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(l10n.knowledgeRemoveAction),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    await service.removeKnowledgeSource(source['source_id'] as String);
    if (!mounted) return;
    ScaffoldMessenger.of(context)
        .showSnackBar(SnackBar(content: Text(l10n.knowledgeRemoved(title))));
  }

  /// Assign [source] to [name], or take it out of its collection (null).
  Future<void> _setCollection(
      HarborService service, Map<String, dynamic> source, String? name) async {
    final l10n = AppLocalizations.of(context)!;
    final title = source['title'] as String;
    final ok =
        await service.setSourceCollection(source['source_id'] as String, name);
    if (!mounted) return;
    final stored = ok
        ? (service.knowledgeSources.firstWhere(
            (s) => s['source_id'] == source['source_id'],
            orElse: () => const {})['collection'] as String?)
        : null;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(
      content: Text(!ok
          ? l10n.knowledgeCollectionFailed
          : stored == null
              ? l10n.knowledgeCollectionCleared(title)
              : l10n.knowledgeCollectionMoved(title, stored)),
    ));
  }

  /// Ask for a new collection's name, then assign [source] to it.
  Future<void> _newCollection(
      HarborService service, Map<String, dynamic> source) async {
    final name = await showDialog<String>(
      context: context,
      builder: (context) => const _CollectionNameDialog(),
    );
    if (name == null || name.trim().isEmpty || !mounted) return;
    await _setCollection(service, source, name.trim());
  }

  /// The per-source collection menu: existing collections, none, or new.
  Widget _collectionMenu(HarborService service, Map<String, dynamic> source,
      List<Map<String, dynamic>> collections) {
    final l10n = AppLocalizations.of(context)!;
    final current = source['collection'] as String?;
    final id = source['source_id'] as String;
    return PopupMenuButton<String>(
      key: ValueKey('source-collection-menu-$id'),
      tooltip: l10n.knowledgeCollectionMenuTooltip,
      icon: Icon(current == null ? Icons.folder_outlined : Icons.folder,
          color: current == null ? null : HarborTheme.of(context).colors.brand),
      onSelected: (value) {
        if (value == '\u0000new') {
          _newCollection(service, source);
        } else if (value == '\u0000none') {
          _setCollection(service, source, null);
        } else {
          _setCollection(service, source, value);
        }
      },
      itemBuilder: (context) => [
        for (final c in collections)
          CheckedPopupMenuItem<String>(
            key: ValueKey('collection-item-${c['name']}'),
            value: c['name'] as String,
            checked: c['name'] == current,
            // Collection names are data.
            child: Text(c['name'] as String, overflow: TextOverflow.ellipsis),
          ),
        if (current != null)
          PopupMenuItem<String>(
            key: const ValueKey('collection-clear'),
            value: '\u0000none',
            child: Text(l10n.knowledgeCollectionNone),
          ),
        PopupMenuItem<String>(
          key: const ValueKey('collection-new'),
          value: '\u0000new',
          child: Text(l10n.knowledgeCollectionNew),
        ),
      ],
    );
  }

  /// The collections filter: All plus one chip per collection.
  Widget _collectionFilter(
      AppLocalizations l10n, List<Map<String, dynamic>> collections) {
    return Padding(
      padding: const EdgeInsets.only(bottom: HarborSpace.s2),
      child: Wrap(
        key: const ValueKey('collection-filter'),
        spacing: HarborSpace.s2,
        runSpacing: HarborSpace.s1,
        children: [
          ChoiceChip(
            key: const ValueKey('collection-filter-all'),
            label: Text(l10n.knowledgeCollectionAll),
            selected: _filter == null,
            onSelected: (_) => setState(() => _filter = null),
          ),
          for (final c in collections)
            ChoiceChip(
              key: ValueKey('collection-filter-${c['name']}'),
              avatar: const Icon(Icons.folder_outlined, size: 16),
              label: Text(
                  l10n.knowledgeCollectionChip(
                      c['name'] as String, c['source_count'] as int),
                  overflow: TextOverflow.ellipsis),
              selected: _filter == c['name'],
              onSelected: (_) => setState(() => _filter = c['name'] as String),
            ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final state = AppStateScope.maybeOf(context);
    final open = service?.knowledgeOpen == true;
    final allSources = service?.knowledgeSources ?? const [];
    final collections = service?.knowledgeCollections ?? const [];
    // A filter whose collection has emptied out falls back to everything.
    final filter =
        collections.any((c) => c['name'] == _filter) ? _filter : null;
    final sources = filter == null
        ? allSources
        : [
            for (final s in allSources)
              if (s['collection'] == filter) s
          ];
    final ingest = service?.kindProgress['ingest'];
    final totalChunks =
        allSources.fold<int>(0, (sum, s) => sum + (s['chunks'] as int));

    return Column(children: [
      HarborSurfaceHeader(
        title: l10n.surfaceKnowledge,
        subtitle: l10n.knowledgeSubtitle,
        actions: [
          if (service != null && !sp.failed) ...[
            FilledButton.icon(
              onPressed: _addFiles,
              icon: const Icon(Icons.note_add_outlined, size: 16),
              label: Text(l10n.addSources),
            ),
            if (service.knowledgeMultimodal)
              OutlinedButton.icon(
                key: const ValueKey('knowledge-add-media'),
                onPressed: open ? _addMedia : null,
                icon: const Icon(Icons.perm_media_outlined, size: 16),
                label: Text(l10n.knowledgeAddMediaAction),
              ),
            OutlinedButton.icon(
              onPressed: open ? _pasteText : null,
              icon: const Icon(Icons.content_paste_outlined, size: 16),
              label: Text(l10n.knowledgeAddTextAction),
            ),
          ],
        ],
      ),
      Expanded(
        child: Builder(builder: (context) {
          if (sp.failed || service == null) {
            return HarborErrorState(
                title: l10n.coreDegradedTitle,
                message: l10n.coreNotLoadedSkills);
          }
          if (_opening && !open) {
            return HarborLoadingState(label: l10n.knowledgeOpening);
          }
          if (!open) {
            return HarborEmptyState(
              icon: Icons.library_books_outlined,
              title: l10n.knowledgeEmptyTitle,
              body: l10n.knowledgeOpenNeedsModel,
              actionLabel: l10n.addSources,
              onAction: _addFiles,
              secondaryActionLabel:
                  state == null ? null : l10n.knowledgeGoModels,
              onSecondaryAction:
                  state == null ? null : () => state.goTo(HarborSurface.models),
            );
          }
          final indexCard = HarborCard(
            raised: true,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(children: [
                  Icon(Icons.hub_outlined, size: 18, color: t.colors.brand),
                  const SizedBox(width: HarborSpace.s2),
                  Expanded(
                    child: Text(l10n.knowledgeIndexHeading,
                        style: t.text.bodyStrongOf(t.colors.ink),
                        overflow: TextOverflow.ellipsis),
                  ),
                  const SizedBox(width: HarborSpace.s2),
                  StatusBadge(
                      semantic: ExecutionSemantic.local,
                      label: executionDisplay(service, l10n)),
                ]),
                const SizedBox(height: HarborSpace.s3),
                Row(children: [
                  Expanded(
                    child: HarborMetric(
                        value: '${allSources.length}',
                        label: l10n.knowledgeSourcesHeading,
                        icon: Icons.article_outlined),
                  ),
                  const SizedBox(width: HarborSpace.s2),
                  Expanded(
                    child: HarborMetric(
                        value: '$totalChunks',
                        label: l10n.knowledgeChunksCount(totalChunks),
                        icon: Icons.grain_outlined),
                  ),
                  const SizedBox(width: HarborSpace.s2),
                  Expanded(
                    child: HarborMetric(
                        value: '${service.knowledgeDimension}',
                        label: l10n.knowledgeDimension,
                        icon: Icons.linear_scale_outlined),
                  ),
                ]),
                const SizedBox(height: HarborSpace.s3),
                HarborKeyValue(
                    label: l10n.knowledgeEmbedding,
                    value: service.knowledgePackage ?? service.defaultEmbedder,
                    identifier: true),
                HarborKeyValue(
                    label: l10n.knowledgeBackendLabel,
                    value: switch (service.knowledgeBackend) {
                      'cpu_fallback' => l10n.knowledgeBackendCpuFallback,
                      'cpu_unverified' => l10n.knowledgeBackendCpuUnverified,
                      _ => l10n.knowledgeBackendGpu,
                    }),
                if (service.knowledgeIdentity != null)
                  HarborKeyValue(
                      label: l10n.knowledgeIdentity,
                      value: service.knowledgeIdentity!,
                      identifier: true),
              ],
            ),
          );
          final sourceList = allSources.isEmpty && ingest == null
              ? HarborEmptyState(
                  compact: true,
                  icon: Icons.note_add_outlined,
                  title: l10n.knowledgeNoSources,
                  body: l10n.knowledgeEmptyBody,
                  actionLabel: l10n.addSources,
                  onAction: _addFiles,
                )
              : HarborCard(
                  padding: const EdgeInsets.symmetric(vertical: HarborSpace.s1),
                  child: Column(children: [
                    for (final (i, s) in sources.indexed) ...[
                      if (i > 0)
                        Divider(height: 1, color: t.colors.borderSubtle),
                      HarborListRow(
                        leading: Icon(
                            (s['source_id'] as String).startsWith('media:')
                                ? Icons.perm_media_outlined
                                : Icons.article_outlined),
                        title: Text(s['title'] as String),
                        subtitle: Text(
                          '${l10n.knowledgeChunksCount(s['chunks'] as int)} · '
                          '${l10n.knowledgeSourceKb(((s['bytes'] as num?)?.toInt() ?? 0) ~/ 1024)}'
                          '${s['collection'] is String ? ' · ${s['collection']}' : ''}',
                        ),
                        trailing: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            _collectionMenu(service, s, collections),
                            IconButton(
                              tooltip: l10n.knowledgeRemoveAction,
                              icon: const Icon(Icons.delete_outline),
                              onPressed: () => _remove(service, s),
                            ),
                          ],
                        ),
                      ),
                    ],
                    if (sources.isEmpty)
                      Padding(
                        padding: const EdgeInsets.all(HarborSpace.s4),
                        child: Text(l10n.knowledgeCollectionEmpty,
                            style: t.text.captionOf(t.colors.inkMuted)),
                      ),
                  ]),
                );
          return HarborPage(
            children: [
              if (ingest != null)
                Padding(
                  padding: const EdgeInsets.only(bottom: HarborSpace.s3),
                  child: OpProgressCard(progress: ingest, service: service),
                ),
              HarborTwoColumn(
                main: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    HarborSectionHeader(
                      title: l10n.knowledgeSourcesHeading,
                      trailing: sources.isEmpty
                          ? null
                          : HarborPill(
                              l10n.knowledgeSourcesCount(sources.length)),
                    ),
                    if (collections.isNotEmpty)
                      _collectionFilter(l10n, collections),
                    sourceList,
                  ],
                ),
                aside: indexCard,
              ),
              const SizedBox(height: HarborSpace.s5),
              MemoryPanel(service: service),
            ],
          );
        }),
      ),
    ]);
  }
}

/// Asks for a collection name (free text, up to 64 characters). Owns its
/// controller so it outlives the dialog's exit animation.
class _CollectionNameDialog extends StatefulWidget {
  const _CollectionNameDialog();

  @override
  State<_CollectionNameDialog> createState() => _CollectionNameDialogState();
}

class _CollectionNameDialogState extends State<_CollectionNameDialog> {
  final _controller = TextEditingController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return AlertDialog(
      title: Text(l10n.knowledgeCollectionNewTitle),
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 360),
        child: TextField(
          key: const ValueKey('collection-name-field'),
          controller: _controller,
          autofocus: true,
          maxLength: 64,
          maxLengthEnforcement: MaxLengthEnforcement.enforced,
          buildCounter: (_,
                  {required currentLength, required isFocused, maxLength}) =>
              null,
          decoration:
              InputDecoration(hintText: l10n.knowledgeCollectionNameHint),
          onSubmitted: (v) => Navigator.of(context).pop(v),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(l10n.cancelAction),
        ),
        FilledButton(
          key: const ValueKey('collection-name-confirm'),
          onPressed: () => Navigator.of(context).pop(_controller.text),
          child: Text(l10n.knowledgeCollectionCreate),
        ),
      ],
    );
  }
}
