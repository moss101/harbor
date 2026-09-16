import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import '../widgets/ops.dart';
import '../widgets/trust.dart';
import 'knowledge_ingest.dart';

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

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final state = AppStateScope.maybeOf(context);
    final open = service?.knowledgeOpen == true;
    final sources = service?.knowledgeSources ?? const [];
    final ingest = service?.kindProgress['ingest'];
    final totalChunks =
        sources.fold<int>(0, (sum, s) => sum + (s['chunks'] as int));

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
                  Text(l10n.knowledgeIndexHeading,
                      style: t.text.bodyStrongOf(t.colors.ink)),
                  const Spacer(),
                  StatusBadge(
                      semantic: ExecutionSemantic.local,
                      label: executionDisplay(service, l10n)),
                ]),
                const SizedBox(height: HarborSpace.s3),
                Row(children: [
                  Expanded(
                    child: HarborMetric(
                        value: '${sources.length}',
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
                    value: 'bge-small-en-v1.5',
                    identifier: true),
                if (service.knowledgeIdentity != null)
                  HarborKeyValue(
                      label: l10n.knowledgeIdentity,
                      value: service.knowledgeIdentity!,
                      identifier: true),
              ],
            ),
          );
          final sourceList = sources.isEmpty && ingest == null
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
                        leading: const Icon(Icons.article_outlined),
                        title: Text(s['title'] as String),
                        subtitle: Text(
                          '${l10n.knowledgeChunksCount(s['chunks'] as int)} · '
                          '${l10n.knowledgeSourceKb(((s['bytes'] as num?)?.toInt() ?? 0) ~/ 1024)}',
                        ),
                        trailing: IconButton(
                          tooltip: l10n.knowledgeRemoveAction,
                          icon: const Icon(Icons.delete_outline),
                          onPressed: () => _remove(service, s),
                        ),
                      ),
                    ],
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
                    sourceList,
                  ],
                ),
                aside: indexCard,
              ),
            ],
          );
        }),
      ),
    ]);
  }
}
